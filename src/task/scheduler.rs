
































#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::pmm::{self, PAGE_SIZE};
use crate::sync::{self, IrqMutex};
use crate::{idt, pit, vmm};
use crate::{log_fail, log_ok};




const STACK_REGION: u64 = 0xffff_ffff_a000_0000;


const SLOT_SIZE: u64 = 0x2_0000; 
const MAX_SLOTS: usize = 512;
const STACK_PAGES: u64 = 16; 

extern "C" {
    
    
    
    fn switch_context(
        old_rsp: *mut u64,
        new_rsp: u64,
        old_fpu: *mut crate::fpu::FpuState,
        new_fpu: *const crate::fpu::FpuState,
    );
    
    fn thread_trampoline();
}

core::arch::global_asm!(
    r#"
.section .text

.global switch_context
switch_context:
    fxsave64 [rdx]
    fxrstor64 [rcx]
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

.global thread_trampoline
thread_trampoline:
    mov rdi, r12
    call thread_start
    ud2
"#
);

struct Thread {
    id: u64,
    name: &'static str,
    
    rsp: u64,
    
    
    stack_slot: Option<usize>,
    
    wake_at: u64,
    is_idle: bool,
    
    
    
    
    aspace: Option<vmm::AddressSpace>,
    
    user_entry: u64,
    user_rsp: u64,
    
    fpu: crate::fpu::FpuState,
}



pub struct CrashContext {
    pub id: u64,
    pub thread_name: &'static str,
    pub user_process: bool,
    pub stack_low: u64,
    pub stack_high: u64,
}

struct Scheduler {
    current: Option<Box<Thread>>,
    ready: VecDeque<Box<Thread>>,
    sleeping: Vec<Box<Thread>>,
    idle: Option<Box<Thread>>,
    zombies: Vec<Box<Thread>>,
    free_slots: Vec<usize>,
    next_slot: usize,
    next_id: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Scheduler {
            current: None,
            ready: VecDeque::new(),
            sleeping: Vec::new(),
            idle: None,
            zombies: Vec::new(),
            free_slots: Vec::new(),
            next_slot: 0,
            next_id: 1, 
        }
    }
}

static SCHED: IrqMutex<Scheduler> = IrqMutex::new(Scheduler::new());
static READY: AtomicBool = AtomicBool::new(false);


const TIME_SLICE_MS: u64 = 10;


static SLICE_TICKS: AtomicU64 = AtomicU64::new(0);

static NEED_RESCHED: AtomicBool = AtomicBool::new(false);

static PREEMPTIONS: AtomicU64 = AtomicU64::new(0);

enum Next<'a> {
    Yield,
    Sleep(u64),
    Exit,
    
    
    Block(&'a IrqMutex<VecDeque<Box<Thread>>>),
}

fn slot_base(slot: usize) -> u64 {
    STACK_REGION + slot as u64 * SLOT_SIZE
}



fn unmap_stack(base: u64, pages: u64) {
    for i in 0..pages {
        let virt = base + (1 + i) * PAGE_SIZE;
        if let Some(phys) = vmm::unmap_page(virt) {
            pmm::free_frame(phys);
        }
    }
}


fn create_thread(name: &'static str, entry: fn(), is_idle: bool) -> Option<Box<Thread>> {
    let (slot, id) = {
        let mut guard = SCHED.lock();
        let s = &mut *guard;
        let slot = if let Some(i) = s.free_slots.pop() {
            i
        } else if s.next_slot < MAX_SLOTS {
            let i = s.next_slot;
            s.next_slot += 1;
            i
        } else {
            return None;
        };
        let id = s.next_id;
        s.next_id += 1;
        (slot, id)
    };

    let base = slot_base(slot);
    for i in 0..STACK_PAGES {
        let Some(phys) = pmm::alloc_frame() else {
            unmap_stack(base, i);
            SCHED.lock().free_slots.push(slot);
            return None;
        };
        vmm::map_page(base + (1 + i) * PAGE_SIZE, phys, vmm::KERNEL_RW);
    }

    
    
    
    
    
    let stack_top = base + (1 + STACK_PAGES) * PAGE_SIZE;
    let rsp = stack_top - 7 * 8;
    unsafe {
        let frame = rsp as *mut u64;
        for k in 0..7 {
            frame.add(k).write(0);
        }
        frame.add(3).write(entry as usize as u64); 
        frame
            .add(6)
            .write(thread_trampoline as *const () as usize as u64); 
    }

    Some(Box::new(Thread {
        id,
        name,
        rsp,
        stack_slot: Some(slot),
        wake_at: 0,
        is_idle,
        aspace: None,
        user_entry: 0,
        user_rsp: 0,
        fpu: crate::fpu::default_state(),
    }))
}






fn free_thread(t: Box<Thread>) {
    if let Some(slot) = t.stack_slot {
        unmap_stack(slot_base(slot), STACK_PAGES);
        SCHED.lock().free_slots.push(slot);
    }
    drop(t);
}



fn finish_switch() {
    let dead = {
        let mut guard = SCHED.lock();
        core::mem::take(&mut guard.zombies)
    };
    for t in dead {
        free_thread(t);
    }
}

fn wake_sleepers(s: &mut Scheduler) {
    if s.sleeping.is_empty() {
        return;
    }
    let now = pit::uptime_ms();
    let mut i = 0;
    while i < s.sleeping.len() {
        if s.sleeping[i].wake_at <= now {
            let t = s.sleeping.swap_remove(i);
            s.ready.push_back(t);
        } else {
            i += 1;
        }
    }
}




fn reschedule(how: Next<'_>) {
    let saved = sync::irq_save();

    
    SLICE_TICKS.store(0, Ordering::Relaxed);
    NEED_RESCHED.store(false, Ordering::Relaxed);

    let switch = {
        let mut guard = SCHED.lock();
        let s = &mut *guard;
        wake_sleepers(s);

        let mut cur = s.current.take().expect("scheduler: no current thread");
        let cur_ptr: *mut Thread = &mut *cur;
        let cur_is_idle = cur.is_idle;

        match how {
            Next::Yield => {
                if cur_is_idle {
                    s.idle = Some(cur);
                } else {
                    s.ready.push_back(cur);
                }
            }
            Next::Sleep(wake_at) => {
                cur.wake_at = wake_at;
                s.sleeping.push(cur);
            }
            Next::Exit => s.zombies.push(cur),
            Next::Block(queue) => {
                
                
                
                
                queue.lock().push_back(cur);
            }
        }

        let mut next = match s.ready.pop_front() {
            Some(t) => t,
            None => s.idle.take().expect("scheduler: no idle thread"),
        };
        let next_ptr: *mut Thread = &mut *next;
        let next_rsp = next.rsp;
        
        
        
        
        
        
        
        if let Some(slot) = next.stack_slot {
            crate::gdt::set_kernel_stack(slot_base(slot) + (1 + STACK_PAGES) * PAGE_SIZE);
        }
        
        
        
        
        
        let want_cr3 = match next.aspace.as_ref() {
            Some(a) => a.phys(),
            None => vmm::kernel_cr3(),
        };
        if want_cr3 != 0 && want_cr3 != vmm::current_cr3() {
            unsafe { vmm::write_cr3(want_cr3) };
        }
        s.current = Some(next);

        if cur_ptr == next_ptr {
            
            None
        } else {
            Some((
                unsafe { core::ptr::addr_of_mut!((*cur_ptr).rsp) },
                next_rsp,
                unsafe { core::ptr::addr_of_mut!((*cur_ptr).fpu) },
                unsafe { core::ptr::addr_of!((*next_ptr).fpu) },
            ))
        }
    };

    if let Some((old_rsp, new_rsp, old_fpu, new_fpu)) = switch {
        unsafe { switch_context(old_rsp, new_rsp, old_fpu, new_fpu) };
        
        finish_switch();
    }

    sync::irq_restore(saved);
}


#[no_mangle]
extern "C" fn thread_start(entry: usize) -> ! {
    finish_switch();
    
    idt::enable_interrupts();
    let f: fn() = unsafe { core::mem::transmute(entry) };
    f();
    exit()
}

fn idle_main() {
    loop {
        
        
        unsafe { core::arch::asm!("sti", "hlt", options(nomem, nostack)) };
        yield_now();
    }
}




pub fn init() {
    let Some(idle) = create_thread("idle", idle_main, true) else {
        log_fail!("Scheduler", "Init", "Could not create the idle thread");
        return;
    };
    let boot = Box::new(Thread {
        id: 0,
        name: "boot",
        rsp: 0,
        stack_slot: None,
        wake_at: 0,
        is_idle: false,
        aspace: None,
        user_entry: 0,
        user_rsp: 0,
        fpu: crate::fpu::default_state(),
    });
    {
        let mut guard = SCHED.lock();
        guard.current = Some(boot);
        guard.idle = Some(idle);
    }
    READY.store(true, Ordering::SeqCst);

    log_ok!(
        "Scheduler",
        "Init",
        "Preemptive round-robin scheduler online ({} ms time slice, {} KiB stacks with guard pages, idle thread ready)",
        TIME_SLICE_MS,
        STACK_PAGES * PAGE_SIZE / 1024
    );
}


pub fn spawn(name: &'static str, entry: fn()) -> Option<u64> {
    let t = create_thread(name, entry, false)?;
    let id = t.id;
    SCHED.lock().ready.push_back(t);
    Some(id)
}




fn user_thread_main() {
    let (entry, rsp) = {
        let s = SCHED.lock();
        let t = s.current.as_ref().expect("scheduler: no current thread");
        (t.user_entry, t.user_rsp)
    };
    
    
    crate::syscall::enter_user_mode(entry, rsp)
}






pub fn spawn_user(
    name: &'static str,
    aspace: vmm::AddressSpace,
    entry: u64,
    user_rsp: u64,
) -> Option<u64> {
    let mut t = create_thread(name, user_thread_main, false)?;
    t.aspace = Some(aspace);
    t.user_entry = entry;
    t.user_rsp = user_rsp;
    let id = t.id;
    SCHED.lock().ready.push_back(t);
    Some(id)
}



pub fn kill_user(id: u64) -> bool {
    let saved = sync::irq_save();
    let killed = {
        let mut scheduler = SCHED.lock();
        let ready_index = scheduler
            .ready
            .iter()
            .position(|thread| thread.id == id && thread.aspace.is_some());
        if let Some(index) = ready_index {
            let thread = scheduler.ready.remove(index).unwrap();
            scheduler.zombies.push(thread);
            true
        } else {
            let sleeping_index = scheduler
                .sleeping
                .iter()
                .position(|thread| thread.id == id && thread.aspace.is_some());
            if let Some(index) = sleeping_index {
                let thread = scheduler.sleeping.swap_remove(index);
                scheduler.zombies.push(thread);
                true
            } else {
                false
            }
        }
    };
    sync::irq_restore(saved);
    killed
}


pub fn yield_now() {
    if READY.load(Ordering::SeqCst) {
        reschedule(Next::Yield);
    }
}


pub fn sleep_ms(ms: u64) {
    if !READY.load(Ordering::SeqCst) {
        return;
    }
    reschedule(Next::Sleep(pit::uptime_ms() + ms));
}


pub fn exit() -> ! {
    if READY.load(Ordering::SeqCst) {
        reschedule(Next::Exit);
    }
    
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}




pub fn tick() {
    if !READY.load(Ordering::Relaxed) {
        return;
    }
    let used = SLICE_TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    let slice_ticks = (pit::frequency_hz() * TIME_SLICE_MS / 1000).max(1);

    
    
    
    let Some(s) = SCHED.try_lock() else { return };
    let now = pit::uptime_ms();
    let sleeper_due = s.sleeping.iter().any(|t| t.wake_at <= now);
    let slice_over = used >= slice_ticks && !s.ready.is_empty();
    drop(s);

    if sleeper_due || slice_over {
        NEED_RESCHED.store(true, Ordering::Relaxed);
    }
}



pub fn preempt_if_needed() {
    if NEED_RESCHED.swap(false, Ordering::Relaxed) {
        PREEMPTIONS.fetch_add(1, Ordering::Relaxed);
        reschedule(Next::Yield);
    }
}

pub fn preemption_count() -> u64 {
    PREEMPTIONS.load(Ordering::Relaxed)
}

pub fn current_id() -> u64 {
    SCHED.lock().current.as_ref().map(|t| t.id).unwrap_or(0)
}

pub fn current_name() -> &'static str {
    SCHED.lock().current.as_ref().map(|t| t.name).unwrap_or("?")
}

pub fn try_crash_context() -> Option<CrashContext> {
    let scheduler = SCHED.try_lock()?;
    let thread = scheduler.current.as_ref()?;
    let (stack_low, stack_high) = thread.stack_slot.map_or((0, 0), |slot| {
        let base = slot_base(slot);
        (base + PAGE_SIZE, base + (1 + STACK_PAGES) * PAGE_SIZE)
    });
    Some(CrashContext {
        id: thread.id,
        thread_name: thread.name,
        user_process: thread.aspace.is_some(),
        stack_low,
        stack_high,
    })
}


pub fn stack_slots_in_use() -> usize {
    let s = SCHED.lock();
    s.next_slot - s.free_slots.len()
}


















pub struct WaitQueue {
    waiters: IrqMutex<VecDeque<Box<Thread>>>,
}

impl WaitQueue {
    pub const fn new() -> Self {
        WaitQueue {
            waiters: IrqMutex::new(VecDeque::new()),
        }
    }

    
    
    
    
    
    
    
    
    
    pub fn wait_until<F: FnMut() -> bool>(&self, mut cond: F) {
        loop {
            let saved = sync::irq_save();
            if cond() {
                sync::irq_restore(saved);
                return;
            }
            if !READY.load(Ordering::SeqCst) {
                
                sync::irq_restore(saved);
                return;
            }
            
            
            reschedule(Next::Block(&self.waiters));
            sync::irq_restore(saved);
        }
    }

    
    
    
    pub fn wait(&self) {
        if !READY.load(Ordering::SeqCst) {
            return;
        }
        reschedule(Next::Block(&self.waiters));
    }

    
    
    pub fn wake_one(&self) -> bool {
        let woken = self.waiters.lock().pop_front();
        match woken {
            Some(t) => {
                SCHED.lock().ready.push_back(t);
                true
            }
            None => false,
        }
    }

    
    pub fn wake_all(&self) {
        let all: Vec<Box<Thread>> = self.waiters.lock().drain(..).collect();
        if all.is_empty() {
            return;
        }
        let mut s = SCHED.lock();
        for t in all {
            s.ready.push_back(t);
        }
    }

    
    pub fn len(&self) -> usize {
        self.waiters.lock().len()
    }
}





static DONE: AtomicUsize = AtomicUsize::new(0);
static ORDER_LEN: AtomicUsize = AtomicUsize::new(0);
static ORDER: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
static SLEEP_ELAPSED: AtomicU64 = AtomicU64::new(0);

static STOP: AtomicBool = AtomicBool::new(false);
static SPIN_A: AtomicU64 = AtomicU64::new(0);
static SPIN_B: AtomicU64 = AtomicU64::new(0);
static SPIN_DONE: AtomicUsize = AtomicUsize::new(0);

const WORKERS: usize = 3;
const ROUNDS: usize = 4;

fn worker() {
    let id = current_id();
    for _ in 0..ROUNDS {
        let i = ORDER_LEN.fetch_add(1, Ordering::SeqCst);
        if i < ORDER.len() {
            ORDER[i].store(id, Ordering::SeqCst);
        }
        yield_now();
    }
    DONE.fetch_add(1, Ordering::SeqCst);
}

fn sleeper() {
    let start = pit::uptime_ms();
    sleep_ms(50);
    SLEEP_ELAPSED.store(pit::uptime_ms() - start, Ordering::SeqCst);
    DONE.fetch_add(1, Ordering::SeqCst);
}


fn spinner_a() {
    while !STOP.load(Ordering::Relaxed) {
        SPIN_A.fetch_add(1, Ordering::Relaxed);
        core::hint::spin_loop();
    }
    SPIN_DONE.fetch_add(1, Ordering::SeqCst);
}

fn spinner_b() {
    while !STOP.load(Ordering::Relaxed) {
        SPIN_B.fetch_add(1, Ordering::Relaxed);
        core::hint::spin_loop();
    }
    SPIN_DONE.fetch_add(1, Ordering::SeqCst);
}


fn wait_for(counter: &AtomicUsize, target: usize) -> bool {
    let deadline = pit::uptime_ms() + 2000;
    while counter.load(Ordering::SeqCst) < target {
        if pit::uptime_ms() > deadline {
            return false;
        }
        yield_now();
    }
    true
}

pub fn self_test() {
    if !READY.load(Ordering::SeqCst) {
        log_fail!("Scheduler", "SelfTest", "Scheduler not initialized");
        return;
    }

    DONE.store(0, Ordering::SeqCst);
    ORDER_LEN.store(0, Ordering::SeqCst);
    let slots_before = stack_slots_in_use();

    
    for _ in 0..WORKERS {
        if spawn("worker", worker).is_none() {
            log_fail!("Scheduler", "SelfTest", "Could not spawn a worker thread");
            return;
        }
    }
    if spawn("sleeper", sleeper).is_none() {
        log_fail!(
            "Scheduler",
            "SelfTest",
            "Could not spawn the sleeper thread"
        );
        return;
    }

    if !wait_for(&DONE, WORKERS + 1) {
        log_fail!(
            "Scheduler",
            "SelfTest",
            "Timed out: only {} of {} threads finished",
            DONE.load(Ordering::SeqCst),
            WORKERS + 1
        );
        return;
    }

    
    
    sleep_ms(20);

    let total = ORDER_LEN.load(Ordering::SeqCst);
    if total != WORKERS * ROUNDS {
        log_fail!(
            "Scheduler",
            "SelfTest",
            "Workers made {} steps, expected {}",
            total,
            WORKERS * ROUNDS
        );
        return;
    }

    
    
    
    
    let order: [u64; 16] = core::array::from_fn(|i| ORDER[i].load(Ordering::SeqCst));
    for i in 0..total {
        let n = order[..total].iter().filter(|&&x| x == order[i]).count();
        if n != ROUNDS {
            log_fail!(
                "Scheduler",
                "SelfTest",
                "Thread {} took {} steps, expected {}",
                order[i],
                n,
                ROUNDS
            );
            return;
        }
    }
    if order[0] == order[1] && order[1] == order[2] {
        log_fail!("Scheduler", "SelfTest", "Threads did not interleave");
        return;
    }

    let slept = SLEEP_ELAPSED.load(Ordering::SeqCst);
    if slept < 50 || slept > 500 {
        log_fail!("Scheduler", "SelfTest", "sleep_ms(50) took {} ms", slept);
        return;
    }

    
    
    
    STOP.store(false, Ordering::SeqCst);
    SPIN_A.store(0, Ordering::SeqCst);
    SPIN_B.store(0, Ordering::SeqCst);
    SPIN_DONE.store(0, Ordering::SeqCst);
    let preempt_before = preemption_count();

    if spawn("spin-a", spinner_a).is_none() || spawn("spin-b", spinner_b).is_none() {
        log_fail!("Scheduler", "SelfTest", "Could not spawn the busy threads");
        return;
    }

    let start = pit::uptime_ms();
    sleep_ms(100);
    let woke_after = pit::uptime_ms() - start;

    let a = SPIN_A.load(Ordering::Relaxed);
    let b = SPIN_B.load(Ordering::Relaxed);
    STOP.store(true, Ordering::SeqCst);

    if !wait_for(&SPIN_DONE, 2) {
        log_fail!("Scheduler", "SelfTest", "Busy threads did not stop");
        return;
    }
    sleep_ms(20); 

    let preemptions = preemption_count() - preempt_before;
    if a == 0 || b == 0 {
        log_fail!(
            "Scheduler",
            "SelfTest",
            "Busy threads did not share the CPU (a={}, b={})",
            a,
            b
        );
        return;
    }
    if preemptions < 2 {
        log_fail!(
            "Scheduler",
            "SelfTest",
            "Only {} preemptions during a 100 ms busy period",
            preemptions
        );
        return;
    }
    if woke_after > 100 + 100 {
        log_fail!(
            "Scheduler",
            "SelfTest",
            "Sleeping thread woke {} ms late behind busy threads",
            woke_after - 100
        );
        return;
    }

    let slots_after = stack_slots_in_use();
    if slots_after != slots_before {
        log_fail!(
            "Scheduler",
            "SelfTest",
            "Stack slots leaked: {} in use before, {} after",
            slots_before,
            slots_after
        );
        return;
    }

    log_ok!(
        "Scheduler",
        "SelfTest",
        "cooperative + preemptive verified: sleep_ms(50) took {} ms, 2 busy threads shared the CPU ({} preemptions in 100 ms, boot woke after {} ms), stacks reclaimed",
        slept,
        preemptions,
        woke_after
    );
}

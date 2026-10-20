//! `SYSCALL`/`SYSRET`: the fast path a user-mode thread uses to trap
//! into the kernel and back, plus `enter_user_mode`, the one-way door
//! from ring 0 into ring 3 that something has to walk through first.
//!
//! `SYSCALL` is deliberately minimal in hardware: it saves RIP in RCX
//! and RFLAGS in R11, loads CS/SS from `STAR` and RIP from `LSTAR`, and
//! masks RFLAGS with `SFMASK` -- and that's it. Unlike a hardware
//! interrupt/exception (which, on a ring 3 -> ring 0 transition, also
//! switches to the stack in `TSS.rsp0` automatically), `SYSCALL` does
//! *not* touch RSP at all: the entry stub is still running on whatever
//! stack the user thread had. The stub's first job is always to get off
//! that stack before doing anything else, using `gdt::TSS_RSP0` -- the
//! same "top of this thread's kernel stack" value the scheduler already
//! maintains for the hardware path -- to find a safe one.
//!
//! Self-test: the test maps a couple of user-accessible pages straight
//! into the kernel's own address space, drops a dedicated thread into
//! ring 3 to run a few hand-assembled instructions there, and checks
//! that the `syscall` those instructions issue makes it back into the
//! kernel with the right argument. (Real processes, with their own
//! address spaces, live in `process.rs`.)
//!
//! System calls follow the Linux x86-64 convention -- number in RAX,
//! arguments in RDI/RSI/RDX/R10/R8/R9, result (or a negative errno) in
//! RAX -- and reuse Linux's numbers for the calls that exist, so
//! ordinary tools and libcs have a chance of working later.

#![allow(dead_code)]

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::{gdt, pit, pmm, process, scheduler, sync, vmm};
use crate::{log_debug, log_fail, log_ok};

const MSR_EFER: u32 = 0xC000_0080;
const MSR_STAR: u32 = 0xC000_0081;
const MSR_LSTAR: u32 = 0xC000_0082;
const MSR_SFMASK: u32 = 0xC000_0084;
const EFER_SCE: u64 = 1 << 0;

unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr, out("eax") lo, out("edx") hi,
        options(nomem, nostack, preserves_flags),
    );
    ((hi as u64) << 32) | lo as u64
}

unsafe fn wrmsr(msr: u32, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr, in("eax") lo, in("edx") hi,
        options(nomem, nostack, preserves_flags),
    );
}

/// Register frame `syscall_entry` builds on the kernel stack before
/// calling `syscall_dispatch`, lowest address first (i.e. in the order
/// the entry stub pushes them, read back to front).
#[repr(C)]
pub struct SyscallFrame {
    /// Syscall number in, return value out.
    pub rax: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    /// 4th argument. Not RCX -- `syscall` clobbers RCX with the return
    /// RIP, so the calling convention moves that argument to R10.
    pub r10: u64,
    pub r8: u64,
    pub r9: u64,
    /// Saved by the CPU into RCX; restored from here before `sysretq`.
    pub rip: u64,
    /// Saved by the CPU into R11; restored from here before `sysretq`.
    pub rflags: u64,
    pub user_rsp: u64,
}

extern "C" {
    fn syscall_entry();
    fn enter_user_mode_asm(entry: u64, user_rsp: u64, user_cs: u64, user_ds: u64) -> !;
}

core::arch::global_asm!(
    r#"
.section .bss
.balign 8
syscall_scratch_rsp:
    .skip 8

.section .text

.global syscall_entry
syscall_entry:
    # On entry: RIP is in RCX, RFLAGS is in R11, CS/SS are already the
    # kernel selectors (from STAR) -- but RSP is still the user thread's
    # own stack, exactly as `syscall` left it. Get off it before doing
    # anything else.
    mov [rip + syscall_scratch_rsp], rsp
    mov rsp, [rip + TSS_RSP0]
    push qword ptr [rip + syscall_scratch_rsp] # user_rsp
    push r11                               # rflags
    push rcx                               # rip
    push r9
    push r8
    push r10
    push rdx
    push rsi
    push rdi
    push rax
    mov rdi, rsp
    cld
    call syscall_dispatch
    # The dispatcher runs with interrupts enabled. Turn them off again
    # before RSP is pointed back at the *user* stack below: an interrupt
    # taken in ring 0 on a user-controlled stack would be a disaster.
    # SYSRET restores IF from the saved RFLAGS (R11).
    cli
    pop rax
    pop rdi
    pop rsi
    pop rdx
    pop r10
    pop r8
    pop r9
    pop rcx
    pop r11
    pop rsp
    sysretq

.global enter_user_mode_asm
enter_user_mode_asm:
    # rdi = entry, rsi = user_rsp, rdx = user_cs (RPL 3), rcx = user_ds (RPL 3)
    mov ax, cx
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    push rcx        # SS
    push rsi        # RSP
    pushfq
    pop rax
    or rax, 0x200   # force IF, so the new thread is interruptible
    push rax        # RFLAGS
    push rdx        # CS
    push rdi        # RIP
    # Don't leak kernel register contents into the new program.
    xor eax, eax
    xor ebx, ebx
    xor ecx, ecx
    xor edx, edx
    xor esi, esi
    xor edi, edi
    xor ebp, ebp
    xor r8d, r8d
    xor r9d, r9d
    xor r10d, r10d
    xor r11d, r11d
    xor r12d, r12d
    xor r13d, r13d
    xor r14d, r14d
    xor r15d, r15d
    iretq
"#
);

/// Wire up `SYSCALL`/`SYSRET`. Call once, after `gdt::init` (needs
/// `USER_CS`/`USER_DS` to already be in the GDT) and before any thread
/// might execute a `syscall`.
pub fn init() {
    unsafe {
        let efer = rdmsr(MSR_EFER);
        wrmsr(MSR_EFER, efer | EFER_SCE);

        // STAR[47:32]: SYSCALL loads CS from here (RPL forced 0) and SS
        // from here+8 -- must be the kernel code selector, immediately
        // followed by the kernel data selector.
        //
        // STAR[63:48]: SYSRETQ loads SS from here+8 and CS from here+16
        // (RPL forced 3) -- so this must be 8 less than USER_DS, which
        // is exactly why gdt.rs lays USER_DS/USER_CS out back-to-back.
        let syscall_base = gdt::KERNEL_CS as u64;
        let sysret_base = gdt::USER_DS as u64 - 8;
        wrmsr(MSR_STAR, (sysret_base << 48) | (syscall_base << 32));

        wrmsr(MSR_LSTAR, syscall_entry as *const () as usize as u64);

        // Cleared from RFLAGS on entry, before the stub has a valid
        // stack: IF (so the stub can't be preempted mid-switch), TF and
        // DF (so a stray debug/string-op assumption from user mode
        // can't follow it in).
        wrmsr(MSR_SFMASK, (1 << 8) | (1 << 9) | (1 << 10));
    }

    log_ok!(
        "Syscall",
        "Init",
        "SYSCALL/SYSRET enabled (entry={:#018x}, user_cs={:#04x}, user_ds={:#04x})",
        syscall_entry as *const () as usize,
        gdt::USER_CS | 3,
        gdt::USER_DS | 3
    );
}

// System call numbers (Linux x86-64 numbering for the ones Linux has).
pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_SCHED_YIELD: u64 = 24;
pub const SYS_GETPID: u64 = 39;
pub const SYS_EXIT: u64 = 60;
pub const SYS_WAIT4: u64 = 61;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_KILL: u64 = 62;

// EonOS-specific utilities used by the shell. File access itself uses
// open/read/write/close; list/read-by-path remain compatibility helpers.
pub const SYS_LIST_FILES: u64 = 9001;
pub const SYS_READ_FILE: u64 = 9002;
/// Milliseconds since boot, per `pit::uptime_ms()`. Backs the shell's
/// `uptime` command; Linux has no equivalent single-call primitive
/// (it's normally read out of /proc), so this gets an EonOS-specific
/// number like the two above rather than trying to match a real one.
pub const SYS_UPTIME_MS: u64 = 9003;
/// Seconds since 1970-01-01 UTC, or zero when the RTC is unavailable.
pub const SYS_WALL_TIME: u64 = 9045;
pub const SYS_SLEEP_MS: u64 = 9004;
pub const SYS_EXEC: u64 = 9005;
pub const SYS_LIST_PROCESSES: u64 = 9006;
pub const SYS_MKDIR: u64 = 9007;
pub const SYS_REMOVE: u64 = 9008;
pub const SYS_SYNC: u64 = 9009;
pub const SYS_LIST_BLOCK_DEVICES: u64 = 9010;
/// Intentionally enter the kernel panic handler (exposed by the shell's
/// `panic` command for testing the panic screen and logging path).
pub const SYS_PANIC: u64 = 9011;
/// Mount a FAT32 block device at a VFS path; arguments are NUL-terminated strings.
pub const SYS_MOUNT: u64 = 9012;
/// Allocate and free process-owned writable pages for the userspace heap.
pub const SYS_ALLOC_PAGES: u64 = 9013;
pub const SYS_FREE_PAGES: u64 = 9014;
pub const SYS_CHDIR: u64 = 9015;
pub const SYS_GETCWD: u64 = 9016;
pub const SYS_LIST_DIRECTORY: u64 = 9017;
pub const SYS_EXEC_IO: u64 = 9018;
pub const SYS_PIPE: u64 = 9019;
pub const SYS_MOUSE_STATE: u64 = 9020;
/// Era's bounded user-mode drawing API.
pub const SYS_ERA_GRAPHICS: u64 = 9021;
/// Copy physical-frame and kernel-heap statistics into a userspace buffer.
pub const SYS_MEMORY_INFO: u64 = 9022;
/// Flush storage and request a system-wide restart or ACPI soft-off.
pub const SYS_POWER_CONTROL: u64 = 9023;
pub const SYS_IS_MOUNTPOINT: u64 = 9024;
pub const SYS_NET_INFO: u64 = 9025;
pub const SYS_NET_SEND: u64 = 9026;
pub const SYS_NET_RECEIVE: u64 = 9027;
/// Copy the active VFS mount table into a caller-provided buffer.
pub const SYS_LIST_MOUNTS: u64 = 9028;
pub const SYS_NET_DHCP: u64 = 9029;
pub const SYS_NET_CONFIG: u64 = 9030;
pub const SYS_UDP_SEND: u64 = 9031;
pub const SYS_UDP_RECEIVE: u64 = 9032;
pub const SYS_NET_PING: u64 = 9033;
pub const SYS_DNS_LOOKUP: u64 = 9034;
pub const SYS_TCP_CONNECT: u64 = 9041;
pub const SYS_TCP_SEND: u64 = 9042;
pub const SYS_TCP_RECEIVE: u64 = 9043;
pub const SYS_TCP_CLOSE: u64 = 9044;
/// Read one focused keyboard edge event into a user buffer (0 if none).
pub const SYS_KEYBOARD_EVENT: u64 = 9035;
/// Claim or release keyboard-event focus (operation: 1 claim, 0 release).
pub const SYS_KEYBOARD_FOCUS: u64 = 9036;
/// Seek an open file descriptor (fd, signed offset, SEEK_SET/CUR/END).
pub const SYS_SEEK: u64 = 9037;
/// Read currently buffered pipe bytes without waiting for a writer.
pub const SYS_PIPE_READ_AVAILABLE: u64 = 9038;
pub const SYS_PIPE_WRITE_AVAILABLE: u64 = 9039;
pub const SYS_EXEC_APP_IO: u64 = 9040;

const ERA_GRAPHICS_ENTER: u64 = 0;
const ERA_GRAPHICS_FILL_RECT: u64 = 1;
const ERA_GRAPHICS_TEXT: u64 = 2;
const ERA_GRAPHICS_PRESENT: u64 = 3;
const ERA_GRAPHICS_EXIT: u64 = 4;
const ERA_GRAPHICS_IMAGE: u64 = 5;
const ERA_GRAPHICS_BLIT_SCALED: u64 = 6;
const ERA_MAX_TEXT_BYTES: usize = 512;
const ERA_MAX_IMAGE_PIXELS: usize = 262_144;

const USER_HEAP_BASE: u64 = 0x0000_7000_0000_0000;
const USER_HEAP_END: u64 = 0x0000_7000_1000_0000;
const MAX_USER_HEAP_PAGES: usize = 64 * 1024;
const MAX_USER_ALLOCATION_PAGES: usize = 16 * 1024;
const MAX_SANDBOX_HEAP_PAGES: usize = 16 * 1024;

struct UserPageAllocation {
    pid: u64,
    base: u64,
    pages: usize,
}

static USER_PAGE_ALLOCATIONS: sync::IrqMutex<alloc::vec::Vec<UserPageAllocation>> =
    sync::IrqMutex::new(alloc::vec::Vec::new());

#[derive(Clone, Copy, PartialEq, Eq)]
enum FdKind {
    Stdin,
    Stdout,
    Stderr,
    Console,
    Null,
    Zero,
    Urandom,
    File,
    PipeRead(u64),
    PipeWrite(u64),
}

struct OpenFile {
    pid: u64,
    fd: u64,
    kind: FdKind,
    path: alloc::string::String,
    offset: usize,
    readable: bool,
    writable: bool,
    append: bool,
}
static OPEN_FILES: sync::IrqMutex<alloc::vec::Vec<OpenFile>> =
    sync::IrqMutex::new(alloc::vec::Vec::new());
struct PipeState {
    id: u64,
    bytes: alloc::collections::VecDeque<u8>,
    readers: usize,
    writers: usize,
}
static PIPES: sync::IrqMutex<alloc::vec::Vec<PipeState>> =
    sync::IrqMutex::new(alloc::vec::Vec::new());
static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(1);
const PIPE_CAPACITY: usize = 16 * 1024;

const EBADF: i64 = 9;
const EFAULT: i64 = 14;
const ENOENT: i64 = 2;
const ENOSYS: i64 = 38;
const ENAMETOOLONG: i64 = 36;
const ERANGE: i64 = 34;
const ESRCH: i64 = 3;
const EROFS: i64 = 30;
const ENOSPC: i64 = 28;
const EINVAL: i64 = 22;
const EIO: i64 = 5;
const EPERM: i64 = 1;
const ENOMEM: i64 = 12;
const EEXIST: i64 = 17;
const ENOTDIR: i64 = 20;
const EISDIR: i64 = 21;
const ENOTEMPTY: i64 = 39;
const ECHILD: i64 = 10;
const E2BIG: i64 = 7;
const ENOEXEC: i64 = 8;
const EAGAIN: i64 = 11;
const ENODEV: i64 = 19;
const EADDRNOTAVAIL: i64 = 99;
const ENETDOWN: i64 = 100;
const EHOSTUNREACH: i64 = 113;
const ENOBUFS: i64 = 105;
const ETIMEDOUT: i64 = 110;
const ECONNRESET: i64 = 104;
const ENOTCONN: i64 = 107;
const ENOTSUP: i64 = 95;
const EPIPE: i64 = 32;
const ESPIPE: i64 = 29;

const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0x40;
const O_TRUNC: u64 = 0x200;
const O_APPEND: u64 = 0x400;

/// Install a fresh process's standard streams. Processes currently start
/// independently, so there is no parent descriptor inheritance yet.
pub fn init_process_files(pid: u64, parent: u64, stdin_fd: Option<u64>, stdout_fd: Option<u64>) {
    let mut files = OPEN_FILES.lock();
    files.retain(|file| file.pid != pid);
    let input = stdin_fd.and_then(|fd| {
        files
            .iter()
            .find(|file| file.pid == parent && file.fd == fd && file.readable)
    });
    let output = stdout_fd.and_then(|fd| {
        files
            .iter()
            .find(|file| file.pid == parent && file.fd == fd && file.writable)
    });
    let input = input.map(|file| {
        (
            file.kind,
            file.path.clone(),
            file.offset,
            file.readable,
            file.writable,
            file.append,
        )
    });
    let output = output.map(|file| {
        (
            file.kind,
            file.path.clone(),
            file.offset,
            file.readable,
            file.writable,
            file.append,
        )
    });
    for (fd, source, kind, readable, writable) in [
        (0, input, FdKind::Stdin, true, false),
        (1, output, FdKind::Stdout, false, true),
        (2, None, FdKind::Stderr, false, true),
    ] {
        let (kind, path, offset, readable, writable, append) = source
            .map(|(kind, path, offset, readable, writable, append)| {
                (kind, path, offset, readable, writable, append)
            })
            .unwrap_or((
                kind,
                alloc::string::String::new(),
                0,
                readable,
                writable,
                false,
            ));
        if matches!(kind, FdKind::PipeRead(_) | FdKind::PipeWrite(_)) {
            add_pipe_ref(kind);
        }
        files.push(OpenFile {
            pid,
            fd,
            kind,
            path,
            offset,
            readable,
            writable,
            append,
        });
    }
}

/// Encode a positive errno as the negative value a syscall returns.
const fn errno(e: i64) -> u64 {
    (-e) as u64
}

/// Longest run of bytes a single `write` will take; longer requests are
/// short-written (callers loop, exactly as with Linux).
const MAX_WRITE: usize = 4096;

/// Put raw process output on the serial port and the screen, without
/// the log decorations `klog!` adds.
fn console_write(bytes: &[u8]) {
    let text = alloc::string::String::from_utf8_lossy(bytes);
    {
        let mut serial = crate::serial::SERIAL1.lock();
        let _ = serial.write_str(&text);
    }
    if let Some(console) = crate::console::CONSOLE.lock().as_mut() {
        let _ = console.write_str(&text);
    }
}

/// How many previous lines `read_line` remembers for up/down-arrow
/// recall. Bounded so a long session doesn't grow this forever; old
/// entries are dropped, same trade-off as the PS/2 driver's own ring
/// buffer.
const HISTORY_CAP: usize = 32;

static HISTORY: sync::IrqMutex<alloc::collections::VecDeque<alloc::vec::Vec<u8>>> =
    sync::IrqMutex::new(alloc::collections::VecDeque::new());
static DEFERRED_INPUT: sync::IrqMutex<alloc::collections::VecDeque<u8>> =
    sync::IrqMutex::new(alloc::collections::VecDeque::new());
static FOREGROUND_CHILD: AtomicU64 = AtomicU64::new(0);
static CTRL_C_PENDING: AtomicBool = AtomicBool::new(false);
static TERMINAL_READER: AtomicBool = AtomicBool::new(false);
static URANDOM_STATE: AtomicU64 = AtomicU64::new(0);

/// Get one pseudorandom word, preferring RDRAND when the CPU advertises it.
/// The timing-seeded fallback is useful for non-cryptographic random data,
/// but is not a cryptographic random number generator.
fn urandom_word() -> u64 {
    if core::arch::x86_64::__cpuid(1).ecx & (1 << 30) != 0 {
        for _ in 0..10 {
            let value: u64;
            let ready: u8;
            unsafe {
                core::arch::asm!("rdrand {value}", "setc {ready}",
                    value = lateout(reg) value, ready = lateout(reg_byte) ready,
                    options(nomem, nostack));
            }
            if ready != 0 {
                return value;
            }
        }
    }
    let mut old = URANDOM_STATE.load(Ordering::Relaxed);
    loop {
        let mut value = if old == 0 {
            0x9E37_79B9_7F4A_7C15 ^ pit::uptime_ms().rotate_left(19) ^ scheduler::current_id()
        } else {
            old
        };
        let tsc_low: u32;
        let tsc_high: u32;
        unsafe {
            core::arch::asm!("rdtsc", out("eax") tsc_low, out("edx") tsc_high, options(nomem, nostack));
        }
        value ^= ((tsc_high as u64) << 32) | tsc_low as u64;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        if value == 0 {
            value = 0xA076_1D64_78BD_642F;
        }
        match URANDOM_STATE.compare_exchange_weak(old, value, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return value.wrapping_mul(0x2545_F491_4F6C_DD1D),
            Err(actual) => old = actual,
        }
    }
}

fn fill_urandom(out: &mut [u8]) {
    for chunk in out.chunks_mut(8) {
        let bytes = urandom_word().to_ne_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

/// Next raw input byte from whichever source has one, blocking
/// (via yield, not a real wait queue) until one shows up. PS/2 and
/// serial both just fill a byte queue from their own IRQ handlers, so
/// polling either in turn is enough -- no reason to prefer one over
/// the other beyond "whichever has something waiting first".
fn next_byte() -> u8 {
    loop {
        if let Some(byte) = DEFERRED_INPUT.lock().pop_front() {
            return byte;
        }
        crate::drivers::usb::xhci::poll();
        if let Some(b) = crate::drivers::ps2::try_read_byte() {
            return b;
        }
        if let Some(b) = crate::drivers::usb::xhci::try_read_byte() {
            return b;
        }
        if let Some(b) = crate::serial::SERIAL1.lock().try_read_byte() {
            return b;
        }
        scheduler::yield_now();
    }
}

/// Decode a `\x1b[...` escape sequence (as PS/2 arrow/nav keys are
/// encoded into by `drivers::ps2::push_escape`, and as a real terminal
/// emulator would send them over serial too) into one of a small fixed
/// set of edit actions. Called right after the ESC byte itself is
/// consumed. Reads (and blocks on) further bytes as needed -- safe
/// because a `\x1b` this driver produces is always immediately followed
/// by the rest of its sequence; a lone stray ESC from a real keyboard's
/// Esc key will just stall until another byte arrives, same as it would
/// on a real tty waiting to see if it's the start of a sequence.
enum EditAction {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Delete,
    Unknown,
}

fn decode_escape() -> EditAction {
    if next_byte() != b'[' {
        return EditAction::Unknown;
    }
    match next_byte() {
        b'A' => EditAction::Up,
        b'B' => EditAction::Down,
        b'C' => EditAction::Right,
        b'D' => EditAction::Left,
        b'H' => EditAction::Home,
        b'F' => EditAction::End,
        b'3' => {
            // Delete is "\x1b[3~" -- a three-byte final sequence rather
            // than one letter, so it needs an extra byte consumed.
            let _ = next_byte(); // expected '~'
            EditAction::Delete
        }
        _ => EditAction::Unknown,
    }
}

/// Erase the currently-displayed line on screen and replace both the
/// buffer and the terminal display with `new_line`, cursor at the end.
/// Used for history recall, where the whole line changes at once
/// rather than one character at a time.
fn replace_line(line: &mut alloc::vec::Vec<u8>, cursor: &mut usize, new_line: alloc::vec::Vec<u8>) {
    cursor_left(*cursor);
    for _ in 0..line.len() {
        console_write(b" ");
    }
    cursor_left(line.len());
    console_write(&new_line);
    *line = new_line;
    *cursor = line.len();
}

/// Move the terminal cursor left `n` cells without touching what's
/// drawn there, via the standard `CSI n D` sequence (`console.rs`
/// handles it; a real serial terminal understands it natively too).
/// Distinct from sending literal `0x08` bytes, which both this
/// console and real terminals treat as destructive backspace -- correct
/// for deleting a character, wrong for merely repositioning over
/// content that must stay on screen (redraws, arrow-key movement).
fn cursor_left(n: usize) {
    if n == 0 {
        return;
    }
    let mut seq = alloc::vec::Vec::new();
    seq.extend_from_slice(b"\x1b[");
    seq.extend_from_slice(itoa(n).as_bytes());
    seq.push(b'D');
    console_write(&seq);
}

/// Minimal integer-to-decimal-ASCII, since `core::fmt` formatting isn't
/// worth pulling into this hot little path. `n` is always small here
/// (line lengths), so no need for anything fancier.
fn itoa(mut n: usize) -> alloc::string::String {
    if n == 0 {
        return alloc::string::String::from("0");
    }
    let mut digits = alloc::vec::Vec::new();
    while n > 0 {
        digits.push(b'0' + (n % 10) as u8);
        n /= 10;
    }
    digits.reverse();
    alloc::string::String::from_utf8(digits).unwrap()
}

/// Redraw the visible line from `from` (a byte index into `line`) to
/// its end, then park the cursor back at `cursor`. Used whenever an
/// edit touches anything before the end of the line -- a plain append
/// or trailing backspace doesn't need this and stays on the cheaper
/// path in `read_line` itself.
fn redraw_tail(line: &[u8], from: usize, cursor: usize) {
    console_write(&line[from..]);
    console_write(b" "); // erase whatever character used to trail here
    cursor_left(line.len() - cursor + 1);
}

fn complete_line(line: &mut alloc::vec::Vec<u8>, cursor: &mut usize, max: usize) {
    let start = line[..*cursor]
        .iter()
        .rposition(|byte| byte.is_ascii_whitespace())
        .map_or(0, |index| index + 1);
    let Ok(prefix) = core::str::from_utf8(&line[start..*cursor]) else {
        console_write(b"\x07");
        return;
    };
    let mut candidates = alloc::vec::Vec::<alloc::string::String>::new();
    if start == 0 {
        for command in [
            "cat", "cd", "clear", "cp", "echo", "edit", "exec", "exit", "help", "jobs", "kill",
            "ls", "lsblk", "mkdir", "mount", "mouse", "pid", "ps", "pwd", "rm", "sleep", "sync",
            "touch", "uptime", "wait", "write", "era",
        ] {
            if command.starts_with(prefix) {
                candidates.push(alloc::string::String::from(command));
            }
        }
        if let Ok(entries) = crate::vfs::list("/bin") {
            for entry in entries.into_iter().filter(|entry| !entry.is_dir) {
                if entry.name.starts_with(prefix)
                    && !candidates.iter().any(|candidate| candidate == &entry.name)
                {
                    candidates.push(entry.name);
                }
            }
        }
    } else {
        fn walk(
            path: &str,
            prefix: &str,
            out: &mut alloc::vec::Vec<alloc::string::String>,
            depth: usize,
        ) {
            if depth >= 32 || out.len() >= 1024 {
                return;
            }
            let Ok(entries) = crate::vfs::list(path) else {
                return;
            };
            for entry in entries {
                let child = if path == "/" {
                    alloc::format!("/{}", entry.name)
                } else {
                    alloc::format!("{}/{}", path.trim_end_matches('/'), entry.name)
                };
                let candidate = if entry.is_dir {
                    alloc::format!("{}/", child.trim_end_matches('/'))
                } else {
                    child.clone()
                };
                if candidate.starts_with(prefix) {
                    out.push(candidate);
                }
                if entry.is_dir {
                    walk(&child, prefix, out, depth + 1);
                }
            }
        }
        walk("/", prefix, &mut candidates, 0);
    }
    candidates.sort();
    candidates.dedup();
    if candidates.is_empty() {
        console_write(b"\x07");
        return;
    }

    let replacement = if candidates.len() == 1 {
        candidates[0].clone()
    } else {
        let mut common = candidates[0].as_bytes().to_vec();
        for candidate in candidates.iter().skip(1) {
            let shared = common
                .iter()
                .zip(candidate.as_bytes())
                .take_while(|(left, right)| left == right)
                .count();
            common.truncate(shared);
        }
        alloc::string::String::from_utf8(common).unwrap_or_default()
    };
    let suffix_len = line.len().saturating_sub(*cursor);
    let capacity = max.saturating_sub(1 + suffix_len + start);
    let mut bytes = replacement.as_bytes()[..replacement.len().min(capacity)].to_vec();
    if candidates.len() == 1
        && suffix_len == 0
        && !replacement.ends_with('/')
        && start + bytes.len() < max - 1
    {
        bytes.push(b' ');
    }
    if bytes.len() > prefix.len() {
        line.splice(start..*cursor, bytes.iter().copied());
        cursor_left(*cursor - start);
        *cursor = start + bytes.len();
        redraw_tail(line, start, *cursor);
        return;
    }

    if candidates.len() > 1 {
        console_write(b"\n");
        for candidate in candidates.iter().take(16) {
            console_write(candidate.as_bytes());
            console_write(b"  ");
        }
        if candidates.len() > 16 {
            console_write(b"...");
        }
        console_write(b"\n");
        console_write(line);
        cursor_left(line.len().saturating_sub(*cursor));
    } else {
        console_write(b"\x07");
    }
}

/// Read one line of input for `sys_read(0, ...)`.
///
/// Input comes from the PS/2 keyboard (`drivers::ps2`), a USB HID boot
/// keyboard (`drivers::usb::xhci`), or the
/// serial port (COM1 -- in QEMU, typically `-serial stdio`, i.e. the
/// host terminal); both are polled so either can drive the shell.
/// This does canonical-mode editing in the kernel -- echoing bytes
/// back, backspace/delete, left/right cursor movement, and up/down
/// history recall via ANSI escape sequences -- the same job a real
/// tty line discipline does, just smaller.
///
/// Polls rather than waiting on an IRQ: both input sources just fill a
/// buffer in their own interrupt handlers, so a bare read-and-retry
/// loop (yielding between attempts so it doesn't starve other threads)
/// is the simplest way to consume either. Returns at most `max` bytes,
/// including the trailing `\n`.
fn read_line(max: usize) -> alloc::vec::Vec<u8> {
    let mut line: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    if max == 0 {
        return line;
    }
    let mut cursor = 0usize; // byte index into `line`; can be < line.len()
    let mut history_pos: Option<usize> = None; // index into HISTORY while recalling

    loop {
        let byte = next_byte();

        match byte {
            0x03 => {
                if FOREGROUND_CHILD.load(Ordering::Acquire) == scheduler::current_id() {
                    CTRL_C_PENDING.store(true, Ordering::Release);
                    TERMINAL_READER.store(false, Ordering::Release);
                    loop {
                        scheduler::yield_now();
                    }
                }
                console_write(b"^C\n");
                line.clear();
                line.push(b'\n');
                break;
            }
            b'\r' | b'\n' => {
                console_write(b"\n");
                if line.len() < max {
                    line.push(b'\n');
                }
                break;
            }
            0x1B => {
                match decode_escape() {
                    EditAction::Left => {
                        if cursor > 0 {
                            cursor -= 1;
                            cursor_left(1);
                        }
                    }
                    EditAction::Right => {
                        if cursor < line.len() {
                            console_write(&line[cursor..cursor + 1]);
                            cursor += 1;
                        }
                    }
                    EditAction::Home => {
                        cursor_left(cursor);
                        cursor = 0;
                    }
                    EditAction::End => {
                        console_write(&line[cursor..]);
                        cursor = line.len();
                    }
                    EditAction::Delete => {
                        if cursor < line.len() {
                            line.remove(cursor);
                            redraw_tail(&line, cursor, cursor);
                        }
                    }
                    EditAction::Up => {
                        let history = HISTORY.lock();
                        if history.is_empty() {
                            continue;
                        }
                        let next_pos = match history_pos {
                            None => history.len() - 1,
                            Some(0) => 0,
                            Some(p) => p - 1,
                        };
                        history_pos = Some(next_pos);
                        let entry = history[next_pos].clone();
                        drop(history);
                        replace_line(&mut line, &mut cursor, entry);
                    }
                    EditAction::Down => {
                        let history = HISTORY.lock();
                        match history_pos {
                            None => {} // already at the blank line, nothing newer
                            Some(p) if p + 1 < history.len() => {
                                let next_pos = p + 1;
                                history_pos = Some(next_pos);
                                let entry = history[next_pos].clone();
                                drop(history);
                                replace_line(&mut line, &mut cursor, entry);
                            }
                            Some(_) => {
                                history_pos = None;
                                drop(history);
                                replace_line(&mut line, &mut cursor, alloc::vec::Vec::new());
                            }
                        }
                    }
                    EditAction::Unknown => {}
                }
            }
            b'\t' => complete_line(&mut line, &mut cursor, max),
            0x08 | 0x7F => {
                // Backspace/DEL: remove the character before the
                // cursor, if any. A trailing backspace (cursor at the
                // end) is the common case and stays cheap; editing
                // mid-line requires reflowing everything after it.
                if cursor > 0 {
                    cursor -= 1;
                    line.remove(cursor);
                    console_write(b"\x08");
                    redraw_tail(&line, cursor, cursor);
                }
            }
            b => {
                if line.len() + 1 < max {
                    if cursor == line.len() {
                        line.push(b);
                        console_write(&[b]);
                        cursor += 1;
                    } else {
                        line.insert(cursor, b);
                        cursor += 1;
                        // Redraw from the inserted character onward (not
                        // from `cursor` alone) so it's drawn exactly
                        // once, then walk the terminal cursor back to
                        // just after it.
                        console_write(&line[cursor - 1..]);
                        for _ in 0..(line.len() - cursor) {
                            console_write(b"\x08");
                        }
                    }
                }
            }
        }
    }

    // Record non-empty, non-history-recalled lines for future recall.
    // (The trailing '\n' is stripped for storage and reattached to
    // whatever's returned to the caller as usual.)
    if line.len() > 1 {
        let mut h = HISTORY.lock();
        let stored = line[..line.len() - 1].to_vec();
        if h.back() != Some(&stored) {
            if h.len() == HISTORY_CAP {
                h.pop_front();
            }
            h.push_back(stored);
        }
    }

    line
}

fn sys_read(fd: u64, buf: u64, len: u64) -> u64 {
    let n = (len as usize).min(MAX_WRITE);
    if n == 0 {
        return 0;
    }
    let pid = scheduler::current_id();
    let kind = {
        let files = OPEN_FILES.lock();
        let Some(file) = files.iter().find(|f| f.pid == pid && f.fd == fd) else {
            return errno(EBADF);
        };
        if !file.readable {
            return errno(EBADF);
        }
        file.kind
    };
    // Check the full destination before reading a file or consuming a
    // console line. copy_to_user checks it again when it performs the copy.
    if !vmm::validate_user_range(buf, n, true) {
        return errno(EFAULT);
    }
    if let FdKind::PipeRead(pipe_id) = kind {
        return read_pipe(pipe_id, buf, n);
    }
    if kind == FdKind::Null {
        return 0;
    }
    if kind == FdKind::Zero {
        let data = alloc::vec![0; n];
        if !vmm::copy_to_user(buf, &data) {
            return errno(EFAULT);
        }
        return n as u64;
    }
    if kind == FdKind::Urandom {
        let mut data = alloc::vec![0; n];
        fill_urandom(&mut data);
        if !vmm::copy_to_user(buf, &data) {
            return errno(EFAULT);
        }
        return n as u64;
    }
    if matches!(kind, FdKind::Stdin | FdKind::Console) {
        while TERMINAL_READER
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            scheduler::yield_now();
        }
        let line = read_line(n);
        TERMINAL_READER.store(false, Ordering::Release);
        if !vmm::copy_to_user(buf, &line) {
            return errno(EFAULT);
        }
        return line.len() as u64;
    }
    if kind != FdKind::File {
        return errno(EBADF);
    }
    let mut files = OPEN_FILES.lock();
    let Some(file) = files.iter_mut().find(|f| f.pid == pid && f.fd == fd) else {
        return errno(EBADF);
    };
    let mut data = alloc::vec![0; n];
    let read = match crate::vfs::read_at(&file.path, file.offset, &mut data) {
        Ok(n) => n,
        Err(error) => return fs_errno(error),
    };
    if !vmm::copy_to_user(buf, &data[..read]) {
        return errno(EFAULT);
    }
    file.offset += read;
    read as u64
}

fn sys_open(path_ptr: u64, path_len: u64, flags: u64) -> u64 {
    let sandboxed = process::is_sandboxed(scheduler::current_id());
    let access = flags & 3;
    let allowed = O_WRONLY | O_RDWR | O_CREAT | O_TRUNC | O_APPEND;
    if flags & !allowed != 0
        || access == (O_WRONLY | O_RDWR)
        || flags & (O_TRUNC | O_APPEND) != 0 && access == 0
    {
        return errno(EINVAL);
    }
    if sandboxed && (access != 0 || flags & (O_CREAT | O_TRUNC | O_APPEND) != 0) {
        return errno(EPERM);
    }
    let n = path_len as usize;
    if n > MAX_NAME {
        return errno(ENAMETOOLONG);
    }
    let mut path = alloc::vec![0; n];
    if !vmm::copy_from_user(&mut path, path_ptr) {
        return errno(EFAULT);
    }
    let Ok(path) = core::str::from_utf8(&path) else {
        return errno(ENOENT);
    };
    let path = match crate::vfs::resolve_path(path, &process::cwd(scheduler::current_id())) {
        Ok(path) => path,
        Err(error) => return fs_errno(error),
    };
    if sandboxed && path == "/device/console" {
        return errno(EPERM);
    }
    let device = match path.as_str() {
        "/device/console" => Some(FdKind::Console),
        "/device/null" => Some(FdKind::Null),
        "/device/zero" => Some(FdKind::Zero),
        "/device/urandom" => Some(FdKind::Urandom),
        _ => None,
    };
    if let Some(kind) = device {
        if flags & (O_CREAT | O_TRUNC | O_APPEND) != 0 {
            return errno(EINVAL);
        }
        let pid = scheduler::current_id();
        let mut files = OPEN_FILES.lock();
        let fd = (3..)
            .find(|fd| !files.iter().any(|f| f.pid == pid && f.fd == *fd))
            .unwrap_or(3);
        files.push(OpenFile {
            pid,
            fd,
            kind,
            path: alloc::string::String::new(),
            offset: 0,
            readable: access == 0 || access == O_RDWR,
            writable: access == O_WRONLY || access == O_RDWR,
            append: false,
        });
        return fd;
    }
    let size = match crate::vfs::file_size(&path) {
        Ok(size) => size,
        Err(_) if flags & O_CREAT != 0 => {
            if let Err(error) = crate::vfs::create_file(&path) {
                return fs_errno(error);
            }
            0
        }
        Err(_) => return errno(ENOENT),
    };
    if flags & O_TRUNC != 0 {
        if let Err(error) = crate::vfs::truncate(&path) {
            return fs_errno(error);
        }
    }
    let pid = scheduler::current_id();
    let mut files = OPEN_FILES.lock();
    let fd = (3..)
        .find(|fd| !files.iter().any(|f| f.pid == pid && f.fd == *fd))
        .unwrap_or(3);
    let offset = if flags & O_APPEND != 0 && flags & O_TRUNC == 0 {
        size
    } else {
        0
    };
    files.push(OpenFile {
        pid,
        fd,
        path,
        offset,
        kind: FdKind::File,
        readable: access == 0 || access == O_RDWR,
        writable: access == O_WRONLY || access == O_RDWR,
        append: flags & O_APPEND != 0,
    });
    fd
}

fn read_pipe(id: u64, user_buffer: u64, length: usize) -> u64 {
    loop {
        let (data, eof) = {
            let mut pipes = PIPES.lock();
            let Some(pipe) = pipes.iter_mut().find(|pipe| pipe.id == id) else {
                return errno(EBADF);
            };
            if !pipe.bytes.is_empty() {
                let count = length.min(pipe.bytes.len());
                let data = pipe.bytes.drain(..count).collect::<alloc::vec::Vec<_>>();
                (Some(data), false)
            } else {
                (None, pipe.writers == 0)
            }
        };
        if let Some(data) = data {
            let count = data.len();
            if !vmm::copy_to_user(user_buffer, &data) {
                return errno(EFAULT);
            }
            return count as u64;
        }
        if eof {
            return 0;
        }
        scheduler::yield_now();
    }
}

fn sys_pipe_read_available(fd: u64, user_buffer: u64, length: usize) -> u64 {
    if length == 0 { return 0; }
    if length > PIPE_CAPACITY || !vmm::validate_user_range(user_buffer, length, true) {
        return errno(EINVAL);
    }
    let pid = scheduler::current_id();
    let pipe_id = {
        let files = OPEN_FILES.lock();
        let Some(file) = files.iter().find(|file| file.pid == pid && file.fd == fd && file.readable) else {
            return errno(EBADF);
        };
        match file.kind {
            FdKind::PipeRead(id) => id,
            _ => return errno(EINVAL),
        }
    };
    let data = {
        let mut pipes = PIPES.lock();
        let Some(pipe) = pipes.iter_mut().find(|pipe| pipe.id == pipe_id) else {
            return errno(EBADF);
        };
        let count = length.min(pipe.bytes.len());
        pipe.bytes.drain(..count).collect::<alloc::vec::Vec<_>>()
    };
    if !vmm::copy_to_user(user_buffer, &data) { return errno(EFAULT); }
    data.len() as u64
}

fn sys_pipe_write_available(fd: u64, user_buffer: u64, length: usize) -> u64 {
    if length == 0 { return 0; }
    if length > PIPE_CAPACITY || !vmm::validate_user_range(user_buffer, length, false) {
        return errno(EINVAL);
    }
    let pid = scheduler::current_id();
    let pipe_id = {
        let files = OPEN_FILES.lock();
        let Some(file) = files.iter().find(|file| file.pid == pid && file.fd == fd && file.writable) else {
            return errno(EBADF);
        };
        match file.kind { FdKind::PipeWrite(id) => id, _ => return errno(EINVAL) }
    };
    let mut data = alloc::vec![0; length];
    if !vmm::copy_from_user(&mut data, user_buffer) { return errno(EFAULT); }
    let mut pipes = PIPES.lock();
    let Some(pipe) = pipes.iter_mut().find(|pipe| pipe.id == pipe_id) else { return errno(EBADF); };
    if pipe.readers == 0 { return errno(EPIPE); }
    let count = length.min(PIPE_CAPACITY.saturating_sub(pipe.bytes.len()));
    pipe.bytes.extend(data[..count].iter().copied());
    count as u64
}

fn write_pipe(id: u64, data: &[u8]) -> u64 {
    loop {
        let mut pipes = PIPES.lock();
        let Some(pipe) = pipes.iter_mut().find(|pipe| pipe.id == id) else {
            return errno(EPIPE);
        };
        if pipe.readers == 0 {
            return errno(EPIPE);
        }
        let available = PIPE_CAPACITY.saturating_sub(pipe.bytes.len());
        if available > 0 {
            let count = available.min(data.len());
            pipe.bytes.extend(data[..count].iter().copied());
            return count as u64;
        }
        drop(pipes);
        scheduler::yield_now();
    }
}

fn sys_close(fd: u64) -> u64 {
    let pid = scheduler::current_id();
    let kind = {
        let mut files = OPEN_FILES.lock();
        let Some(i) = files.iter().position(|f| f.pid == pid && f.fd == fd) else {
            return errno(EBADF);
        };
        files.swap_remove(i).kind
    };
    drop_pipe_ref(kind);
    0
}

/// Drop a process's file descriptors as part of process teardown.
pub fn close_process_files(pid: u64) {
    let mut pipe_refs = alloc::vec::Vec::new();
    OPEN_FILES.lock().retain(|file| {
        if file.pid == pid {
            if matches!(file.kind, FdKind::PipeRead(_) | FdKind::PipeWrite(_)) {
                pipe_refs.push(file.kind);
            }
            false
        } else {
            true
        }
    });
    for kind in pipe_refs {
        drop_pipe_ref(kind);
    }
    USER_PAGE_ALLOCATIONS
        .lock()
        .retain(|allocation| allocation.pid != pid);
}

fn add_pipe_ref(kind: FdKind) {
    let mut pipes = PIPES.lock();
    if let Some(pipe) = pipes.iter_mut().find(|pipe| match kind {
        FdKind::PipeRead(id) | FdKind::PipeWrite(id) => pipe.id == id,
        _ => false,
    }) {
        match kind {
            FdKind::PipeRead(_) => pipe.readers += 1,
            FdKind::PipeWrite(_) => pipe.writers += 1,
            _ => {}
        }
    }
}

fn drop_pipe_ref(kind: FdKind) {
    let id = match kind {
        FdKind::PipeRead(id) | FdKind::PipeWrite(id) => id,
        _ => return,
    };
    let mut pipes = PIPES.lock();
    if let Some(pipe) = pipes.iter_mut().find(|pipe| pipe.id == id) {
        match kind {
            FdKind::PipeRead(_) => pipe.readers = pipe.readers.saturating_sub(1),
            FdKind::PipeWrite(_) => pipe.writers = pipe.writers.saturating_sub(1),
            _ => {}
        }
    }
    pipes.retain(|pipe| pipe.id != id || pipe.readers != 0 || pipe.writers != 0);
}

fn sys_pipe(pointer: u64) -> u64 {
    if !vmm::validate_user_range(pointer, 2 * core::mem::size_of::<usize>(), true) {
        return errno(EFAULT);
    }
    let pid = scheduler::current_id();
    let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
    PIPES.lock().push(PipeState {
        id,
        bytes: alloc::collections::VecDeque::new(),
        readers: 1,
        writers: 1,
    });
    let mut files = OPEN_FILES.lock();
    let read_fd = (3..)
        .find(|fd| !files.iter().any(|file| file.pid == pid && file.fd == *fd))
        .unwrap_or(3);
    let write_fd = (read_fd + 1..)
        .find(|fd| !files.iter().any(|file| file.pid == pid && file.fd == *fd))
        .unwrap_or(read_fd + 1);
    files.push(OpenFile {
        pid,
        fd: read_fd,
        kind: FdKind::PipeRead(id),
        path: alloc::string::String::new(),
        offset: 0,
        readable: true,
        writable: false,
        append: false,
    });
    files.push(OpenFile {
        pid,
        fd: write_fd,
        kind: FdKind::PipeWrite(id),
        path: alloc::string::String::new(),
        offset: 0,
        readable: false,
        writable: true,
        append: false,
    });
    drop(files);
    let result = [read_fd as usize, write_fd as usize];
    if !vmm::copy_to_user(pointer, unsafe {
        core::slice::from_raw_parts(
            result.as_ptr() as *const u8,
            core::mem::size_of_val(&result),
        )
    }) {
        let _ = sys_close(read_fd);
        let _ = sys_close(write_fd);
        return errno(EFAULT);
    }
    0
}

fn sys_mouse_state(pointer: u64) -> u64 {
    let state = crate::drivers::usb::mouse::state();
    let size = core::mem::size_of_val(&state);
    if !vmm::validate_user_range(pointer, size, true) {
        return errno(EFAULT);
    }
    if !vmm::copy_to_user(pointer, unsafe {
        core::slice::from_raw_parts((&state as *const _) as *const u8, size)
    }) {
        return errno(EFAULT);
    }
    0
}

fn net_errno(error: crate::drivers::e1000::Error) -> u64 {
    use crate::drivers::e1000::Error as N;
    errno(match error {
        N::NoDevice => ENODEV,
        N::LinkDown => ENETDOWN,
        N::Busy => EAGAIN,
        N::TxBusy => ENOBUFS,
        N::Timeout => ETIMEDOUT,
        N::ArpTimeout => EHOSTUNREACH,
        N::InvalidFrame => EINVAL,
        N::TcpReset => ECONNRESET,
        N::TcpNotConnected => ENOTCONN,
    })
}

fn sys_net_info(pointer: u64) -> u64 {
    let info = match crate::drivers::e1000::info() {
        Ok(info) => info,
        Err(error) => return net_errno(error),
    };
    let bytes = unsafe {
        core::slice::from_raw_parts((&info as *const _) as *const u8, core::mem::size_of_val(&info))
    };
    if !vmm::copy_to_user(pointer, bytes) { return errno(EFAULT); }
    0
}

fn sys_net_send(pointer: u64, length: u64) -> u64 {
    let length = length as usize;
    if !(14..=1518).contains(&length) { return errno(EINVAL); }
    let mut frame = alloc::vec![0u8; length];
    if !vmm::copy_from_user(&mut frame, pointer) { return errno(EFAULT); }
    match crate::drivers::e1000::transmit(&frame) {
        Ok(()) => length as u64,
        Err(error) => net_errno(error),
    }
}

fn sys_net_receive(pointer: u64, capacity: u64) -> u64 {
    let capacity = (capacity as usize).min(1518);
    if capacity < 14 { return errno(EINVAL); }
    if !vmm::validate_user_range(pointer, capacity, true) { return errno(EFAULT); }
    let mut frame = alloc::vec![0u8; capacity];
    match crate::drivers::e1000::receive(&mut frame) {
        Ok(Some(length)) => {
            if !vmm::copy_to_user(pointer, &frame[..length]) { return errno(EFAULT); }
            length as u64
        }
        Ok(None) => errno(EAGAIN),
        Err(error) => net_errno(error),
    }
}

fn sys_net_dhcp() -> u64 {
    match crate::network::dhcp() {
        Ok(_) => 0,
        Err(error) => net_errno(error),
    }
}

fn sys_net_config(pointer: u64) -> u64 {
    let config = crate::network::config();
    let bytes = unsafe {
        core::slice::from_raw_parts((&config as *const _) as *const u8, core::mem::size_of_val(&config))
    };
    if !vmm::copy_to_user(pointer, bytes) { return errno(EFAULT); }
    0
}

fn sys_udp_send(destination_ptr: u64, destination_port: u64, payload_ptr: u64, length: u64, source_port: u64) -> u64 {
    let length = length as usize;
    if length > 1472 || destination_port == 0 || destination_port > u16::MAX as u64
        || source_port == 0 || source_port > u16::MAX as u64 { return errno(EINVAL); }
    if crate::network::config().configured == 0 { return errno(EADDRNOTAVAIL); }
    let mut destination = [0u8; 4];
    if !vmm::copy_from_user(&mut destination, destination_ptr) { return errno(EFAULT); }
    let mut payload = alloc::vec![0u8; length];
    if !vmm::copy_from_user(&mut payload, payload_ptr) { return errno(EFAULT); }
    match crate::network::udp_send(destination, destination_port as u16, source_port as u16, &payload) {
        Ok(sent) => sent as u64,
        Err(error) => net_errno(error),
    }
}

fn sys_udp_receive(info_ptr: u64, payload_ptr: u64, capacity: u64) -> u64 {
    let capacity = (capacity as usize).min(1472);
    if !vmm::validate_user_range(info_ptr, 8, true)
        || (capacity != 0 && !vmm::validate_user_range(payload_ptr, capacity, true)) {
        return errno(EFAULT);
    }
    let mut payload = alloc::vec![0u8; capacity];
    match crate::network::udp_receive(&mut payload) {
        Ok(Some((source_ip, source_port, length))) => {
            let mut info = [0u8; 8];
            info[..4].copy_from_slice(&source_ip);
            info[4..6].copy_from_slice(&source_port.to_ne_bytes());
            info[6..8].copy_from_slice(&(length as u16).to_ne_bytes());
            if !vmm::copy_to_user(info_ptr, &info) || !vmm::copy_to_user(payload_ptr, &payload[..length]) { return errno(EFAULT); }
            length as u64
        }
        Ok(None) => errno(EAGAIN),
        Err(error) => net_errno(error),
    }
}

fn sys_net_ping(address_ptr: u64, timeout_ms: u64) -> u64 {
    if timeout_ms == 0 || timeout_ms > 10_000 { return errno(EINVAL); }
    if crate::network::config().configured == 0 { return errno(EADDRNOTAVAIL); }
    let mut address = [0u8; 4];
    if !vmm::copy_from_user(&mut address, address_ptr) { return errno(EFAULT); }
    match crate::network::ping(address, timeout_ms) {
        Ok(Some(elapsed)) => elapsed.saturating_add(1),
        Ok(None) => 0,
        Err(error) => net_errno(error),
    }
}

fn sys_tcp_connect(address_ptr: u64, port: u64) -> u64 {
    if port == 0 || port > u16::MAX as u64 { return errno(EINVAL); }
    if crate::network::config().configured == 0 { return errno(EADDRNOTAVAIL); }
    let mut address = [0u8; 4];
    if !vmm::copy_from_user(&mut address, address_ptr) { return errno(EFAULT); }
    match crate::network::tcp_connect(address, port as u16) {
        Ok(()) => 0,
        Err(error) => net_errno(error),
    }
}

fn sys_tcp_send(pointer: u64, length: u64) -> u64 {
    let length = length as usize;
    if length > 64 * 1024 { return errno(EINVAL); }
    if length == 0 { return 0; }
    if !vmm::validate_user_range(pointer, length, false) { return errno(EFAULT); }
    let mut payload = alloc::vec![0u8; length];
    if !vmm::copy_from_user(&mut payload, pointer) { return errno(EFAULT); }
    match crate::network::tcp_send(&payload) {
        Ok(sent) => sent as u64,
        Err(error) => net_errno(error),
    }
}

fn sys_tcp_receive(pointer: u64, capacity: u64, timeout_ms: u64) -> u64 {
    let capacity = capacity as usize;
    if capacity > 8192 || timeout_ms == 0 || timeout_ms > 30_000 { return errno(EINVAL); }
    if capacity != 0 && !vmm::validate_user_range(pointer, capacity, true) { return errno(EFAULT); }
    let mut payload = alloc::vec![0u8; capacity];
    match crate::network::tcp_receive(&mut payload, timeout_ms) {
        Ok(length) => {
            if length != 0 && !vmm::copy_to_user(pointer, &payload[..length]) { return errno(EFAULT); }
            length as u64
        }
        Err(error) => net_errno(error),
    }
}

fn sys_tcp_close() -> u64 {
    match crate::network::tcp_close() {
        Ok(()) => 0,
        Err(error) => net_errno(error),
    }
}

fn sys_dns_lookup(name_ptr: u64, name_len: u64, output_ptr: u64) -> u64 {
    let name_len = name_len as usize;
    if name_len == 0 || name_len > 253 { return errno(EINVAL); }
    let mut name = alloc::vec![0u8; name_len];
    if !vmm::copy_from_user(&mut name, name_ptr) { return errno(EFAULT); }
    let name = match core::str::from_utf8(&name) {
        Ok(name) => name,
        Err(_) => return errno(EINVAL),
    };
    match crate::network::resolve_ipv4(name) {
        Ok(Some(address)) => {
            if !vmm::copy_to_user(output_ptr, &address) { return errno(EFAULT); }
            0
        }
        Ok(None) => errno(ENOENT),
        Err(error) => net_errno(error),
    }
}

fn sys_keyboard_event(pointer: u64) -> u64 {
    if !vmm::validate_user_range(pointer, core::mem::size_of::<crate::input::KeyEvent>(), true) {
        return errno(EFAULT);
    }
    let Some(event) = crate::input::pop(scheduler::current_id()) else { return 0; };
    let bytes = unsafe {
        core::slice::from_raw_parts((&event as *const crate::input::KeyEvent) as *const u8,
            core::mem::size_of::<crate::input::KeyEvent>())
    };
    if !vmm::copy_to_user(pointer, bytes) { return errno(EFAULT); }
    1
}

fn sys_keyboard_focus(operation: u64) -> u64 {
    let pid = scheduler::current_id();
    match operation {
        1 => if crate::input::claim(pid) { 0 } else { errno(16) },
        0 => { crate::input::release(pid); 0 },
        _ => errno(EINVAL),
    }
}

fn sys_seek(fd: u64, offset: i64, whence: u64) -> u64 {
    let pid = scheduler::current_id();
    let mut files = OPEN_FILES.lock();
    let Some(file) = files.iter_mut().find(|file| file.pid == pid && file.fd == fd) else {
        return errno(EBADF);
    };
    if file.kind != FdKind::File { return errno(ESPIPE); }
    let base = match whence {
        0 => 0usize,
        1 => file.offset,
        2 => match crate::vfs::file_size(&file.path) {
            Ok(size) => size,
            Err(error) => return fs_errno(error),
        },
        _ => return errno(EINVAL),
    };
    let Ok(base) = i64::try_from(base) else { return errno(EINVAL); };
    let Some(next) = base.checked_add(offset).filter(|value| *value >= 0) else {
        return errno(EINVAL);
    };
    file.offset = next as usize;
    next as u64
}

fn sys_era_graphics(operation: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    let pid = scheduler::current_id();
    match operation {
        ERA_GRAPHICS_ENTER => {
            if !vmm::validate_user_range(a1, 2 * core::mem::size_of::<u32>(), true) {
                return errno(EFAULT);
            }
            let dimensions = {
                let mut guard = crate::console::CONSOLE.lock();
                let Some(console) = guard.as_mut() else {
                    return errno(ENODEV);
                };
                match console.graphics_enter(pid) {
                    Ok((width, height)) => [width as u32, height as u32],
                    Err(crate::console::GraphicsError::Busy) => return errno(16),
                    Err(crate::console::GraphicsError::OutOfMemory) => return errno(ENOMEM),
                }
            };
            if !vmm::copy_to_user(a1, unsafe {
                core::slice::from_raw_parts(
                    dimensions.as_ptr() as *const u8,
                    core::mem::size_of_val(&dimensions),
                )
            }) {
                if let Some(console) = crate::console::CONSOLE.lock().as_mut() {
                    console.graphics_exit(pid);
                }
                return errno(EFAULT);
            }
            0
        }
        ERA_GRAPHICS_FILL_RECT => {
            let mut guard = crate::console::CONSOLE.lock();
            let Some(console) = guard.as_mut() else {
                return errno(ENODEV);
            };
            if console.graphics_fill_rect(
                pid,
                a1 as i64,
                a2 as i64,
                a3 as usize,
                a4 as usize,
                a5 as u32,
            ) {
                0
            } else {
                errno(13)
            }
        }
        ERA_GRAPHICS_TEXT => {
            let length = a5 as usize;
            if length > ERA_MAX_TEXT_BYTES {
                return errno(EINVAL);
            }
            let mut buffer = [0u8; ERA_MAX_TEXT_BYTES];
            if !vmm::copy_from_user(&mut buffer[..length], a4) {
                return errno(EFAULT);
            }
            let Ok(text) = core::str::from_utf8(&buffer[..length]) else {
                return errno(EINVAL);
            };
            let mut guard = crate::console::CONSOLE.lock();
            let Some(console) = guard.as_mut() else {
                return errno(ENODEV);
            };
            if console.graphics_text(pid, a1 as i64, a2 as i64, a3 as u32, text) {
                0
            } else {
                errno(13)
            }
        }
        ERA_GRAPHICS_IMAGE => {
            let width = a3 as usize;
            let byte_len = a5 as usize;
            if width == 0 || byte_len == 0 || byte_len % 4 != 0
                || byte_len / 4 > ERA_MAX_IMAGE_PIXELS || byte_len / 4 < width {
                return errno(EINVAL);
            }
            let mut bytes = alloc::vec::Vec::new();
            if bytes.try_reserve_exact(byte_len).is_err() { return errno(ENOMEM); }
            bytes.resize(byte_len, 0);
            if !vmm::copy_from_user(&mut bytes, a4) { return errno(EFAULT); }
            let mut pixels = alloc::vec::Vec::new();
            if pixels.try_reserve_exact(byte_len / 4).is_err() { return errno(ENOMEM); }
            pixels.extend(bytes.chunks_exact(4).map(|p| u32::from_ne_bytes([p[0], p[1], p[2], p[3]])));
            let mut guard = crate::console::CONSOLE.lock();
            let Some(console) = guard.as_mut() else { return errno(ENODEV); };
            if console.graphics_image(pid, a1 as i64, a2 as i64, width, &pixels) { 0 } else { errno(13) }
        }
        ERA_GRAPHICS_BLIT_SCALED => {
            let width = a1 as usize;
            let height = a2 as usize;
            let byte_len = a4 as usize;
            let Some(pixel_count) = width.checked_mul(height) else { return errno(EINVAL); };
            if width == 0 || height == 0 || pixel_count > ERA_MAX_IMAGE_PIXELS
                || pixel_count.checked_mul(4) != Some(byte_len) {
                return errno(EINVAL);
            }
            let mut pixels = alloc::vec::Vec::new();
            if pixels.try_reserve_exact(pixel_count).is_err() { return errno(ENOMEM); }
            pixels.resize(pixel_count, 0);
            // The API accepts packed native-endian u32 pixels. Copy directly
            // into their final allocation instead of allocating a byte buffer
            // and converting every pixel into a second allocation each frame.
            let pixel_bytes = unsafe {
                core::slice::from_raw_parts_mut(pixels.as_mut_ptr().cast::<u8>(), byte_len)
            };
            if !vmm::copy_from_user(pixel_bytes, a3) { return errno(EFAULT); }
            let mut guard = crate::console::CONSOLE.lock();
            let Some(console) = guard.as_mut() else { return errno(ENODEV); };
            if console.graphics_blit_scaled(pid, width, height, &pixels) { 0 } else { errno(13) }
        }
        ERA_GRAPHICS_PRESENT => {
            let mouse = crate::drivers::usb::mouse::state();
            let mut guard = crate::console::CONSOLE.lock();
            let Some(console) = guard.as_mut() else {
                return errno(ENODEV);
            };
            let cursor = if a1 == 1 { (-32, -32) } else { (mouse.x, mouse.y) };
            if console.graphics_present(pid, cursor) {
                0
            } else {
                errno(13)
            }
        }
        ERA_GRAPHICS_EXIT => {
            let mut guard = crate::console::CONSOLE.lock();
            let Some(console) = guard.as_mut() else {
                return errno(ENODEV);
            };
            if console.graphics_exit(pid) {
                0
            } else {
                errno(13)
            }
        }
        _ => errno(EINVAL),
    }
}

fn unmap_user_pages(base: u64, pages: usize) {
    for index in 0..pages {
        let address = base + index as u64 * pmm::PAGE_SIZE;
        if let Some(phys) = vmm::unmap_current_user_page(address) {
            pmm::free_frame(phys);
        }
    }
}

fn sys_alloc_pages(page_count: u64) -> u64 {
    let pages = page_count as usize;
    if page_count == 0 || pages > MAX_USER_ALLOCATION_PAGES {
        return errno(EINVAL);
    }
    if vmm::current_cr3() == vmm::kernel_cr3() {
        return errno(ENOMEM);
    }

    let pid = scheduler::current_id();
    let mut allocations = USER_PAGE_ALLOCATIONS.lock();
    let process_page_limit = if process::is_sandboxed(pid) {
        MAX_SANDBOX_HEAP_PAGES
    } else {
        MAX_USER_HEAP_PAGES
    };
    let current_pages: usize = allocations
        .iter()
        .filter(|allocation| allocation.pid == pid)
        .map(|allocation| allocation.pages)
        .sum();
    if current_pages > process_page_limit.saturating_sub(pages) {
        return errno(ENOMEM);
    }

    let bytes = pages as u64 * pmm::PAGE_SIZE;
    let mut base = USER_HEAP_BASE;
    let base = loop {
        let Some(end) = base.checked_add(bytes) else {
            return errno(ENOMEM);
        };
        if end > USER_HEAP_END {
            return errno(ENOMEM);
        }

        if let Some(overlap) = allocations.iter().find(|allocation| {
            allocation.pid == pid
                && allocation.base < end
                && allocation.base + allocation.pages as u64 * pmm::PAGE_SIZE > base
        }) {
            base = overlap.base + overlap.pages as u64 * pmm::PAGE_SIZE;
            continue;
        }

        let occupied = (0..pages)
            .find(|index| vmm::current_user_page_mapped(base + *index as u64 * pmm::PAGE_SIZE));
        if let Some(index) = occupied {
            base += (index as u64 + 1) * pmm::PAGE_SIZE;
            continue;
        }
        break base;
    };

    let mut mapped = 0usize;
    for index in 0..pages {
        let address = base + index as u64 * pmm::PAGE_SIZE;
        let Some(phys) = pmm::alloc_frame_zeroed() else {
            unmap_user_pages(base, mapped);
            return errno(ENOMEM);
        };
        if !vmm::map_current_user_page(address, phys) {
            pmm::free_frame(phys);
            unmap_user_pages(base, mapped);
            return errno(ENOMEM);
        }
        mapped += 1;
    }

    allocations.push(UserPageAllocation { pid, base, pages });
    base
}

fn sys_free_pages(base: u64, page_count: u64) -> u64 {
    let pages = page_count as usize;
    if base & (pmm::PAGE_SIZE - 1) != 0 || page_count == 0 || pages > MAX_USER_ALLOCATION_PAGES {
        return errno(EINVAL);
    }
    let pid = scheduler::current_id();
    let mut allocations = USER_PAGE_ALLOCATIONS.lock();
    let Some(index) = allocations.iter().position(|allocation| {
        allocation.pid == pid && allocation.base == base && allocation.pages == pages
    }) else {
        return errno(EINVAL);
    };

    if (0..pages)
        .any(|offset| !vmm::current_user_page_mapped(base + offset as u64 * pmm::PAGE_SIZE))
    {
        return errno(EINVAL);
    }
    allocations.swap_remove(index);
    unmap_user_pages(base, pages);
    0
}

fn sys_kill(pid: u64, signal: u64) -> u64 {
    let current = scheduler::current_id();
    if pid != current && !process::has_child(current, pid) {
        return errno(ESRCH);
    }
    match signal {
        0 => 0,
        9 | 15 if pid == current => process::exit_current(128 + signal as i64),
        9 | 15 => {
            if process::terminate_child(current, pid, 128 + signal as i64) {
                scheduler::yield_now();
                0
            } else {
                errno(ESRCH)
            }
        }
        _ => errno(ENOSYS),
    }
}

fn copy_user_string(ptr: u64) -> Result<alloc::string::String, u64> {
    let mut bytes = alloc::vec::Vec::new();
    for i in 0..=MAX_NAME {
        let mut byte = [0u8; 1];
        let address = ptr.checked_add(i as u64).ok_or(errno(EFAULT))?;
        if !vmm::copy_from_user(&mut byte, address) {
            return Err(errno(EFAULT));
        }
        if byte[0] == 0 {
            return core::str::from_utf8(&bytes)
                .map(alloc::string::String::from)
                .map_err(|_| errno(EINVAL));
        }
        if i == MAX_NAME {
            return Err(errno(ENAMETOOLONG));
        }
        bytes.push(byte[0]);
    }
    Err(errno(ENAMETOOLONG))
}

fn copy_user_vector(
    ptr: u64,
    fallback: Option<alloc::string::String>,
) -> Result<alloc::vec::Vec<alloc::string::String>, u64> {
    if ptr == 0 {
        return Ok(fallback.into_iter().collect());
    }
    let mut values = alloc::vec::Vec::new();
    for i in 0..32usize {
        let mut raw = [0u8; 8];
        let address = ptr.checked_add((i * 8) as u64).ok_or(errno(EFAULT))?;
        if !vmm::copy_from_user(&mut raw, address) {
            return Err(errno(EFAULT));
        }
        let string_ptr = u64::from_ne_bytes(raw);
        if string_ptr == 0 {
            return Ok(values);
        }
        values.push(copy_user_string(string_ptr)?);
    }
    Err(errno(E2BIG))
}

fn sys_exec(path_ptr: u64, path_len: u64, argv_ptr: u64, env_ptr: u64) -> u64 {
    sys_exec_with_io(path_ptr, path_len, argv_ptr, env_ptr, None, None)
}

fn sys_exec_io(
    path_ptr: u64,
    path_len: u64,
    argv_ptr: u64,
    env_ptr: u64,
    stdin_fd: u64,
    stdout_fd: u64,
) -> u64 {
    let input = (stdin_fd != u64::MAX).then_some(stdin_fd);
    let output = (stdout_fd != u64::MAX).then_some(stdout_fd);
    sys_exec_with_io(path_ptr, path_len, argv_ptr, env_ptr, input, output)
}

fn sys_exec_app_io(
    path_ptr: u64,
    path_len: u64,
    argv_ptr: u64,
    env_ptr: u64,
    stdin_fd: u64,
    stdout_fd: u64,
) -> u64 {
    let input = (stdin_fd != u64::MAX).then_some(stdin_fd);
    let output = (stdout_fd != u64::MAX).then_some(stdout_fd);
    sys_exec_with_io_policy(path_ptr, path_len, argv_ptr, env_ptr, input, output, true)
}

fn sys_exec_with_io(
    path_ptr: u64,
    path_len: u64,
    argv_ptr: u64,
    env_ptr: u64,
    stdin_fd: Option<u64>,
    stdout_fd: Option<u64>,
) -> u64 {
    sys_exec_with_io_policy(path_ptr, path_len, argv_ptr, env_ptr, stdin_fd, stdout_fd, false)
}

fn sys_exec_with_io_policy(
    path_ptr: u64,
    path_len: u64,
    argv_ptr: u64,
    env_ptr: u64,
    stdin_fd: Option<u64>,
    stdout_fd: Option<u64>,
    force_sandbox: bool,
) -> u64 {
    let n = path_len as usize;
    if n > MAX_NAME {
        return errno(ENAMETOOLONG);
    }
    let mut path = alloc::vec![0; n];
    if !vmm::copy_from_user(&mut path, path_ptr) {
        return errno(EFAULT);
    }
    let Ok(path) = core::str::from_utf8(&path) else {
        return errno(ENOENT);
    };
    let path = match crate::vfs::resolve_path(path, &process::cwd(scheduler::current_id())) {
        Ok(path) => path,
        Err(error) => return fs_errno(error),
    };
    let argv = match copy_user_vector(argv_ptr, Some(path.clone())) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let env = match copy_user_vector(env_ptr, None) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let parent = scheduler::current_id();
    if let Some(fd) = stdin_fd {
        if !OPEN_FILES
            .lock()
            .iter()
            .any(|file| file.pid == parent && file.fd == fd && file.readable)
        {
            return errno(EBADF);
        }
    }
    if let Some(fd) = stdout_fd {
        if !OPEN_FILES
            .lock()
            .iter()
            .any(|file| file.pid == parent && file.fd == fd && file.writable)
        {
            return errno(EBADF);
        }
    }
    let spawned = if force_sandbox {
        process::spawn_sandboxed_with_io(&path, &argv, &env, stdin_fd, stdout_fd)
    } else {
        process::spawn_with_io(&path, &argv, &env, stdin_fd, stdout_fd)
    };
    match spawned {
        Ok(pid) => pid,
        Err(process::SpawnError::NotFound) => errno(ENOENT),
        Err(process::SpawnError::Elf(_)) => errno(ENOEXEC),
        Err(process::SpawnError::NoMemory) => errno(ENOMEM),
        Err(process::SpawnError::NoThread) => errno(EAGAIN),
    }
}

fn sys_path_mutation(path_ptr: u64, path_len: u64, make_dir: bool) -> u64 {
    if process::is_sandboxed(scheduler::current_id()) {
        return errno(EPERM);
    }
    let n = path_len as usize;
    if n == 0 || n > MAX_NAME {
        return errno(EINVAL);
    }
    let mut path = alloc::vec![0; n];
    if !vmm::copy_from_user(&mut path, path_ptr) {
        return errno(EFAULT);
    }
    let Ok(path) = core::str::from_utf8(&path) else {
        return errno(EINVAL);
    };
    let path = match crate::vfs::resolve_path(path, &process::cwd(scheduler::current_id())) {
        Ok(path) => path,
        Err(error) => return fs_errno(error),
    };
    let result = if make_dir {
        crate::vfs::create_dir(&path)
    } else {
        crate::vfs::remove(&path)
    };
    match result {
        Ok(()) => 0,
        Err(error) => fs_errno(error),
    }
}

fn sys_chdir(path_ptr: u64, path_len: u64) -> u64 {
    let n = path_len as usize;
    if n == 0 || n > MAX_NAME {
        return errno(EINVAL);
    }
    let mut bytes = alloc::vec![0; n];
    if !vmm::copy_from_user(&mut bytes, path_ptr) {
        return errno(EFAULT);
    }
    let Ok(path) = core::str::from_utf8(&bytes) else {
        return errno(EINVAL);
    };
    let cwd = process::cwd(scheduler::current_id());
    let path = match crate::vfs::resolve_path(path, &cwd) {
        Ok(path) => path,
        Err(error) => return fs_errno(error),
    };
    if let Err(error) = crate::vfs::list(&path) {
        return fs_errno(error);
    }
    if process::set_cwd(scheduler::current_id(), path) {
        0
    } else {
        errno(ESRCH)
    }
}

fn sys_getcwd(buf: u64, len: u64) -> u64 {
    let cwd = process::cwd(scheduler::current_id());
    if cwd.len() > len as usize {
        return errno(ERANGE);
    }
    if !vmm::copy_to_user(buf, cwd.as_bytes()) {
        return errno(EFAULT);
    }
    cwd.len() as u64
}

fn sys_list_directory(path_ptr: u64, path_len: u64, buf: u64, len: u64) -> u64 {
    let n = path_len as usize;
    if n > MAX_NAME {
        return errno(ENAMETOOLONG);
    }
    let mut bytes = alloc::vec![0; n];
    if !vmm::copy_from_user(&mut bytes, path_ptr) {
        return errno(EFAULT);
    }
    let Ok(path) = core::str::from_utf8(&bytes) else {
        return errno(EINVAL);
    };
    let path = match crate::vfs::resolve_path(path, &process::cwd(scheduler::current_id())) {
        Ok(path) => path,
        Err(error) => return fs_errno(error),
    };
    let entries = match crate::vfs::list(&path) {
        Ok(entries) => entries,
        Err(error) => return fs_errno(error),
    };
    let mut output = alloc::vec::Vec::new();
    for entry in entries {
        if !output.is_empty() {
            output.push(b'\n');
        }
        output.extend_from_slice(entry.name.as_bytes());
        if entry.is_dir {
            output.push(b'/');
        }
    }
    if output.len() > len as usize {
        return errno(ERANGE);
    }
    if !vmm::copy_to_user(buf, &output) {
        return errno(EFAULT);
    }
    output.len() as u64
}

fn sys_sync() -> u64 {
    match crate::vfs::sync() {
        Ok(()) => 0,
        Err(error) => fs_errno(error),
    }
}

fn sys_power_control(action: u64) -> u64 {
    if action > 1 {
        return errno(EINVAL);
    }
    let sync_result = sys_sync();
    if sync_result != 0 {
        return sync_result;
    }
    if action == 0 {
        crate::power::reboot();
    }
    if crate::power::shutdown() {
        crate::power::halt();
    }
    errno(ENOTSUP)
}

fn sys_mount(device_ptr: u64, mountpoint_ptr: u64) -> u64 {
    let device_path = match copy_user_string(device_ptr) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let mountpoint = match copy_user_string(mountpoint_ptr) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let Some(device_name) = device_path.strip_prefix("/device/") else {
        return errno(ENODEV);
    };
    if device_name.is_empty() || device_name.contains('/') {
        return errno(ENODEV);
    }
    let Some(device) = crate::block::find_device(device_name) else {
        return errno(ENODEV);
    };
    match crate::vfs::mount_fat32_device(&mountpoint, device_name, device) {
        Ok(()) => 0,
        Err(error) => fs_errno(error),
    }
}

fn sys_is_mountpoint(path_ptr: u64) -> u64 {
    let path = match copy_user_string(path_ptr) {
        Ok(path) => path,
        Err(error) => return error,
    };
    crate::vfs::is_writable_mountpoint(&path) as u64
}

fn sys_waitpid(pid: u64, status_ptr: u64, options: u64) -> u64 {
    const WNOHANG: u64 = 1;
    if options & !WNOHANG != 0 {
        return errno(EINVAL);
    }
    // Reaping is destructive, so reject a bad status pointer before waiting
    // for or removing the child's exit status.
    if status_ptr != 0 && !vmm::validate_user_range(status_ptr, 4, true) {
        return errno(EFAULT);
    }
    let _foreground = if options & WNOHANG == 0 && pid != 0 && pid != u64::MAX {
        FOREGROUND_CHILD.store(pid, Ordering::Release);
        CTRL_C_PENDING.store(false, Ordering::Release);
        Some(ForegroundWaitGuard)
    } else {
        None
    };
    loop {
        match process::take_child_exit_status(scheduler::current_id(), pid) {
            Err(()) => return errno(ECHILD),
            Ok(Some((child_pid, status))) => {
                if status_ptr != 0 && !vmm::copy_to_user(status_ptr, &(status as i32).to_ne_bytes())
                {
                    return errno(EFAULT);
                }
                return child_pid;
            }
            Ok(None) if options & WNOHANG != 0 => return 0,
            Ok(None) => {}
        }
        if pid != 0 && pid != u64::MAX {
            if CTRL_C_PENDING.swap(false, Ordering::AcqRel) {
                TERMINAL_READER.store(false, Ordering::Release);
                if process::terminate_child(scheduler::current_id(), pid, 130) {
                    console_write(b"^C\n");
                }
            } else if let Some(byte) = try_terminal_byte() {
                if byte == 0x03 {
                    CTRL_C_PENDING.store(true, Ordering::Release);
                } else {
                    let mut pending = DEFERRED_INPUT.lock();
                    if pending.len() < 256 {
                        pending.push_back(byte);
                    }
                }
            }
        }
        scheduler::yield_now();
    }
}

struct ForegroundWaitGuard;

impl Drop for ForegroundWaitGuard {
    fn drop(&mut self) {
        FOREGROUND_CHILD.store(0, Ordering::Release);
        CTRL_C_PENDING.store(false, Ordering::Release);
    }
}

/// Poll terminal input while a foreground process is running.
fn try_terminal_byte() -> Option<u8> {
    crate::drivers::usb::xhci::poll();
    crate::drivers::ps2::try_read_byte()
        .or_else(crate::drivers::usb::xhci::try_read_byte)
        .or_else(|| crate::serial::SERIAL1.lock().try_read_byte())
}

/// Longest path or argument string accepted by user-facing syscalls.
const MAX_NAME: usize = 255;

fn sys_list_files(buf: u64, len: u64) -> u64 {
    let cap = (len as usize).min(MAX_WRITE);
    let mut out = alloc::vec::Vec::new();
    for e in crate::vfs::list_files() {
        if !out.is_empty() {
            out.push(b'\n');
        }
        out.extend_from_slice(e.as_bytes());
    }
    if out.len() > cap {
        return errno(ERANGE);
    }
    if !vmm::copy_to_user(buf, &out) {
        return errno(EFAULT);
    }
    out.len() as u64
}

fn sys_list_processes(buf: u64, len: u64) -> u64 {
    let cap = (len as usize).min(MAX_WRITE);
    let mut output = alloc::string::String::from("PID  STATE    NAME\n");
    for process in process::list() {
        let state = if process.exit_status.is_some() {
            "EXITED"
        } else {
            "RUNNING"
        };
        let _ = writeln!(output, "{:<4} {:<8} {}", process.pid, state, process.name);
    }
    if output.len() > cap {
        return errno(ERANGE);
    }
    if !vmm::copy_to_user(buf, output.as_bytes()) {
        return errno(EFAULT);
    }
    output.len() as u64
}

fn sys_memory_info(buf: u64) -> u64 {
    let values = [
        (pmm::free_frame_count() as u64).saturating_mul(pmm::PAGE_SIZE),
        crate::heap::heap_size() as u64,
        crate::heap::allocated_bytes() as u64,
        (pmm::usable_frame_count() as u64).saturating_mul(pmm::PAGE_SIZE),
    ];
    let bytes = unsafe {
        core::slice::from_raw_parts(values.as_ptr() as *const u8, core::mem::size_of_val(&values))
    };
    if !vmm::copy_to_user(buf, bytes) {
        return errno(EFAULT);
    }
    0
}

fn sys_list_mounts(buf: u64, len: u64) -> u64 {
    let output = crate::vfs::mount_listing();
    let cap = (len as usize).min(MAX_WRITE);
    if output.len() > cap {
        return errno(ERANGE);
    }
    if !vmm::copy_to_user(buf, output.as_bytes()) {
        return errno(EFAULT);
    }
    output.len() as u64
}

fn sys_list_block_devices(buf: u64, len: u64) -> u64 {
    let cap = (len as usize).min(MAX_WRITE);
    let mut output = alloc::string::String::from("NAME        FSTYPE FSVER LABEL        UUID                                 FSAVAIL FSUSE% MOUNTPOINTS\n");
    for registered in crate::block::devices() {
        let device = registered.device;
        let info = crate::fs_probe::inspect(device);
        let mountpoint = crate::vfs::mountpoint_for_device(registered.name).unwrap_or_default();
        write_lsblk_row(&mut output, registered.name, &info, &mountpoint, "");
        if let Ok(parts) = crate::partition::scan(device) {
            for (i, part) in parts.iter().enumerate() {
                let view = crate::block::PartitionDevice::new(device, part.start_lba, part.sectors);
                if let Some(view) = view {
                    let info = crate::fs_probe::inspect(&view);
                    let separator = if i + 1 == parts.len() {
                        "└─"
                    } else {
                        "├─"
                    };
                    let part_name = if registered.name.starts_with("nvme") {
                        alloc::format!("{}p{}", registered.name, part.index)
                    } else {
                        alloc::format!("{}{}", registered.name, part.index)
                    };
                    write_lsblk_row(&mut output, &part_name, &info, "", separator);
                }
            }
        }
    }
    if output.len() > cap {
        return errno(ERANGE);
    }
    if !vmm::copy_to_user(buf, output.as_bytes()) {
        return errno(EFAULT);
    }
    output.len() as u64
}

fn write_lsblk_row(
    out: &mut alloc::string::String,
    name: &str,
    info: &crate::fs_probe::FsInfo,
    mount: &str,
    tree: &str,
) {
    use core::fmt::Write;
    let fstype = match info.kind {
        crate::fs_probe::FsType::Fat32 | crate::fs_probe::FsType::Vfat => "vfat",
        crate::fs_probe::FsType::Unknown => "",
        other => other.name(),
    };
    let available = info.available.map(format_lsblk_size).unwrap_or_default();
    let used = info
        .used_percent
        .map(|n| alloc::format!("{}%", n))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "{}{:<10} {:<6} {:<5} {:<12} {:<36} {:>7} {:>6} {}",
        tree, name, fstype, info.version, info.label, info.uuid, available, used, mount
    );
}

fn format_lsblk_size(bytes: u64) -> alloc::string::String {
    const UNITS: [(&str, u64); 4] = [
        ("T", 1u64 << 40),
        ("G", 1u64 << 30),
        ("M", 1u64 << 20),
        ("K", 1u64 << 10),
    ];
    for (unit, base) in UNITS {
        if bytes >= base {
            let tenths = bytes.saturating_mul(10) / base;
            return if tenths % 10 == 0 {
                alloc::format!("{}{}", tenths / 10, unit)
            } else {
                alloc::format!("{}.{}{}", tenths / 10, tenths % 10, unit)
            };
        }
    }
    alloc::format!("{}B", bytes)
}

fn sys_read_file(name_ptr: u64, name_len: u64, buf: u64, buf_len: u64) -> u64 {
    let name_len = name_len as usize;
    if name_len > MAX_NAME {
        return errno(ENAMETOOLONG);
    }
    let mut name = alloc::vec![0u8; name_len];
    if !vmm::copy_from_user(&mut name, name_ptr) {
        return errno(EFAULT);
    }
    let Ok(name) = core::str::from_utf8(&name) else {
        return errno(ENOENT);
    };
    let cap = (buf_len as usize).min(MAX_WRITE);
    let Ok(file_size) = crate::vfs::file_size(name) else {
        return errno(ENOENT);
    };
    if file_size > cap {
        return errno(ERANGE);
    }
    let Ok(data) = crate::vfs::read_all(name) else {
        return errno(ENOENT);
    };
    if data.len() > cap {
        return errno(ERANGE);
    }
    if !vmm::copy_to_user(buf, &data) {
        return errno(EFAULT);
    }
    data.len() as u64
}

fn sys_write(fd: u64, buf: u64, len: u64) -> u64 {
    let n = (len as usize).min(MAX_WRITE);
    if n == 0 {
        return 0;
    }
    let pid = scheduler::current_id();
    let (kind, path, offset, append) = {
        let files = OPEN_FILES.lock();
        let Some(file) = files.iter().find(|f| f.pid == pid && f.fd == fd) else {
            return errno(EBADF);
        };
        if !file.writable {
            return errno(EBADF);
        }
        (file.kind, file.path.clone(), file.offset, file.append)
    };
    let mut data = [0u8; MAX_WRITE];
    if !vmm::copy_from_user(&mut data[..n], buf) {
        return errno(EFAULT);
    }
    if let FdKind::PipeWrite(pipe_id) = kind {
        return write_pipe(pipe_id, &data[..n]);
    }
    if matches!(kind, FdKind::Stdout | FdKind::Stderr | FdKind::Console) {
        console_write(&data[..n]);
        return n as u64;
    }
    if matches!(kind, FdKind::Null | FdKind::Zero | FdKind::Urandom) {
        return n as u64;
    }
    if kind != FdKind::File {
        return errno(EBADF);
    }
    // Serialize append's size lookup and write against other descriptor
    // writes so concurrent appends cannot select the same offset.
    let mut files = OPEN_FILES.lock();
    let Some(file) = files.iter_mut().find(|f| f.pid == pid && f.fd == fd) else {
        return errno(EBADF);
    };
    let offset = if append {
        match crate::vfs::file_size(&path) {
            Ok(size) => size,
            Err(error) => return fs_errno(error),
        }
    } else {
        offset
    };
    let written = match crate::vfs::write_at(&path, offset, &data[..n]) {
        Ok(written) => written,
        Err(error) => return fs_errno(error),
    };
    file.offset = offset + written;
    written as u64
}

fn fs_errno(error: crate::vfs::FsError) -> u64 {
    use crate::vfs::FsError as F;
    errno(match error {
        F::ReadOnly => EROFS,
        F::NoSpace => ENOSPC,
        F::InvalidName | F::InvalidPath => EINVAL,
        F::Io => EIO,
        F::NotFound => ENOENT,
        F::NotDirectory => ENOTDIR,
        F::IsDirectory => EISDIR,
        F::AlreadyExists => EEXIST,
        F::DirectoryNotEmpty => ENOTEMPTY,
        _ => EIO,
    })
}

#[no_mangle]
extern "C" fn syscall_dispatch(frame: &mut SyscallFrame) {
    // `SFMASK` cleared IF on entry because the stub had no stack to
    // trust. The frame now lives on this thread's own kernel stack, so
    // interrupts (and with them preemption) can safely come back on:
    // a syscall may take as long as it likes without stalling the timer.
    crate::idt::enable_interrupts();

    // Third-party applications use pipes and the bounded Era drawing protocol.
    // Hardware, global input focus, power, mounts, and kernel control stay out
    // of their capability set. The policy is checked at the syscall boundary,
    // so a hand-crafted ELF cannot bypass the SDK restrictions.
    let pid = scheduler::current_id();
    if process::is_sandboxed(pid)
        && matches!(
            frame.rax,
            SYS_SYNC
                | SYS_MOUNT
                | SYS_PANIC
                | SYS_POWER_CONTROL
                | SYS_NET_SEND
                | SYS_NET_RECEIVE
                | SYS_NET_DHCP
                | SYS_NET_CONFIG
                | SYS_UDP_SEND
                | SYS_UDP_RECEIVE
                | SYS_NET_PING
                | SYS_DNS_LOOKUP
                | SYS_TCP_CONNECT
                | SYS_TCP_SEND
                | SYS_TCP_RECEIVE
                | SYS_TCP_CLOSE
                | SYS_KEYBOARD_FOCUS
                | SYS_ERA_GRAPHICS
                | SYS_KILL
                | SYS_PIPE
                | SYS_EXEC
                | SYS_EXEC_IO
                | SYS_EXEC_APP_IO
        )
    {
        frame.rax = errno(EPERM);
        return;
    }

    match frame.rax {
        SYS_OPEN => frame.rax = sys_open(frame.rdi, frame.rsi, frame.rdx),
        SYS_CLOSE => frame.rax = sys_close(frame.rdi),
        SYS_READ => frame.rax = sys_read(frame.rdi, frame.rsi, frame.rdx),
        SYS_WRITE => frame.rax = sys_write(frame.rdi, frame.rsi, frame.rdx),
        SYS_LIST_FILES => frame.rax = sys_list_files(frame.rdi, frame.rsi),
        SYS_LIST_PROCESSES => frame.rax = sys_list_processes(frame.rdi, frame.rsi),
        SYS_LIST_BLOCK_DEVICES => frame.rax = sys_list_block_devices(frame.rdi, frame.rsi),
        SYS_LIST_MOUNTS => frame.rax = sys_list_mounts(frame.rdi, frame.rsi),
        SYS_MKDIR => frame.rax = sys_path_mutation(frame.rdi, frame.rsi, true),
        SYS_REMOVE => frame.rax = sys_path_mutation(frame.rdi, frame.rsi, false),
        SYS_SYNC => frame.rax = sys_sync(),
        SYS_MOUNT => frame.rax = sys_mount(frame.rdi, frame.rsi),
        SYS_ALLOC_PAGES => frame.rax = sys_alloc_pages(frame.rdi),
        SYS_FREE_PAGES => frame.rax = sys_free_pages(frame.rdi, frame.rsi),
        SYS_CHDIR => frame.rax = sys_chdir(frame.rdi, frame.rsi),
        SYS_GETCWD => frame.rax = sys_getcwd(frame.rdi, frame.rsi),
        SYS_LIST_DIRECTORY => {
            frame.rax = sys_list_directory(frame.rdi, frame.rsi, frame.rdx, frame.r10)
        }
        SYS_PANIC => panic!(
            "kernel panic requested by process {}",
            scheduler::current_id()
        ),
        SYS_READ_FILE => frame.rax = sys_read_file(frame.rdi, frame.rsi, frame.rdx, frame.r10),
        SYS_UPTIME_MS => frame.rax = pit::uptime_ms(),
        SYS_WALL_TIME => frame.rax = crate::rtc::unix_seconds().unwrap_or(0),
        SYS_SLEEP_MS => {
            scheduler::sleep_ms(frame.rdi);
            frame.rax = 0;
        }
        SYS_EXEC => frame.rax = sys_exec(frame.rdi, frame.rsi, frame.rdx, frame.r10),
        SYS_EXEC_IO => {
            frame.rax = sys_exec_io(
                frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9,
            )
        }
        SYS_PIPE => frame.rax = sys_pipe(frame.rdi),
        SYS_MOUSE_STATE => frame.rax = sys_mouse_state(frame.rdi),
        SYS_MEMORY_INFO => frame.rax = sys_memory_info(frame.rdi),
        SYS_POWER_CONTROL => frame.rax = sys_power_control(frame.rdi),
        SYS_IS_MOUNTPOINT => frame.rax = sys_is_mountpoint(frame.rdi),
        SYS_NET_INFO => frame.rax = sys_net_info(frame.rdi),
        SYS_NET_SEND => frame.rax = sys_net_send(frame.rdi, frame.rsi),
        SYS_NET_RECEIVE => frame.rax = sys_net_receive(frame.rdi, frame.rsi),
        SYS_NET_DHCP => frame.rax = sys_net_dhcp(),
        SYS_NET_CONFIG => frame.rax = sys_net_config(frame.rdi),
        SYS_UDP_SEND => frame.rax = sys_udp_send(frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8),
        SYS_UDP_RECEIVE => frame.rax = sys_udp_receive(frame.rdi, frame.rsi, frame.rdx),
        SYS_NET_PING => frame.rax = sys_net_ping(frame.rdi, frame.rsi),
        SYS_DNS_LOOKUP => frame.rax = sys_dns_lookup(frame.rdi, frame.rsi, frame.rdx),
        SYS_TCP_CONNECT => frame.rax = sys_tcp_connect(frame.rdi, frame.rsi),
        SYS_TCP_SEND => frame.rax = sys_tcp_send(frame.rdi, frame.rsi),
        SYS_TCP_RECEIVE => frame.rax = sys_tcp_receive(frame.rdi, frame.rsi, frame.rdx),
        SYS_TCP_CLOSE => frame.rax = sys_tcp_close(),
        SYS_KEYBOARD_EVENT => frame.rax = sys_keyboard_event(frame.rdi),
        SYS_KEYBOARD_FOCUS => frame.rax = sys_keyboard_focus(frame.rdi),
        SYS_SEEK => frame.rax = sys_seek(frame.rdi, frame.rsi as i64, frame.rdx),
        SYS_PIPE_READ_AVAILABLE => frame.rax = sys_pipe_read_available(frame.rdi, frame.rsi, frame.rdx as usize),
        SYS_PIPE_WRITE_AVAILABLE => frame.rax = sys_pipe_write_available(frame.rdi, frame.rsi, frame.rdx as usize),
        SYS_EXEC_APP_IO => frame.rax = sys_exec_app_io(frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9),
        SYS_ERA_GRAPHICS => {
            frame.rax = sys_era_graphics(
                frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9,
            )
        }
        SYS_WAIT4 => frame.rax = sys_waitpid(frame.rdi, frame.rsi, frame.rdx),
        SYS_KILL => frame.rax = sys_kill(frame.rdi, frame.rsi),
        SYS_SCHED_YIELD => {
            scheduler::yield_now();
            frame.rax = 0;
        }
        SYS_GETPID => frame.rax = scheduler::current_id(),
        // Never returns: records the status, ends the thread, and the
        // scheduler frees the process's address space once it's off the CPU.
        SYS_EXIT => process::exit_current((frame.rdi & 0xff) as i64),
        TEST_SYSCALL_NUM => {
            TEST_ARG.store(frame.rdi, Ordering::SeqCst);
            TEST_DONE.store(true, Ordering::SeqCst);
            // Nothing sensible to sysret to for the self-test's
            // throwaway thread -- tear it down here instead.
            scheduler::exit();
        }
        other => {
            log_debug!(
                "Syscall",
                "Unknown",
                "syscall {:#x} from user mode (rip={:#018x})",
                other,
                frame.rip
            );
            frame.rax = errno(ENOSYS);
        }
    }
}

/// Drop from ring 0 into ring 3 at `entry`, running on `user_rsp`. Never
/// returns to the caller: either the user code eventually `syscall`s
/// into a handler that exits the thread, or (self-test only) it spins.
pub fn enter_user_mode(entry: u64, user_rsp: u64) -> ! {
    unsafe {
        enter_user_mode_asm(
            entry,
            user_rsp,
            (gdt::USER_CS | 3) as u64,
            (gdt::USER_DS | 3) as u64,
        )
    }
}

// ---------------------------------------------------------------------
// Self-test
// ---------------------------------------------------------------------

const TEST_SYSCALL_NUM: u64 = 0x42;
const TEST_MAGIC: u64 = 0xABCD_1234_DEAD_BEEF;

static TEST_DONE: AtomicBool = AtomicBool::new(false);
static TEST_ARG: AtomicU64 = AtomicU64::new(0);

const USER_CODE_VIRT: u64 = 0x0000_0000_0040_0000;
const USER_STACK_TOP_VIRT: u64 = 0x0000_0000_0060_0000;
const USER_STACK_PAGE_VIRT: u64 = USER_STACK_TOP_VIRT - pmm::PAGE_SIZE;

/// Hand-assembled, position-independent (no absolute addresses) machine
/// code for the self-test's "user program":
///
/// ```asm
/// mov rax, 0x42                       ; TEST_SYSCALL_NUM
/// movabs rdi, 0xABCD1234DEADBEEF       ; TEST_MAGIC, passed as arg0
/// syscall
/// 1: jmp 1b                           ; unreachable: the handler above
///                                      ; never sysret's back to us
/// ```
#[rustfmt::skip]
const USER_TEST_PROGRAM: [u8; 24] = [
    0x48, 0xB8, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // mov rax, 0x42
    0x48, 0xBF, 0xEF, 0xBE, 0xAD, 0xDE, 0x34, 0x12, 0xCD, 0xAB, // movabs rdi, 0xABCD1234DEADBEEF
    0x0F, 0x05,                                                 // syscall
    0xEB, 0xFE,                                                 // jmp $
];

fn user_test_thread() {
    let Some(code_phys) = pmm::alloc_frame_zeroed() else {
        log_fail!("Syscall", "SelfTest", "No frame for the user code page");
        scheduler::exit();
    };
    let Some(stack_phys) = pmm::alloc_frame_zeroed() else {
        log_fail!("Syscall", "SelfTest", "No frame for the user stack page");
        scheduler::exit();
    };

    unsafe {
        let dst = pmm::phys_to_virt(code_phys);
        core::ptr::copy_nonoverlapping(USER_TEST_PROGRAM.as_ptr(), dst, USER_TEST_PROGRAM.len());
    }

    vmm::map_page(USER_CODE_VIRT, code_phys, vmm::USER_RX);
    vmm::map_page(USER_STACK_PAGE_VIRT, stack_phys, vmm::USER_RW);

    enter_user_mode(USER_CODE_VIRT, USER_STACK_TOP_VIRT);
}

pub fn self_test() {
    TEST_DONE.store(false, Ordering::SeqCst);
    TEST_ARG.store(0, Ordering::SeqCst);

    if scheduler::spawn("usertest", user_test_thread).is_none() {
        log_fail!(
            "Syscall",
            "SelfTest",
            "Could not spawn the user-mode test thread"
        );
        return;
    }

    let deadline = pit::uptime_ms() + 2000;
    while !TEST_DONE.load(Ordering::SeqCst) {
        if pit::uptime_ms() > deadline {
            log_fail!(
                "Syscall",
                "SelfTest",
                "Timed out waiting for a syscall from ring 3"
            );
            return;
        }
        scheduler::yield_now();
    }

    // Let the test thread actually get reaped before freeing the pages
    // it was running on top of.
    scheduler::sleep_ms(20);

    let code_phys = vmm::translate(USER_CODE_VIRT);
    let stack_phys = vmm::translate(USER_STACK_PAGE_VIRT);
    vmm::unmap_page(USER_CODE_VIRT);
    vmm::unmap_page(USER_STACK_PAGE_VIRT);
    if let Some(p) = code_phys {
        pmm::free_frame(p);
    }
    if let Some(p) = stack_phys {
        pmm::free_frame(p);
    }

    let arg = TEST_ARG.load(Ordering::SeqCst);
    if arg != TEST_MAGIC {
        log_fail!(
            "Syscall",
            "SelfTest",
            "syscall arrived but rdi was {:#018x}, expected {:#018x}",
            arg,
            TEST_MAGIC
        );
        return;
    }

    log_ok!(
        "Syscall",
        "SelfTest",
        "Dropped to ring 3 (cs={:#04x}), `syscall` reached the kernel with rdi={:#018x}, thread torn down cleanly",
        gdt::USER_CS | 3,
        arg
    );
}

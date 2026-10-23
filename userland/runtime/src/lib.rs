#![no_std]

extern crate alloc;

pub mod era;
pub mod graphics;

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ffi::{c_char, CStr};
use core::fmt::{self, Write as _};
use core::marker::PhantomData;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};

core::arch::global_asm!(
    ".section .text",
    ".global _start",
    ".type _start,@function",
    "_start:",
    "xor rbp, rbp",
    "mov rdi, rsp",
    "and rsp, -16",
    "call eonos_runtime_start",
    "ud2",
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Error(pub i32);

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
    pub _padding: [u8; 3],
    pub wheel: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MemoryInfo {
    pub free_physical: u64,
    pub heap_total: u64,
    pub heap_used: u64,
    pub total_physical: u64,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct NetworkInfo {
    pub mac: [u8; 6],
    pub link_up: u8,
    pub _reserved: u8,
    pub received: u64,
    pub transmitted: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct Ipv4Config {
    pub address: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub server: [u8; 4],
    pub dns_server: [u8; 4],
    pub lease_seconds: u32,
    pub configured: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct UdpDatagramInfo {
    pub source_ip: [u8; 4],
    pub source_port: u16,
    pub length: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct KeyEvent {
    pub usage: u8,
    pub pressed: u8,
    pub modifiers: u8,
    pub _reserved: u8,
}

pub type Result<T> = core::result::Result<T, Error>;

const SYS_READ: usize = 0;
const SYS_WRITE: usize = 1;
const SYS_OPEN: usize = 2;
const SYS_CLOSE: usize = 3;
const SYS_GETPID: usize = 39;
const SYS_EXIT: usize = 60;
const SYS_WAIT4: usize = 61;
const SYS_UPTIME_MS: usize = 9003;
const SYS_WALL_TIME: usize = 9045;
const SYS_SLEEP_MS: usize = 9004;
const SYS_SYNC: usize = 9009;
const SYS_EXEC: usize = 9005;
const SYS_LIST_FILES: usize = 9001;
const SYS_LIST_PROCESSES: usize = 9006;
const SYS_MKDIR: usize = 9007;
const SYS_REMOVE: usize = 9008;
const SYS_LIST_BLOCK_DEVICES: usize = 9010;
const SYS_PANIC: usize = 9011;
const SYS_MOUNT: usize = 9012;
const SYS_ALLOC_PAGES: usize = 9013;
const SYS_FREE_PAGES: usize = 9014;
const SYS_CHDIR: usize = 9015;
const SYS_GETCWD: usize = 9016;
const SYS_LIST_DIRECTORY: usize = 9017;
const SYS_EXEC_IO: usize = 9018;
const SYS_EXEC_APP_IO: usize = 9040;
const SYS_EXEC_WEB_FETCH: usize = 9050;
const SYS_PIPE: usize = 9019;
const SYS_MOUSE_STATE: usize = 9020;
const SYS_MEMORY_INFO: usize = 9022;
const SYS_POWER_CONTROL: usize = 9023;
const SYS_IS_MOUNTPOINT: usize = 9024;
const SYS_NET_INFO: usize = 9025;
const SYS_NET_SEND: usize = 9026;
const SYS_NET_RECEIVE: usize = 9027;
const SYS_LIST_MOUNTS: usize = 9028;
const SYS_NET_DHCP: usize = 9029;
const SYS_NET_CONFIG: usize = 9030;
const SYS_UDP_SEND: usize = 9031;
const SYS_UDP_RECEIVE: usize = 9032;
const SYS_NET_PING: usize = 9033;
const SYS_DNS_LOOKUP: usize = 9034;
const SYS_TCP_CONNECT: usize = 9041;
const SYS_TCP_SEND: usize = 9042;
const SYS_TCP_RECEIVE: usize = 9043;
const SYS_TCP_CLOSE: usize = 9044;
const SYS_SECURE_RANDOM: usize = 9046;
const SYS_KILL: usize = 62;
const SYS_SCHED_YIELD: usize = 24;
const SYS_KEYBOARD_EVENT: usize = 9035;
const SYS_KEYBOARD_FOCUS: usize = 9036;
const SYS_KEYBOARD_LAYOUT: usize = 9047;
const SYS_SEEK: usize = 9037;
const SYS_PIPE_READ_AVAILABLE: usize = 9038;
const SYS_PIPE_WRITE_AVAILABLE: usize = 9039;

const PAGE_SIZE: usize = 4096;
const ALLOCATION_MAGIC: usize = 0xE0A5_A110_CAFE_BEEF;
const SIZE_CLASSES: [usize; 6] = [64, 128, 256, 512, 1024, 2048];
const SMALL_ALLOCATION_LIMIT: usize = 2048;

pub const O_RDONLY: usize = 0;

pub const O_WRONLY: usize = 1;

pub const O_RDWR: usize = 2;

pub const O_CREAT: usize = 0x40;

pub const O_TRUNC: usize = 0x200;

pub const O_APPEND: usize = 0x400;

pub fn list_files(buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe {
        syscall3(
            SYS_LIST_FILES,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            0,
        )
    })
}

pub fn list_processes(buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe {
        syscall3(
            SYS_LIST_PROCESSES,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            0,
        )
    })
}

pub fn list_block_devices(buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe {
        syscall3(
            SYS_LIST_BLOCK_DEVICES,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            0,
        )
    })
}

pub fn list_mounts(buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe {
        syscall3(
            SYS_LIST_MOUNTS,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            0,
        )
    })
}

pub fn mkdir(path: &str) -> Result<()> {
    checked(unsafe { syscall3(SYS_MKDIR, path.as_ptr() as usize, path.len(), 0) }).map(|_| ())
}

pub fn remove(path: &str) -> Result<()> {
    checked(unsafe { syscall3(SYS_REMOVE, path.as_ptr() as usize, path.len(), 0) }).map(|_| ())
}

pub fn mount(device: &CStr, mountpoint: &CStr) -> Result<()> {
    checked(unsafe {
        syscall3(
            SYS_MOUNT,
            device.as_ptr() as usize,
            mountpoint.as_ptr() as usize,
            0,
        )
    })
    .map(|_| ())
}

pub fn panic_kernel() -> ! {
    unsafe { syscall3(SYS_PANIC, 0, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

#[inline(always)]
unsafe fn syscall3(number: usize, a1: usize, a2: usize, a3: usize) -> isize {
    let result: usize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") number => result,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    result as isize
}

#[inline(always)]
unsafe fn syscall4(number: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    let result: usize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") number => result,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("r10") a4,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    result as isize
}

#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
) -> isize {
    let result: usize;
    core::arch::asm!(
        "syscall",
        inlateout("rax") number => result,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("r10") a4,
        in("r8") a5,
        in("r9") a6,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    result as isize
}

fn checked(raw: isize) -> Result<usize> {
    if raw < 0 {
        Err(Error((-raw) as i32))
    } else {
        Ok(raw as usize)
    }
}

pub fn read(fd: usize, buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe { syscall3(SYS_READ, fd, buffer.as_mut_ptr() as usize, buffer.len()) })
}

pub fn write(fd: usize, buffer: &[u8]) -> Result<usize> {
    checked(unsafe { syscall3(SYS_WRITE, fd, buffer.as_ptr() as usize, buffer.len()) })
}

pub fn write_all(fd: usize, mut buffer: &[u8]) -> Result<()> {
    while !buffer.is_empty() {
        let written = write(fd, buffer)?;
        if written == 0 {
            return Err(Error(5)); 
        }
        buffer = &buffer[written..];
    }
    Ok(())
}

pub fn open(path: &str, flags: usize) -> Result<usize> {
    checked(unsafe { syscall3(SYS_OPEN, path.as_ptr() as usize, path.len(), flags) })
}

pub fn pipe() -> Result<(usize, usize)> {
    let mut descriptors = [0usize; 2];
    checked(unsafe {
        syscall3(
            SYS_PIPE,
            descriptors.as_mut_ptr() as usize,
            0,
            0,
        )
    })?;
    Ok((descriptors[0], descriptors[1]))
}

pub fn start_web_fetch(url: &str, stdout_fd: usize) -> Result<usize> {
    checked(unsafe {
        syscall3(SYS_EXEC_WEB_FETCH, url.as_ptr() as usize, url.len(), stdout_fd)
    })
}

pub fn pipe_read_available(fd: usize, buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe {
        syscall3(
            SYS_PIPE_READ_AVAILABLE,
            fd,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
        )
    })
}

pub fn pipe_write_available(fd: usize, buffer: &[u8]) -> Result<usize> {
    checked(unsafe {
        syscall3(
            SYS_PIPE_WRITE_AVAILABLE,
            fd,
            buffer.as_ptr() as usize,
            buffer.len(),
        )
    })
}

pub fn mouse_state() -> Result<MouseState> {
    let mut state = MouseState::default();
    checked(unsafe {
        syscall3(
            SYS_MOUSE_STATE,
            (&mut state as *mut MouseState) as usize,
            0,
            0,
        )
    })?;
    Ok(state)
}

pub fn memory_info() -> Result<MemoryInfo> {
    let mut info = MemoryInfo::default();
    checked(unsafe {
        syscall3(SYS_MEMORY_INFO, (&mut info as *mut MemoryInfo) as usize, 0, 0)
    })?;
    Ok(info)
}

pub fn chdir(path: &str) -> Result<()> {
    checked(unsafe { syscall3(SYS_CHDIR, path.as_ptr() as usize, path.len(), 0) }).map(|_| ())
}

pub fn getcwd(buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe { syscall3(SYS_GETCWD, buffer.as_mut_ptr() as usize, buffer.len(), 0) })
}

pub fn list_directory(path: &str, buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe {
        syscall4(SYS_LIST_DIRECTORY, path.as_ptr() as usize, path.len(), buffer.as_mut_ptr() as usize, buffer.len())
    })
}

pub fn close(fd: usize) -> Result<()> {
    checked(unsafe { syscall3(SYS_CLOSE, fd, 0, 0) }).map(|_| ())
}

pub fn seek(fd: usize, offset: i64, whence: usize) -> Result<usize> {
    checked(unsafe { syscall3(SYS_SEEK, fd, offset as usize, whence) })
}

pub fn keyboard_claim() -> Result<()> {
    checked(unsafe { syscall3(SYS_KEYBOARD_FOCUS, 1, 0, 0) }).map(|_| ())
}

pub fn keyboard_release() -> Result<()> {
    checked(unsafe { syscall3(SYS_KEYBOARD_FOCUS, 0, 0, 0) }).map(|_| ())
}

pub fn keyboard_event() -> Result<Option<KeyEvent>> {
    let mut event = KeyEvent::default();
    let count = checked(unsafe {
        syscall3(SYS_KEYBOARD_EVENT, (&mut event as *mut KeyEvent) as usize, 0, 0)
    })?;
    Ok((count != 0).then_some(event))
}

/// Current text layout: 0 = US, 1 = German, 2 = Dutch.
pub fn keyboard_layout() -> Result<u8> {
    checked(unsafe { syscall3(SYS_KEYBOARD_LAYOUT, 0, 0, 0) }).map(|value| value as u8)
}

/// Select a system-wide text layout: 0 = US, 1 = German, 2 = Dutch.
pub fn set_keyboard_layout(layout: u8) -> Result<()> {
    if layout > 2 { return Err(Error(22)); }
    checked(unsafe { syscall3(SYS_KEYBOARD_LAYOUT, layout as usize + 1, 0, 0) }).map(|_| ())
}

pub fn getpid() -> usize {
    unsafe { syscall3(SYS_GETPID, 0, 0, 0) as usize }
}

pub fn uptime_ms() -> usize {
    unsafe { syscall3(SYS_UPTIME_MS, 0, 0, 0) as usize }
}

pub fn wall_time_secs() -> Option<u64> {
    let seconds = unsafe { syscall3(SYS_WALL_TIME, 0, 0, 0) };
    (seconds > 0).then_some(seconds as u64)
}

pub fn sleep_ms(milliseconds: usize) -> Result<()> {
    checked(unsafe { syscall3(SYS_SLEEP_MS, milliseconds, 0, 0) }).map(|_| ())
}

pub fn yield_now() {
    let _ = unsafe { syscall3(SYS_SCHED_YIELD, 0, 0, 0) };
}

pub fn sync() -> Result<()> {
    checked(unsafe { syscall3(SYS_SYNC, 0, 0, 0) }).map(|_| ())
}

pub fn reboot() -> Result<()> {
    checked(unsafe { syscall3(SYS_POWER_CONTROL, 0, 0, 0) }).map(|_| ())
}

pub fn shutdown() -> Result<()> {
    checked(unsafe { syscall3(SYS_POWER_CONTROL, 1, 0, 0) }).map(|_| ())
}

pub fn is_mountpoint(path: &CStr) -> Result<bool> {
    checked(unsafe { syscall3(SYS_IS_MOUNTPOINT, path.as_ptr() as usize, 0, 0) })
        .map(|mounted| mounted != 0)
}

pub fn network_info() -> Result<NetworkInfo> {
    let mut info = NetworkInfo::default();
    checked(unsafe { syscall3(SYS_NET_INFO, (&mut info as *mut NetworkInfo) as usize, 0, 0) })?;
    Ok(info)
}

pub fn ipv4_config() -> Result<Ipv4Config> {
    let mut config = Ipv4Config::default();
    checked(unsafe { syscall3(SYS_NET_CONFIG, (&mut config as *mut Ipv4Config) as usize, 0, 0) })?;
    Ok(config)
}

pub fn dhcp() -> Result<()> {
    checked(unsafe { syscall3(SYS_NET_DHCP, 0, 0, 0) }).map(|_| ())
}

pub fn udp_send(destination: [u8; 4], destination_port: u16, source_port: u16, payload: &[u8]) -> Result<usize> {
    checked(unsafe {
        syscall6(
            SYS_UDP_SEND,
            destination.as_ptr() as usize,
            destination_port as usize,
            payload.as_ptr() as usize,
            payload.len(),
            source_port as usize,
            0,
        )
    })
}

pub fn udp_receive(info: &mut UdpDatagramInfo, buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe {
        syscall4(
            SYS_UDP_RECEIVE,
            (info as *mut UdpDatagramInfo) as usize,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            0,
        )
    })
}

pub fn ping(destination: [u8; 4], timeout_ms: u64) -> Result<Option<u64>> {
    let result = checked(unsafe {
        syscall3(SYS_NET_PING, destination.as_ptr() as usize, timeout_ms as usize, 0)
    })?;
    Ok(if result == 0 { None } else { Some((result - 1) as u64) })
}

pub fn tcp_connect(destination: [u8; 4], port: u16) -> Result<()> {
    checked(unsafe {
        syscall3(SYS_TCP_CONNECT, destination.as_ptr() as usize, port as usize, 0)
    }).map(|_| ())
}

pub fn tcp_send(bytes: &[u8]) -> Result<usize> {
    checked(unsafe { syscall3(SYS_TCP_SEND, bytes.as_ptr() as usize, bytes.len(), 0) })
}

pub fn tcp_receive(buffer: &mut [u8], timeout_ms: u64) -> Result<usize> {
    checked(unsafe {
        syscall4(SYS_TCP_RECEIVE, buffer.as_mut_ptr() as usize, buffer.len(), timeout_ms as usize, 0)
    })
}

pub fn tcp_close() -> Result<()> {
    checked(unsafe { syscall3(SYS_TCP_CLOSE, 0, 0, 0) }).map(|_| ())
}

pub fn secure_random(buffer: &mut [u8]) -> Result<()> {
    checked(unsafe { syscall3(SYS_SECURE_RANDOM, buffer.as_mut_ptr() as usize, buffer.len(), 0) })
        .map(|_| ())
}

pub fn resolve_ipv4(hostname: &str) -> Result<[u8; 4]> {
    let mut address = [0u8; 4];
    checked(unsafe {
        syscall4(
            SYS_DNS_LOOKUP,
            hostname.as_ptr() as usize,
            hostname.len(),
            address.as_mut_ptr() as usize,
            0,
        )
    })?;
    Ok(address)
}

pub fn send_ethernet_frame(frame: &[u8]) -> Result<usize> {
    checked(unsafe { syscall3(SYS_NET_SEND, frame.as_ptr() as usize, frame.len(), 0) })
}

pub fn receive_ethernet_frame(buffer: &mut [u8]) -> Result<usize> {
    checked(unsafe { syscall3(SYS_NET_RECEIVE, buffer.as_mut_ptr() as usize, buffer.len(), 0) })
}

pub fn wait4(pid: isize, status: &mut i32, options: usize) -> Result<usize> {
    checked(unsafe {
        syscall3(
            SYS_WAIT4,
            pid as usize,
            status as *mut i32 as usize,
            options,
        )
    })
}

pub fn kill(pid: usize, signal: usize) -> Result<()> {
    checked(unsafe { syscall3(SYS_KILL, pid, signal, 0) }).map(|_| ())
}

pub fn exec(path: &str, argv: &[&CStr], env: &[&CStr]) -> Result<usize> {
    exec_with_stdio(path, argv, env, None, None)
}

pub fn exec_with_stdio(
    path: &str,
    argv: &[&CStr],
    env: &[&CStr],
    stdin_fd: Option<usize>,
    stdout_fd: Option<usize>,
) -> Result<usize> {
    let argv_ptrs: alloc::vec::Vec<*const c_char> = argv
        .iter()
        .map(|value| value.as_ptr())
        .chain(core::iter::once(core::ptr::null()))
        .collect();
    let env_ptrs: alloc::vec::Vec<*const c_char> = env
        .iter()
        .map(|value| value.as_ptr())
        .chain(core::iter::once(core::ptr::null()))
        .collect();
    let (input, output) = if stdin_fd.is_none() && stdout_fd.is_none() {
        let pid = checked(unsafe {
            syscall4(
                SYS_EXEC,
                path.as_ptr() as usize,
                path.len(),
                argv_ptrs.as_ptr() as usize,
                env_ptrs.as_ptr() as usize,
            )
        })?;
        return Ok(pid);
    } else {
        (stdin_fd.unwrap_or(usize::MAX), stdout_fd.unwrap_or(usize::MAX))
    };
    checked(unsafe {
        syscall6(
            SYS_EXEC_IO,
            path.as_ptr() as usize,
            path.len(),
            argv_ptrs.as_ptr() as usize,
            env_ptrs.as_ptr() as usize,
            input,
            output,
        )
    })
}

pub fn exec_sandboxed_with_stdio(
    path: &str,
    argv: &[&CStr],
    env: &[&CStr],
    stdin_fd: Option<usize>,
    stdout_fd: Option<usize>,
) -> Result<usize> {
    let argv_ptrs: alloc::vec::Vec<*const c_char> = argv
        .iter()
        .map(|value| value.as_ptr())
        .chain(core::iter::once(core::ptr::null()))
        .collect();
    let env_ptrs: alloc::vec::Vec<*const c_char> = env
        .iter()
        .map(|value| value.as_ptr())
        .chain(core::iter::once(core::ptr::null()))
        .collect();
    checked(unsafe {
        syscall6(
            SYS_EXEC_APP_IO,
            path.as_ptr() as usize,
            path.len(),
            argv_ptrs.as_ptr() as usize,
            env_ptrs.as_ptr() as usize,
            stdin_fd.unwrap_or(usize::MAX),
            stdout_fd.unwrap_or(usize::MAX),
        )
    })
}

fn allocate_pages(pages: usize) -> Result<usize> {
    checked(unsafe { syscall3(SYS_ALLOC_PAGES, pages, 0, 0) })
}

fn free_pages(base: usize, pages: usize) -> Result<()> {
    checked(unsafe { syscall3(SYS_FREE_PAGES, base, pages, 0) }).map(|_| ())
}

#[repr(C, align(16))]
struct AllocationHeader {
    base: usize,
    pages: usize,
    class: usize,
    magic: usize,
}

struct FreeNode {
    next: *mut FreeNode,
}

struct AllocatorState {
    free_lists: [*mut FreeNode; SIZE_CLASSES.len()],
}

impl AllocatorState {
    const fn new() -> Self {
        Self {
            free_lists: [core::ptr::null_mut(); SIZE_CLASSES.len()],
        }
    }
}

pub struct UserAllocator {
    locked: AtomicBool,
    state: UnsafeCell<AllocatorState>,
}

unsafe impl Sync for UserAllocator {}

impl UserAllocator {

    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            state: UnsafeCell::new(AllocatorState::new()),
        }
    }

    fn lock(&self) -> AllocatorGuard<'_> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        AllocatorGuard(&self.locked)
    }
}

struct AllocatorGuard<'a>(&'a AtomicBool);

impl Drop for AllocatorGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

unsafe impl GlobalAlloc for UserAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _guard = self.lock();
        let alignment = layout
            .align()
            .max(core::mem::align_of::<AllocationHeader>());
        let Some(total) = layout
            .size()
            .max(1)
            .checked_add(core::mem::size_of::<AllocationHeader>())
            .and_then(|size| size.checked_add(alignment - 1))
        else {
            return core::ptr::null_mut();
        };

        if alignment <= 16 && total <= SMALL_ALLOCATION_LIMIT {
            let Some(class_index) = SIZE_CLASSES.iter().position(|size| *size >= total) else {
                return core::ptr::null_mut();
            };
            let class_size = SIZE_CLASSES[class_index];
            let state = &mut *self.state.get();
            if state.free_lists[class_index].is_null() {
                let Ok(slab) = allocate_pages(1) else {
                    return core::ptr::null_mut();
                };
                let blocks = PAGE_SIZE / class_size;
                for index in 0..blocks {
                    let node = (slab + index * class_size) as *mut FreeNode;
                    node.write(FreeNode {
                        next: state.free_lists[class_index],
                    });
                    state.free_lists[class_index] = node;
                }
            }
            let block = state.free_lists[class_index];
            state.free_lists[class_index] = (*block).next;
            let header = block as *mut AllocationHeader;
            header.write(AllocationHeader {
                base: block as usize,
                pages: 0,
                class: class_index + 1,
                magic: ALLOCATION_MAGIC,
            });
            return block.add(1) as *mut u8;
        }

        let Some(rounded_total) = total.checked_add(PAGE_SIZE - 1) else {
            return core::ptr::null_mut();
        };
        let pages = rounded_total / PAGE_SIZE;
        let Ok(base) = allocate_pages(pages) else {
            return core::ptr::null_mut();
        };
        let Some(start) = base.checked_add(core::mem::size_of::<AllocationHeader>()) else {
            let _ = free_pages(base, pages);
            return core::ptr::null_mut();
        };
        let Some(aligned_input) = start.checked_add(alignment - 1) else {
            let _ = free_pages(base, pages);
            return core::ptr::null_mut();
        };
        let aligned = aligned_input & !(alignment - 1);
        let header = (aligned - core::mem::size_of::<AllocationHeader>()) as *mut AllocationHeader;
        header.write(AllocationHeader {
            base,
            pages,
            class: 0,
            magic: ALLOCATION_MAGIC,
        });
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        if pointer.is_null() {
            return;
        }
        let _guard = self.lock();
        let header = pointer.sub(core::mem::size_of::<AllocationHeader>()) as *mut AllocationHeader;
        let allocation = header.read();
        if allocation.magic != ALLOCATION_MAGIC {
            return;
        }
        (*header).magic = 0;
        if allocation.class > 0 && allocation.class <= SIZE_CLASSES.len() {
            let class_index = allocation.class - 1;
            let state = &mut *self.state.get();
            let node = header as *mut FreeNode;
            node.write(FreeNode {
                next: state.free_lists[class_index],
            });
            state.free_lists[class_index] = node;
        } else {
            let _ = free_pages(allocation.base, allocation.pages);
        }
    }
}

pub fn exit(status: i32) -> ! {
    unsafe {
        syscall3(SYS_EXIT, status as usize, 0, 0);
    }
    loop {
        core::hint::spin_loop();
    }
}

pub struct Args<'a> {
    argv: *const *const c_char,
    len: usize,
    index: usize,
    envp: *const *const c_char,
    lifetime: PhantomData<&'a CStr>,
}

impl<'a> Args<'a> {

    #[doc(hidden)]
    pub unsafe fn from_raw(
        len: usize,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> Self {
        Self {
            argv,
            len,
            index: 0,
            envp,
            lifetime: PhantomData,
        }
    }

    pub fn envs(&self) -> EnvVars<'_> {
        EnvVars {
            envp: self.envp,
            index: 0,
            lifetime: PhantomData,
        }
    }
}

impl<'a> Iterator for Args<'a> {
    type Item = &'a CStr;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }
        let ptr = unsafe { *self.argv.add(self.index) };
        self.index += 1;
        Some(unsafe { CStr::from_ptr(ptr) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Args<'_> {}

pub struct EnvVars<'a> {
    envp: *const *const c_char,
    index: usize,
    lifetime: PhantomData<&'a CStr>,
}

impl<'a> Iterator for EnvVars<'a> {
    type Item = &'a CStr;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = unsafe { *self.envp.add(self.index) };
        if ptr.is_null() {
            return None;
        }
        self.index += 1;
        Some(unsafe { CStr::from_ptr(ptr) })
    }
}

const FORMAT_BUFFER_SIZE: usize = 1024;

struct BufferedConsoleWriter {
    fd: usize,
    buffer: [u8; FORMAT_BUFFER_SIZE],
    length: usize,
}

impl BufferedConsoleWriter {
    fn flush(&mut self) -> fmt::Result {
        if self.length == 0 {
            return Ok(());
        }
        write_all(self.fd, &self.buffer[..self.length]).map_err(|_| fmt::Error)?;
        self.length = 0;
        Ok(())
    }
}

impl fmt::Write for BufferedConsoleWriter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut remaining = text.as_bytes();
        while !remaining.is_empty() {
            let available = self.buffer.len() - self.length;
            if available == 0 {
                self.flush()?;
                continue;
            }
            let count = available.min(remaining.len());
            self.buffer[self.length..self.length + count].copy_from_slice(&remaining[..count]);
            self.length += count;
            remaining = &remaining[count..];
        }
        Ok(())
    }
}

pub fn write_fmt(arguments: fmt::Arguments<'_>) -> fmt::Result {
    let mut writer = BufferedConsoleWriter {
        fd: 1,
        buffer: [0; FORMAT_BUFFER_SIZE],
        length: 0,
    };
    writer.write_fmt(arguments)?;
    writer.flush()
}

pub fn write_err_fmt(arguments: fmt::Arguments<'_>) -> fmt::Result {
    let mut writer = BufferedConsoleWriter {
        fd: 2,
        buffer: [0; FORMAT_BUFFER_SIZE],
        length: 0,
    };
    writer.write_fmt(arguments)?;
    writer.flush()
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = $crate::write_fmt(core::format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{
        let _ = $crate::write_fmt(core::format_args!("{}\n", core::format_args!($($arg)*)));
    }};
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        let _ = $crate::write_err_fmt(core::format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! eprintln {
    () => { $crate::eprint!("\n") };
    ($($arg:tt)*) => {{
        let _ = $crate::write_err_fmt(core::format_args!("{}\n", core::format_args!($($arg)*)));
    }};
}

#[macro_export]
macro_rules! entry {
    ($main:path) => {
        #[global_allocator]
        static EONOS_ALLOCATOR: $crate::UserAllocator = $crate::UserAllocator::new();

        #[alloc_error_handler]
        fn eonos_alloc_error(layout: core::alloc::Layout) -> ! {
            $crate::eprintln!("allocation failed ({} bytes)", layout.size());
            $crate::exit(12)
        }

        #[no_mangle]
        pub extern "C" fn eonos_main(
            argc: usize,
            argv: *const *const core::ffi::c_char,
            envp: *const *const core::ffi::c_char,
        ) -> i32 {
            let args = unsafe { $crate::Args::from_raw(argc, argv, envp) };
            $main(args)
        }
    };
}

extern "C" {
    fn eonos_main(
        argc: usize,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> i32;
}

#[no_mangle]
pub extern "C" fn eonos_runtime_start(initial_stack: *const usize) -> ! {
    let argc = unsafe { *initial_stack };
    let argv = unsafe { initial_stack.add(1) as *const *const c_char };
    let envp = unsafe { argv.add(argc + 1) };
    let status = unsafe { eonos_main(argc, argv, envp) };
    exit(status)
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    eprintln!("panic: {}", info);
    exit(101)
}

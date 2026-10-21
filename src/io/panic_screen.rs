use core::fmt::Write;

use crate::console::{CONSOLE, Console};

pub fn force_unlock_all() {
    unsafe {
        crate::logger::LOGGER.force_unlock();
        crate::serial::SERIAL1.force_unlock();
        CONSOLE.force_unlock();
    }
}

/// Print a bounded hexdump of memory already known to be readable by the
/// caller. Panic paths pass only their saved exception frame or current stack.
pub fn write_memory_dump(console: &mut Console, address: usize, bytes: &[u8]) {
    let length = bytes.len().min(64);
    let _ = writeln!(console, "Memory dump ({} bytes):", length);
    for offset in (0..length).step_by(16) {
        let _ = write!(console, "{:#018x}: ", address.saturating_add(offset));
        for byte in &bytes[offset..(offset + 16).min(length)] {
            let _ = write!(console, "{:02X} ", byte);
        }
        let _ = writeln!(console);
    }
}

/// Print the active thread and (when it is a user process) process name.
/// The scheduler and process table are only sampled with try-locks because
/// either lock may be held by the code that just crashed.
pub fn write_crash_context(console: &mut Console, fault_cs: Option<u64>) -> Option<(u64, u64)> {
    let Some(context) = crate::scheduler::try_crash_context() else {
        let _ = writeln!(console, "Execution context: unavailable (scheduler busy)");
        return None;
    };

    if context.user_process {
        let mut name = [0u8; 48];
        if let Some(length) = crate::process::try_copy_name(context.id, &mut name) {
            let _ = write!(console, "Process: ");
            for &byte in &name[..length] {
                let ch = if byte.is_ascii_graphic() || byte == b' ' {
                    byte as char
                } else {
                    '?'
                };
                let _ = write!(console, "{}", ch);
            }
            let _ = writeln!(console, " (PID {})", context.id);
        } else {
            let _ = writeln!(
                console,
                "Process: {} (PID {})",
                context.thread_name, context.id
            );
        }
    } else {
        let _ = writeln!(
            console,
            "Kernel thread: {} (TID {})",
            context.thread_name, context.id
        );
    }

    if let Some(cs) = fault_cs {
        let _ = writeln!(
            console,
            "Privilege: {}",
            if cs & 3 == 3 { "user" } else { "kernel" }
        );
    }

    if context.stack_low < context.stack_high {
        Some((context.stack_low, context.stack_high))
    } else {
        None
    }
}

/// Walk a short x86-64 frame-pointer chain. Every read is checked against
/// the current thread's known kernel stack bounds before it is dereferenced.
pub fn write_stack_trace(
    console: &mut Console,
    frame_pointer: u64,
    initial_ip: Option<u64>,
    bounds: Option<(u64, u64)>,
) {
    let _ = writeln!(console, "Stack trace:");
    let mut count = 0;
    if let Some(ip) = initial_ip.filter(|&ip| ip != 0) {
        let _ = writeln!(console, "  #0 {:#018x}", ip);
        count = 1;
    }

    let Some((low, high)) = bounds else {
        let _ = writeln!(console, "  (stack bounds unavailable)");
        return;
    };

    let mut current = frame_pointer;
    while count < 6 {
        let Some(end) = current.checked_add(16) else {
            break;
        };
        if current < low || end > high || current & 7 != 0 {
            break;
        }
        // SAFETY: bounds and alignment are validated against the mapped
        // stack slot owned by the currently executing thread.
        let (next, return_address) = unsafe {
            let frame = current as *const u64;
            (
                core::ptr::read_volatile(frame),
                core::ptr::read_volatile(frame.add(1)),
            )
        };
        if return_address == 0 || return_address < 0xffff_8000_0000_0000 {
            break;
        }
        let _ = writeln!(console, "  #{:<1} {:#018x}", count, return_address);
        count += 1;
        if next <= current || next >= high {
            break;
        }
        current = next;
    }
    if count == 0 {
        let _ = writeln!(console, "  (no frame pointers available)");
    } else {
        let _ = writeln!(console, "  (addresses; symbols unavailable in this build)");
    }
}

pub fn show<F>(cause: &str, error_code: u32, body: F) -> !
where
    F: FnOnce(&mut Console),
{
    crate::idt::disable_interrupts();
    force_unlock_all();

    if let Some(console) = CONSOLE.lock().as_mut() {
        // Classic full-screen panic blue, with white foreground text.
        let _ = write!(console, "\x1b[48;2;0;0;170m\x1b[97m");
        console.clear();

        let _ = writeln!(console, "EonOS Kernel v{}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(console, "*** KERNEL PANIC : {} ***", cause);
        let _ = writeln!(console, "ERROR CODE: 0x{:08X}", error_code);
        let _ = writeln!(console);

        body(console);

        let _ = writeln!(console);
        let _ = writeln!(console, "System halted. If this repeats, file an issue:");
        let _ = writeln!(console, "https://github.com/The-EonOS-Project/eonos/issues");
        let _ = write!(console, "\x1b[0m");
    }

    crate::hcf()
}

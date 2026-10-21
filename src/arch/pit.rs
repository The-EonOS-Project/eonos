#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};

use crate::interrupts;
use crate::{idt, log_fail, log_ok};

const CHANNEL0_DATA: u16 = 0x40;
const COMMAND: u16 = 0x43;

const BASE_FREQUENCY: u32 = 1_193_182;

const IRQ_LINE: u8 = 0;

static TICKS: AtomicU64 = AtomicU64::new(0);
static HZ: AtomicU64 = AtomicU64::new(0);

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

pub fn init(hz: u32) {

    let hz = hz.clamp(19, 1000);
    let divisor = (BASE_FREQUENCY / hz).clamp(1, 65535) as u16;

    unsafe {
        outb(COMMAND, 0x36); 
        outb(CHANNEL0_DATA, (divisor & 0xFF) as u8);
        outb(CHANNEL0_DATA, (divisor >> 8) as u8);
    }

    HZ.store(hz as u64, Ordering::Relaxed);
    TICKS.store(0, Ordering::Relaxed);

    idt::register_irq(IRQ_LINE, on_tick);
    interrupts::unmask_isa_irq(IRQ_LINE);

    log_ok!(
        "PIT",
        "Init",
        "Channel 0 programmed for {} Hz (divisor {}), IRQ0 registered and unmasked",
        hz,
        divisor
    );
}

fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);

    crate::scheduler::tick();
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn frequency_hz() -> u64 {
    HZ.load(Ordering::Relaxed)
}

pub fn uptime_ms() -> u64 {
    let hz = frequency_hz();
    if hz == 0 {
        return 0;
    }
    ticks() * 1000 / hz
}

pub fn self_test() {
    let hz = frequency_hz();
    if hz == 0 {
        log_fail!("PIT", "SelfTest", "Timer not initialized");
        return;
    }

    let start = ticks();
    let target_ticks = (hz / 20).max(1); 
    let deadline = start + target_ticks;

    let tsc_start = rdtsc();

    let tsc_timeout = 1u64 << 33;

    while ticks() < deadline {
        unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
        if rdtsc().wrapping_sub(tsc_start) > tsc_timeout {
            log_fail!(
                "PIT",
                "SelfTest",
                "No timer interrupts after ~{} TSC cycles -- IRQ0 is not reaching the handler \
                 (common on UEFI-only hardware that never programmed the legacy PIC; this kernel \
                 has no IOAPIC support yet, so the timer and any other legacy-IRQ device will be \
                 unusable on this machine)",
                tsc_timeout
            );
            return;
        }
    }

    log_ok!(
        "PIT",
        "SelfTest",
        "{} timer interrupts observed via IRQ0 in {}ms (uptime {}ms)",
        ticks() - start,
        (ticks() - start) * 1000 / hz,
        uptime_ms()
    );
}

fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | lo as u64
}

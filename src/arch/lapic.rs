#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::vmm;
use crate::{log_fail, log_ok};

const REG_ID: u32 = 0x020;
const REG_VERSION: u32 = 0x030;
const REG_EOI: u32 = 0x0B0;
const REG_SPURIOUS: u32 = 0x0F0;
const REG_LVT_TIMER: u32 = 0x320;
const REG_LVT_THERMAL: u32 = 0x330;
const REG_LVT_PERFORMANCE: u32 = 0x340;
const REG_LVT_LINT0: u32 = 0x350;
const REG_LVT_LINT1: u32 = 0x360;
const REG_LVT_ERROR: u32 = 0x370;

const SPURIOUS_APIC_ENABLE: u32 = 1 << 8;
const LVT_MASKED: u32 = 1 << 16;

pub const SPURIOUS_VECTOR: u8 = 0xFF;

static LAPIC_BASE: AtomicU64 = AtomicU64::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
static APIC_ID: AtomicU64 = AtomicU64::new(0);

fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {

    let eax_out: u32;
    let ebx_out: u32;
    let ecx_out: u32;
    let edx_out: u32;
    unsafe {
        core::arch::asm!(
            "mov {ebx_scratch:e}, ebx",
            "cpuid",
            "xchg {ebx_scratch:e}, ebx",
            ebx_scratch = out(reg) ebx_out,
            inout("eax") leaf => eax_out,
            out("ecx") ecx_out,
            out("edx") edx_out,
            options(nomem, nostack, preserves_flags),
        );
    }
    (eax_out, ebx_out, ecx_out, edx_out)
}

fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | lo as u64
}

fn wrmsr(msr: u32, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    unsafe {
        core::arch::asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags));
    }
}

const IA32_APIC_BASE: u32 = 0x1B;
const APIC_BASE_EXTD: u64 = 1 << 10; 
const APIC_BASE_EN: u64 = 1 << 11; 
const APIC_BASE_ADDR_MASK: u64 = 0x0000_000F_FFFF_F000;

fn reg_read(reg: u32) -> u32 {
    let base = LAPIC_BASE.load(Ordering::Relaxed);
    unsafe { core::ptr::read_volatile((base + reg as u64) as *const u32) }
}
fn reg_write(reg: u32, val: u32) {
    let base = LAPIC_BASE.load(Ordering::Relaxed);
    unsafe { core::ptr::write_volatile((base + reg as u64) as *mut u32, val) };
}

pub fn init(madt_local_apic_addr: u64) -> bool {
    let (_, _, ecx1, edx1) = cpuid(1);
    let has_apic = edx1 & (1 << 9) != 0;
    let has_x2apic = ecx1 & (1 << 21) != 0;

    if !has_apic {
        log_fail!("LAPIC", "Init", "CPUID reports no Local APIC on this CPU");
        return false;
    }

    let apic_base_msr = rdmsr(IA32_APIC_BASE);
    if apic_base_msr & APIC_BASE_EXTD != 0 {

        log_fail!(
            "LAPIC",
            "Init",
            "CPU is already in x2APIC mode, which this driver doesn't support -- falling back to the legacy PIC"
        );
        return false;
    }
    let _ = has_x2apic; 

    let msr_addr = apic_base_msr & APIC_BASE_ADDR_MASK;
    let phys_base = if msr_addr != 0 {
        msr_addr
    } else {
        madt_local_apic_addr
    };
    if phys_base == 0 {
        log_fail!(
            "LAPIC",
            "Init",
            "No usable Local APIC address from either the MSR or the MADT"
        );
        return false;
    }

    if apic_base_msr & APIC_BASE_EN == 0 {
        wrmsr(IA32_APIC_BASE, apic_base_msr | APIC_BASE_EN);
    }

    let virt_base = vmm::map_mmio(phys_base, 4096) as u64;
    LAPIC_BASE.store(virt_base, Ordering::Relaxed);

    let version = reg_read(REG_VERSION);
    let max_lvt = (version >> 16) & 0xff;
    reg_write(REG_LVT_TIMER, LVT_MASKED);
    if max_lvt >= 1 {
        reg_write(REG_LVT_THERMAL, LVT_MASKED);
    }
    if max_lvt >= 2 {
        reg_write(REG_LVT_PERFORMANCE, LVT_MASKED);
    }
    if max_lvt >= 3 {
        reg_write(REG_LVT_LINT0, LVT_MASKED);
    }
    if max_lvt >= 4 {
        reg_write(REG_LVT_LINT1, LVT_MASKED);
    }
    if max_lvt >= 5 {
        reg_write(REG_LVT_ERROR, LVT_MASKED);
    }

    reg_write(
        REG_SPURIOUS,
        (SPURIOUS_VECTOR as u32) | SPURIOUS_APIC_ENABLE,
    );

    ENABLED.store(true, Ordering::Release);

    let id = reg_read(REG_ID) >> 24;
    APIC_ID.store(id as u64, Ordering::Release);
    let version = version & 0xFF;
    log_ok!(
        "LAPIC",
        "Init",
        "Local APIC enabled at {:#x} (ID {}, version {:#x}), spurious vector {:#x}",
        phys_base,
        id,
        version,
        SPURIOUS_VECTOR
    );

    true
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

pub fn id() -> u8 {
    APIC_ID.load(Ordering::Acquire) as u8
}

pub fn end_of_interrupt() {
    reg_write(REG_EOI, 0);
}








#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct FpuState([u8; 512]);

static mut DEFAULT_STATE: FpuState = FpuState([0; 512]);

pub fn init() {
    let features = core::arch::x86_64::__cpuid(1);
    const FXSR: u32 = 1 << 24;
    const SSE: u32 = 1 << 25;
    const SSE2: u32 = 1 << 26;
    if features.edx & (FXSR | SSE | SSE2) != (FXSR | SSE | SSE2) {
        panic!("x86-64 CPU lacks required FXSAVE/SSE2 support");
    }

    const CR0_MP: u64 = 1 << 1;
    const CR0_EM: u64 = 1 << 2;
    const CR0_TS: u64 = 1 << 3;
    const CR0_NE: u64 = 1 << 5;
    const CR4_OSFXSR: u64 = 1 << 9;
    const CR4_OSXMMEXCPT: u64 = 1 << 10;
    unsafe {
        let mut cr0: u64;
        let mut cr4: u64;
        core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        cr0 = (cr0 | CR0_MP | CR0_NE) & !(CR0_EM | CR0_TS);
        cr4 |= CR4_OSFXSR | CR4_OSXMMEXCPT;
        core::arch::asm!("mov cr0, {}", in(reg) cr0, options(nostack, preserves_flags));
        core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));
        core::arch::asm!("fninit", options(nostack, preserves_flags));
        let mxcsr = 0x1f80u32;
        core::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr, options(nostack, preserves_flags));
        let mut state = FpuState([0; 512]);
        core::arch::asm!("fxsave64 [{}]", in(reg) state.0.as_mut_ptr(), options(nostack));
        DEFAULT_STATE = state;
    }
    crate::log_ok!("CPU", "FPU", "FXSAVE/SSE2 enabled with eager per-thread state");
}

pub fn default_state() -> FpuState {
    
    unsafe { DEFAULT_STATE }
}

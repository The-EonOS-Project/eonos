//! System-wide reset and power-off controls.

use crate::acpi;

/// Request ACPI soft-off. Returns false if firmware does not expose a
/// supported S5 control interface.
pub fn shutdown() -> bool {
    acpi::shutdown()
}

/// Stop executing after a successful ACPI power-off request. Normally the
/// platform powers down before this loop runs.
pub fn halt() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) };
    }
}

/// Restart the machine, trying the firmware reset register before legacy
/// chipset and keyboard-controller reset mechanisms.
pub fn reboot() -> ! {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    let _ = acpi::reset();

    unsafe {
        // Common PCI chipset reset sequence.
        core::arch::asm!("out dx, al", in("dx") 0x0cf9u16, in("al") 0x02u8, options(nomem, nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") 0x0cf9u16, in("al") 0x06u8, options(nomem, nostack, preserves_flags));

        // 8042 reset is widely supported, including on older PCs.
        core::arch::asm!("out dx, al", in("dx") 0x0064u16, in("al") 0xfeu8, options(nomem, nostack, preserves_flags));
    }

    // Last resort: an invalid IDT followed by an exception causes a
    // triple fault, which x86 platforms implement as a processor reset.
    #[repr(C, packed)]
    struct Idtr {
        limit: u16,
        base: u64,
    }
    let empty_idt = Idtr { limit: 0, base: 0 };
    unsafe {
        core::arch::asm!(
            "lidt [{}]",
            "int3",
            in(reg) &empty_idt,
            options(noreturn, nostack),
        );
    }
}


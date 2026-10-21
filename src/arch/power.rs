use crate::acpi;

pub fn shutdown() -> bool {
    acpi::shutdown()
}

pub fn halt() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) };
    }
}

pub fn reboot() -> ! {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    let _ = acpi::reset();

    unsafe {

        core::arch::asm!("out dx, al", in("dx") 0x0cf9u16, in("al") 0x02u8, options(nomem, nostack, preserves_flags));
        core::arch::asm!("out dx, al", in("dx") 0x0cf9u16, in("al") 0x06u8, options(nomem, nostack, preserves_flags));

        core::arch::asm!("out dx, al", in("dx") 0x0064u16, in("al") 0xfeu8, options(nomem, nostack, preserves_flags));
    }

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


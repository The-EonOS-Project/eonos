#![no_std]
#![no_main]

extern crate alloc;

// Keep crate module names stable while grouping subsystem files by directory.
#[path = "arch/acpi.rs"]
pub mod acpi;
#[path = "io/block.rs"]
pub mod block;
#[path = "task/blocking.rs"]
pub mod blocking;
#[path = "io/console.rs"]
pub mod console;
pub mod drivers;
#[path = "task/elf.rs"]
pub mod elf;
#[path = "filesystem/fat32.rs"]
pub mod fat32;
#[path = "io/framebuffer.rs"]
pub mod framebuffer;
#[path = "filesystem/fs_probe.rs"]
pub mod fs_probe;
#[path = "arch/gdt.rs"]
pub mod gdt;
#[path = "arch/fpu.rs"]
pub mod fpu;
#[path = "memory/heap.rs"]
pub mod heap;
#[path = "arch/idt.rs"]
pub mod idt;
#[path = "filesystem/initramfs.rs"]
pub mod initramfs;
#[path = "input.rs"]
pub mod input;
#[path = "arch/interrupts.rs"]
pub mod interrupts;
#[path = "arch/ioapic.rs"]
pub mod ioapic;
#[path = "arch/lapic.rs"]
pub mod lapic;
#[path = "io/logger.rs"]
pub mod logger;
pub mod network;
#[path = "io/panic_screen.rs"]
pub mod panic_screen;
#[path = "filesystem/partition.rs"]
pub mod partition;
#[path = "arch/pic.rs"]
pub mod pic;
#[path = "arch/power.rs"]
pub mod power;
#[path = "arch/pit.rs"]
pub mod pit;
#[path = "memory/pmm.rs"]
pub mod pmm;
#[path = "task/process.rs"]
pub mod process;
#[path = "task/scheduler.rs"]
pub mod scheduler;
#[path = "io/serial.rs"]
pub mod serial;
#[path = "task/sync.rs"]
pub mod sync;
#[path = "task/syscall.rs"]
pub mod syscall;
#[path = "filesystem/vfs.rs"]
pub mod vfs;
#[path = "memory/vmm.rs"]
pub mod vmm;

use core::fmt::Write;
use core::panic::PanicInfo;
use limine::BaseRevision;

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests_start_marker"]
static _START_MARKER: limine::request::RequestsStartMarker =
    limine::request::RequestsStartMarker::new();

#[used]
#[link_section = ".requests_end_marker"]
static _END_MARKER: limine::request::RequestsEndMarker = limine::request::RequestsEndMarker::new();

#[no_mangle]
unsafe extern "C" fn _start() -> ! {
    serial::SERIAL1.lock().init();

    assert!(BASE_REVISION.is_supported());

    log_ok!("Kernel", "Init", "EonOS booting via Limine");

    framebuffer::init();
    framebuffer::self_test();
    framebuffer::bench("pre-VMM, bootloader mapping");

    gdt::init();
    idt::init();
    fpu::init();

    core::arch::asm!("int3");
    log_ok!(
        "IDT",
        "SelfTest",
        "Breakpoint exception handled and returned"
    );

    // Kernel Exceptions!
    // core::arch::asm!("ud2");                                      // #UD invalid opcode
    // core::ptr::read_volatile(0xffff_8000_dead_0000 as *const u8); // #PF page fault

    pmm::init();
    pmm::self_test();

    vmm::init();
    vmm::self_test();
    framebuffer::bench("post-VMM, write-combining");

    heap::init();
    drivers::virtio_gpu::init();
    // Console shadow storage uses the kernel heap. Bring the console up
    // after VMM/heap so rendering and scrollback never read from WC memory.
    console::init();
    heap::self_test();

    pic::init();
    let mut apic_ready = false;
    if let Some(madt) = acpi::find_madt() {
        if ioapic::init(&madt)
            && ioapic::supports_isa_irq(0)
            && ioapic::supports_isa_irq(1)
            && lapic::init(madt.local_apic_addr)
        {
            interrupts::use_apic();
            apic_ready = true;
            log_ok!("APIC", "Init", "I/O APIC interrupt routing enabled");
        }
    }
    if !apic_ready {
        log_ok!(
            "IRQ",
            "Init",
            "APIC setup unavailable; continuing with legacy PIC routing"
        );
    }
    pit::init(1000); // 1kHz tick, i.e. 1ms resolution
    idt::enable_interrupts();
    pit::self_test();

    // Keyboard drivers need the IDT and interrupt controller (for IRQ1)
    // already up, but come before the scheduler so a typed
    // command doesn't have to wait on anything else finishing init.
    drivers::ps2::init();
    drivers::usb::init();
    drivers::ahci::init();
    drivers::nvme::init();
    drivers::e1000::init();

    scheduler::init();
    scheduler::self_test();

    blocking::self_test();

    syscall::init();
    syscall::self_test();

    vmm::address_space_self_test();

    // The initramfs is a Limine module reached through the direct map, so
    // this has to come after the VMM is up.
    initramfs::init();
    initramfs::self_test();
    vfs::init();

    process::self_test();
    if scheduler::spawn("usb-service", drivers::usb::xhci::service).is_none() {
        log_fail!("USB", "Service", "Could not start USB polling worker");
    }
    process::start_init();

    log_ok!(
        "Kernel",
        "Init",
        "Initialization complete, handing over to the scheduler"
    );

    // The boot thread is done; the idle thread takes over from here.
    scheduler::exit();
}

fn hcf() -> ! {
    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log_critical!("Kernel", "Panic", "{}", info);
    panic_screen::show("KERNEL PANIC", (90, 0, 110), |w| {
        let _ = writeln!(w, "{}", info);
        let _ = writeln!(w);
        let _ = writeln!(w, "Full details are on the serial log.");
    });
}

#![allow(dead_code)]

use crate::log_ok;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_ICW4: u8 = 0x01; 
const ICW1_INIT: u8 = 0x10; 
const ICW4_8086: u8 = 0x01; 

const PIC_EOI: u8 = 0x20;

pub const IRQ_BASE: u8 = 32;

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    val
}

unsafe fn io_wait() {
    outb(0x80, 0);
}

pub fn init() {
    unsafe {

        outb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait();
        outb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait();

        outb(PIC1_DATA, IRQ_BASE);
        io_wait();
        outb(PIC2_DATA, IRQ_BASE + 8);
        io_wait();

        outb(PIC1_DATA, 0b0000_0100);
        io_wait();
        outb(PIC2_DATA, 0b0000_0010);
        io_wait();

        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        outb(PIC1_DATA, 0xFF);
        outb(PIC2_DATA, 0xFF);
    }

    log_ok!(
        "PIC",
        "Init",
        "8259 remapped: IRQ0-7 -> vectors {}-{}, IRQ8-15 -> {}-{} (all masked)",
        IRQ_BASE,
        IRQ_BASE + 7,
        IRQ_BASE + 8,
        IRQ_BASE + 15
    );
}

pub fn unmask(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA) & !(1 << irq);
            outb(PIC1_DATA, mask);
        } else {
            let mask = inb(PIC2_DATA) & !(1 << (irq - 8));
            outb(PIC2_DATA, mask);

            let master_mask = inb(PIC1_DATA) & !(1 << 2);
            outb(PIC1_DATA, master_mask);
        }
    }
}

pub fn mask(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA) | (1 << irq);
            outb(PIC1_DATA, mask);
        } else {
            let mask = inb(PIC2_DATA) | (1 << (irq - 8));
            outb(PIC2_DATA, mask);
        }
    }
}

pub fn end_of_interrupt(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        outb(PIC1_COMMAND, PIC_EOI);
    }
}

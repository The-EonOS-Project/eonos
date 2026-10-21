#![allow(dead_code)]

use alloc::vec::Vec;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

const CLASS_SERIAL_BUS: u8 = 0x0C;
const SUBCLASS_USB: u8 = 0x03;

const PROG_IF_UHCI: u8 = 0x00;
const PROG_IF_OHCI: u8 = 0x10;
const PROG_IF_EHCI: u8 = 0x20;
const PROG_IF_XHCI: u8 = 0x30;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ControllerKind {
    Uhci,
    Ohci,
    Ehci,
    Xhci,
}

impl ControllerKind {
    pub const fn name(self) -> &'static str {
        match self {
            ControllerKind::Uhci => "UHCI",
            ControllerKind::Ohci => "OHCI",
            ControllerKind::Ehci => "EHCI",
            ControllerKind::Xhci => "xHCI",
        }
    }

    fn from_prog_if(prog_if: u8) -> Option<ControllerKind> {
        match prog_if {
            PROG_IF_UHCI => Some(ControllerKind::Uhci),
            PROG_IF_OHCI => Some(ControllerKind::Ohci),
            PROG_IF_EHCI => Some(ControllerKind::Ehci),
            PROG_IF_XHCI => Some(ControllerKind::Xhci),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct PciLocation {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

pub struct UsbController {
    pub location: PciLocation,
    pub kind: ControllerKind,

    pub mmio_base: u64,
}

unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
}

unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!("in eax, dx", in("dx") port, out("eax") val, options(nomem, nostack, preserves_flags));
    val
}

unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags));
}

fn config_address(loc: PciLocation, offset: u8) -> u32 {
    (1 << 31)
        | ((loc.bus as u32) << 16)
        | ((loc.device as u32) << 11)
        | ((loc.function as u32) << 8)
        | (offset as u32 & 0xFC)
}

fn read_config32(loc: PciLocation, offset: u8) -> u32 {
    unsafe {
        outl(CONFIG_ADDRESS, config_address(loc, offset));
        inl(CONFIG_DATA)
    }
}

fn write_config16(loc: PciLocation, offset: u8, value: u16) {
    unsafe {
        outl(CONFIG_ADDRESS, config_address(loc, offset));
        outw(CONFIG_DATA, value);
    }
}

pub fn enable_mmio_bus_master(loc: PciLocation) -> bool {
    let command = (read_config32(loc, 0x04) as u16) | (1 << 1) | (1 << 2);

    write_config16(loc, 0x04, command);
    (read_config32(loc, 0x04) as u16 & 0x6) == 0x6
}

pub fn find_usb_controllers() -> Vec<UsbController> {
    let mut found = Vec::new();

    for bus in 0u16..256 {
        for device in 0u8..32 {

            let loc0 = PciLocation {
                bus: bus as u8,
                device,
                function: 0,
            };
            let id = read_config32(loc0, 0x00);
            let vendor_id = (id & 0xFFFF) as u16;
            if vendor_id == 0xFFFF {
                continue; 
            }

            let header_type = (read_config32(loc0, 0x0C) >> 16) as u8 & 0xFF;
            let multifunction = header_type & 0x80 != 0;
            let function_count = if multifunction { 8 } else { 1 };

            for function in 0..function_count {
                let loc = PciLocation {
                    bus: bus as u8,
                    device,
                    function,
                };
                let id = read_config32(loc, 0x00);
                if (id & 0xFFFF) as u16 == 0xFFFF {
                    continue;
                }

                let class_reg = read_config32(loc, 0x08);
                let class = (class_reg >> 24) as u8;
                let subclass = (class_reg >> 16) as u8;
                let prog_if = (class_reg >> 8) as u8;

                if class != CLASS_SERIAL_BUS || subclass != SUBCLASS_USB {
                    continue;
                }
                let Some(kind) = ControllerKind::from_prog_if(prog_if) else {
                    continue; 
                };

                let bar0 = read_config32(loc, 0x10);
                let mmio_base = if bar0 & 0x1 == 0 {

                    let is_64bit = (bar0 >> 1) & 0x3 == 2;
                    let base_low = (bar0 & !0xF) as u64;
                    if is_64bit {
                        let bar1 = read_config32(loc, 0x14) as u64;
                        base_low | (bar1 << 32)
                    } else {
                        base_low
                    }
                } else {
                    0 
                };

                found.push(UsbController {
                    location: loc,
                    kind,
                    mmio_base,
                });
            }
        }
    }

    found
}

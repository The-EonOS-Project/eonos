#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sync::IrqMutex;
use crate::{log_debug, log_fail, log_ok};
use crate::{pmm, vmm};

use super::pci::UsbController;

#[repr(C)]
struct CapRegs {
    cap_length: u8,
    _reserved: u8,
    hci_version: u16,
    hcs_params1: u32,
    hcs_params2: u32,
    hcs_params3: u32,
    hcc_params1: u32,
    dboff: u32,
    rtsoff: u32,
    hcc_params2: u32,
}

#[repr(C)]
struct OpRegs {
    usbcmd: u32,
    usbsts: u32,
    pagesize: u32,
    _rsvd1: [u32; 2],
    dnctrl: u32,
    crcr_lo: u32,
    crcr_hi: u32,
    _rsvd2: [u32; 4],
    dcbaap_lo: u32,
    dcbaap_hi: u32,
    config: u32,

}

const USBCMD_RUN: u32 = 1 << 0;
const USBCMD_HCRST: u32 = 1 << 1;

const USBSTS_HCH: u32 = 1 << 0; 
const USBSTS_CNR: u32 = 1 << 11; 

#[repr(C)]
struct PortRegs {
    portsc: u32,
    portpmsc: u32,
    portli: u32,
    porthlpmc: u32,
}

const PORTSC_CCS: u32 = 1 << 0; 
const PORTSC_PED: u32 = 1 << 1; 
const PORTSC_PR: u32 = 1 << 4; 
const PORTSC_WPR: u32 = 1 << 31; 
const PORTSC_SPEED_MASK: u32 = 0xF << 10;
const PORTSC_WRC: u32 = 1 << 19; 
const PORTSC_PRC: u32 = 1 << 21; 
const PORTSC_CSC: u32 = 1 << 17; 

const PORTSC_WRITE_ONE_MASK: u32 =
    PORTSC_PED | (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 21) | (1 << 22) | (1 << 23);

#[repr(C)]
struct RuntimeRegs {
    mfindex: u32,
    _rsvd: [u32; 7],
    ir0_iman: u32,
    ir0_imod: u32,
    ir0_erstsz: u32,
    _rsvd2: u32,
    ir0_erstba_lo: u32,
    ir0_erstba_hi: u32,
    ir0_erdp_lo: u32,
    ir0_erdp_hi: u32,
}

const ERDP_BUSY: u32 = 1 << 3; 

#[repr(C)]
#[derive(Clone, Copy)]
struct Trb {
    parameter: u64,
    status: u32,
    control: u32,
}

const TRB_CYCLE: u32 = 1 << 0;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_TOGGLE_CYCLE: u32 = 1 << 1; 
const TRB_IOC: u32 = 1 << 5; 

fn trb_type(control: u32) -> u32 {
    (control >> 10) & 0x3F
}
fn make_type(t: u32) -> u32 {
    (t & 0x3F) << 10
}

const TRB_TYPE_NORMAL: u32 = 1;

const KEYBOARD_REPORT_QUEUE_DEPTH: usize = 8;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_DISABLE_SLOT: u32 = 10;
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
const TRB_TYPE_EVALUATE_CONTEXT: u32 = 13;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TYPE_COMMAND_COMPLETION: u32 = 33;

const COMPLETION_SUCCESS: u32 = 1;
const COMPLETION_SHORT_PACKET: u32 = 13;
const TRB_ISP: u32 = 1 << 2;

const RING_SIZE: usize = 16;

struct Ring {
    trbs: *mut Trb,
    phys: u64,
    enqueue: usize,

    cycle: u32,
}

impl Ring {
    fn new() -> Option<Ring> {
        Self::allocate(true)
    }

    fn new_event() -> Option<Ring> {
        Self::allocate(false)
    }

    fn allocate(with_link: bool) -> Option<Ring> {
        let phys = pmm::alloc_frame_zeroed()?;
        let trbs = pmm::phys_to_virt(phys) as *mut Trb;
        if with_link {

            unsafe {
                let link = trbs.add(RING_SIZE - 1);
                core::ptr::write_volatile(
                    link,
                    Trb {
                        parameter: phys,
                        status: 0,
                        control: make_type(TRB_TYPE_LINK) | TRB_TOGGLE_CYCLE | TRB_CYCLE,
                    },
                );
            }
        }
        Some(Ring {
            trbs,
            phys,
            enqueue: 0,
            cycle: 1,
        })
    }

    unsafe fn push(&mut self, mut trb: Trb) -> u64 {
        trb.control = (trb.control & !TRB_CYCLE) | self.cycle;
        let slot_phys = self.phys + (self.enqueue as u64) * 16;
        core::ptr::write_volatile(self.trbs.add(self.enqueue), trb);
        self.enqueue += 1;
        if self.enqueue == RING_SIZE - 1 {

            let link = self.trbs.add(RING_SIZE - 1);
            let mut existing = core::ptr::read_volatile(link);
            existing.control = (existing.control & !TRB_CYCLE) | self.cycle;
            core::ptr::write_volatile(link, existing);
            self.enqueue = 0;
            self.cycle ^= 1;
        }
        slot_phys
    }
}

fn input_context_entry(ctx_phys: u64, context_size: usize, index: usize) -> *mut u32 {
    unsafe { pmm::phys_to_virt(ctx_phys).add(index * context_size) as *mut u32 }
}

fn setup_packet(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> u64 {
    (request_type as u64)
        | ((request as u64) << 8)
        | ((value as u64) << 16)
        | ((index as u64) << 32)
        | ((length as u64) << 48)
}

const REQ_TYPE_STD_DEVICE_IN: u8 = 0x80;
const REQ_TYPE_STD_DEVICE_OUT: u8 = 0x00;
const REQ_TYPE_CLASS_IFACE_OUT: u8 = 0x21;

const REQ_GET_DESCRIPTOR: u8 = 6;
const REQ_SET_CONFIGURATION: u8 = 9;
const HID_REQ_SET_PROTOCOL: u8 = 0x0B;
const HID_REQ_SET_IDLE: u8 = 0x0A;
const MSC_BULK_ONLY_RESET: u8 = 0xFF;

const DESC_TYPE_DEVICE: u16 = 1 << 8;
const DESC_TYPE_CONFIGURATION: u16 = 2 << 8;
const HID_PROTOCOL_BOOT: u16 = 0;
const HID_PROTOCOL_KEYBOARD: u8 = 1;
const HID_PROTOCOL_MOUSE: u8 = 2;
const MSC_CLASS: u8 = 8;
const MSC_SUBCLASS_SCSI: u8 = 6;
const MSC_PROTOCOL_BULK_ONLY: u8 = 0x50;
const MSC_CBW_SIGNATURE: u32 = 0x4342_5355;
const MSC_CSW_SIGNATURE: u32 = 0x5342_5355;

#[derive(Clone, Copy)]
struct BootHidEndpoint {
    configuration: u8,
    interface: u8,
    dci: u8,
    max_packet: u16,
    interval: u8,
    protocol: u8,
    report_len: u8,
}

#[derive(Clone, Copy)]
struct MassStorageEndpoints {
    configuration: u8,
    interface: u8,
    bulk_in_dci: u8,
    bulk_out_dci: u8,
    bulk_in_packet: u16,
    bulk_out_packet: u16,
}

struct UsbMassStorage {
    id: u64,
    port: u8,
    slot_id: u8,
    interface: u8,
    bulk_in_dci: u8,
    bulk_out_dci: u8,
    bulk_in_packet: u16,
    bulk_out_packet: u16,
    bulk_in_ring: Ring,
    bulk_out_ring: Ring,
    output_ctx_phys: u64,
    ep0_ring_phys: u64,
    buffer_phys: u64,
    csw_phys: u64,
    capacity: u64,
    next_tag: u32,
    registered: bool,
}

static NEXT_STORAGE_ID: AtomicU64 = AtomicU64::new(1);

fn find_boot_hid(config: &[u8], speed: u32) -> alloc::vec::Vec<BootHidEndpoint> {
    let mut endpoints = alloc::vec::Vec::new();
    if config.len() < 9 || config[1] != 2 {
        return endpoints;
    }
    let total = u16::from_le_bytes([config[2], config[3]]) as usize;
    if total < 9 || total > config.len() {
        return endpoints;
    }
    let configuration = config[5];
    let mut hid_iface = None;
    let mut interface_count = 0usize;
    let mut boot_hid_count = 0usize;
    let mut interrupt_in_count = 0usize;
    let mut offset = 9usize;
    while offset + 2 <= total {
        let len = config[offset] as usize;
        let kind = config[offset + 1];
        if len < 2 || offset + len > total {
            return alloc::vec::Vec::new();
        }
        match kind {
            4 if len >= 9 => {
                interface_count += 1;

                let protocol = config[offset + 7];
                hid_iface = if config[offset + 3] == 0
                    && config[offset + 5] == 3
                    && config[offset + 6] == 1
                    && matches!(protocol, HID_PROTOCOL_KEYBOARD | HID_PROTOCOL_MOUSE)
                {
                    boot_hid_count += 1;
                    Some((config[offset + 2], protocol))
                } else {
                    None
                };
            }
            5 if len >= 7 && hid_iface.is_some() => {
                let address = config[offset + 2];
                let attributes = config[offset + 3];
                let packet = u16::from_le_bytes([config[offset + 4], config[offset + 5]]) & 0x07ff;
                let (interface, protocol) = hid_iface.unwrap();
                let min_packet = if protocol == HID_PROTOCOL_KEYBOARD {
                    8
                } else {
                    3
                };

                let report_len = if protocol == HID_PROTOCOL_KEYBOARD {
                    8
                } else {

                    packet.min(64) as u8
                };
                if address & 0x80 != 0 && attributes & 0x03 == 0x03 {
                    interrupt_in_count += 1;
                }
                if address & 0x80 != 0
                    && address & 0x0f != 0
                    && attributes & 0x03 == 0x03
                    && packet >= min_packet as u16

                    && (speed != 2 || packet <= 8)
                {
                    let endpoint = address & 0x0f;
                    endpoints.push(BootHidEndpoint {
                        configuration,
                        interface,
                        dci: endpoint * 2 + 1,
                        max_packet: packet,
                        interval: config[offset + 6].max(1),
                        protocol,
                        report_len,
                    });
                }
            }
            _ => {}
        }
        offset += len;
    }
    if endpoints.is_empty() {
        log_fail!(
            "USB",
            "xHCI",
            "No boot keyboard/mouse endpoint in configuration: len={}, total={}, speed={}, interfaces={}, boot HID interfaces={}, interrupt-IN endpoints={}",
            config.len(),
            total,
            speed,
            interface_count,
            boot_hid_count,
            interrupt_in_count
        );
    }
    endpoints
}

fn find_mass_storage(config: &[u8]) -> Option<MassStorageEndpoints> {
    if config.len() < 9 || config[1] != 2 { return None; }
    let total = u16::from_le_bytes([config[2], config[3]]) as usize;
    if total < 9 || total > config.len() { return None; }
    let configuration = config[5];
    let mut interface = None;
    let mut bulk_in = None;
    let mut bulk_out = None;
    let mut offset = 9usize;
    while offset + 2 <= total {
        let len = config[offset] as usize;
        let kind = config[offset + 1];
        if len < 2 || offset + len > total { return None; }
        match kind {
            4 if len >= 9 => {
                interface = if config[offset + 3] == 0
                    && config[offset + 5] == MSC_CLASS
                    && config[offset + 6] == MSC_SUBCLASS_SCSI
                    && config[offset + 7] == MSC_PROTOCOL_BULK_ONLY
                {
                    bulk_in = None;
                    bulk_out = None;
                    Some(config[offset + 2])
                } else {
                    None
                };
            }
            5 if len >= 7 && interface.is_some() => {
                let address = config[offset + 2];
                let attributes = config[offset + 3] & 0x03;
                let packet = u16::from_le_bytes([config[offset + 4], config[offset + 5]]) & 0x07ff;
                let endpoint = address & 0x0f;
                if attributes == 2 && endpoint != 0 && packet != 0 {
                    if address & 0x80 != 0 {
                        bulk_in = Some((endpoint * 2 + 1, packet));
                    } else {
                        bulk_out = Some((endpoint * 2, packet));
                    }
                }
                if let (Some((bulk_in_dci, bulk_in_packet)), Some((bulk_out_dci, bulk_out_packet))) = (bulk_in, bulk_out) {
                    return Some(MassStorageEndpoints {
                        configuration,
                        interface: interface.unwrap(),
                        bulk_in_dci,
                        bulk_out_dci,
                        bulk_in_packet,
                        bulk_out_packet,
                    });
                }
            }
            _ => {}
        }
        offset += len;
    }
    None
}

fn xhci_interval(speed: u32, descriptor_interval: u8) -> Option<u32> {
    let interval = descriptor_interval.max(1) as u32;
    match speed {

        1 | 2 => {
            let microframes = interval.checked_mul(8)?;
            Some(31 - microframes.leading_zeros())
        }

        3 | 4 if interval <= 16 => Some(interval - 1),
        _ => None,
    }
}

struct Controller {
    cap: *const CapRegs,
    op: *mut OpRegs,
    rt: *mut RuntimeRegs,
    db: *mut u32, 
    max_ports: u8,
    context_size: usize,

    cmd_ring: Ring,
    event_ring: Ring,

    event_cycle: u32,
    event_dequeue: usize,

    dcbaa_phys: u64,

    input_ctx_phys: u64,

    output_ctx_phys: u64,
    ep0_ring_phys: u64,

    slot_id: u8,
    devices: alloc::vec::Vec<UsbHidDevice>,
    storage_devices: alloc::vec::Vec<UsbMassStorage>,
    scanned_ports: alloc::vec::Vec<u8>,
    last_port_scan_ms: u64,
}

struct UsbHidDevice {
    port: u8,
    slot_id: u8,
    dci: u8,
    protocol: u8,
    report_len: u8,
    ring: Ring,
    output_ctx_phys: u64,
    ep0_ring_phys: u64,
    report_phys: [u64; KEYBOARD_REPORT_QUEUE_DEPTH],
    report_enqueue: usize,
    report_dequeue: usize,
    prev_keys: [u8; 6],
    prev_modifiers: u8,
    mouse_report_logged: bool,
}

unsafe impl Send for Controller {}

static CONTROLLERS: IrqMutex<alloc::vec::Vec<Controller>> = IrqMutex::new(alloc::vec::Vec::new());
static POLLING: AtomicBool = AtomicBool::new(false);

static CAPS_LOCK: AtomicBool = AtomicBool::new(false);

fn reg_read32(ptr: *const u32) -> u32 {
    unsafe { core::ptr::read_volatile(ptr) }
}
fn reg_write32(ptr: *mut u32, val: u32) {
    unsafe { core::ptr::write_volatile(ptr, val) }
}
fn reg_write64_split(lo: *mut u32, hi: *mut u32, val: u64) {
    reg_write32(lo, val as u32);
    reg_write32(hi, (val >> 32) as u32);
}

fn spin_until(timeout_ms: u64, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = crate::pit::uptime_ms().saturating_add(timeout_ms);
    loop {
        if cond() {
            return true;
        }
        if crate::pit::uptime_ms() >= deadline {
            return false;
        }
        crate::scheduler::yield_now();
    }
}

const XECP_ID_USB_LEGACY: u32 = 1;
const XECP_ID_SUPPORTED_PROTOCOL: u32 = 2;
const USBLEGSUP_BIOS_OWNED: u32 = 1 << 16;
const USBLEGSUP_OS_OWNED: u32 = 1 << 24;

const USBLEGCTLSTS_DISABLE_SMI: u32 = (0x7 << 1) | (0xFF << 5) | (0x7 << 17);
const USBLEGCTLSTS_SMI_EVENTS: u32 = 0x7 << 29;

fn request_bios_handoff(base: *mut u8, xecp_offset_dwords: usize) {
    let mut offset = xecp_offset_dwords;

    for _ in 0..256 {
        if offset == 0 {
            return; 
        }
        let cap_ptr = unsafe { (base as *mut u32).add(offset) };
        let cap_dword = reg_read32(cap_ptr);
        let cap_id = cap_dword & 0xFF;
        let next = (cap_dword >> 8) & 0xFF;

        if cap_id == XECP_ID_USB_LEGACY {
            if cap_dword & USBLEGSUP_BIOS_OWNED != 0 {
                log_ok!("USB", "xHCI", "Requesting BIOS-to-OS handoff");
                reg_write32(cap_ptr, cap_dword | USBLEGSUP_OS_OWNED);
                let handed_over =
                    spin_until(5_000, || reg_read32(cap_ptr) & USBLEGSUP_BIOS_OWNED == 0);
                if !handed_over {
                    log_fail!("USB", "xHCI", "BIOS did not release ownership in time");
                } else {
                    log_ok!("USB", "xHCI", "BIOS released ownership");
                }
            }
            let ctl_ptr = unsafe { cap_ptr.add(1) };
            let ctl = reg_read32(ctl_ptr);
            reg_write32(
                ctl_ptr,
                (ctl & USBLEGCTLSTS_DISABLE_SMI) | USBLEGCTLSTS_SMI_EVENTS,
            );
            log_ok!("USB", "xHCI", "Disabled legacy USB SMI sources");
            return;
        }

        if next == 0 {
            return;
        }
        offset += next as usize;
    }
}

fn port_uses_usb3(c: &Controller, port: u8) -> bool {
    let hcc_params1 = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*c.cap).hcc_params1)) };
    let mut offset = ((hcc_params1 >> 16) & 0xffff) as usize;
    for _ in 0..256 {
        if offset == 0 {
            break;
        }
        let cap_ptr = unsafe { (c.cap as *const u8 as *const u32).add(offset) };
        let header = reg_read32(cap_ptr);
        let cap_id = header & 0xff;
        let next = ((header >> 8) & 0xff) as usize;
        if cap_id == XECP_ID_SUPPORTED_PROTOCOL {
            let name = reg_read32(unsafe { cap_ptr.add(1) });
            let ports = reg_read32(unsafe { cap_ptr.add(2) });
            let first = (ports & 0xff) as u8;
            let count = ((ports >> 8) & 0xff) as u8;
            if name == u32::from_le_bytes(*b"USB3")
                && count != 0
                && port >= first
                && (port as u16) < first as u16 + count as u16
            {
                return true;
            }
        }
        if next == 0 {
            break;
        }
        offset += next;
    }
    false
}

pub fn probe(controller: &UsbController) {
    if controller.mmio_base == 0 {
        log_fail!(
            "USB",
            "xHCI",
            "Controller has no usable memory BAR -- cannot probe"
        );
        return;
    }

    if !super::pci::enable_mmio_bus_master(controller.location) {
        log_fail!(
            "USB",
            "xHCI",
            "Could not enable PCI memory decoding and bus mastering"
        );
        return;
    }

    let base = vmm::map_mmio(controller.mmio_base, 0x10000);
    let cap = base as *const CapRegs;

    let hci_version = reg_read32(base as *const u32) >> 16;
    let hcs_params1 = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*cap).hcs_params1)) };
    let hcc_params1 = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*cap).hcc_params1)) };
    let dboff = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*cap).dboff)) } & !0x3;
    let rtsoff = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*cap).rtsoff)) } & !0x1F;
    let cap_length = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*cap).cap_length)) };

    let max_slots = (hcs_params1 & 0xFF) as u8;
    let max_ports = ((hcs_params1 >> 24) & 0xFF) as u8;
    let context_size_64 = hcc_params1 & (1 << 2) != 0; 

    log_ok!(
        "USB",
        "xHCI",
        "Controller v{}.{}, {} slot(s), {} port(s), {}-byte contexts",
        (hci_version >> 8) & 0xFF,
        hci_version & 0xFF,
        max_slots,
        max_ports,
        if context_size_64 { 64 } else { 32 }
    );

    if max_slots == 0 || max_ports == 0 {
        log_fail!("USB", "xHCI", "Controller reports no usable slots or ports");
        return;
    }

    let op = unsafe { base.add(cap_length as usize) } as *mut OpRegs;
    let rt = unsafe { base.add(rtsoff as usize) } as *mut RuntimeRegs;
    let db = unsafe { base.add(dboff as usize) } as *mut u32;

    let xecp_dwords = (hcc_params1 >> 16) & 0xFFFF;
    if xecp_dwords != 0 {
        request_bios_handoff(base, xecp_dwords as usize);
    }

    if !reset_controller(op) {
        log_fail!(
            "USB",
            "xHCI",
            "Controller did not come out of reset in time"
        );
        return;
    }

    let Some(cmd_ring) = Ring::new() else {
        log_fail!("USB", "xHCI", "Out of memory allocating the command ring");
        return;
    };
    let Some(event_ring) = Ring::new_event() else {
        log_fail!("USB", "xHCI", "Out of memory allocating the event ring");
        return;
    };
    let Some(dcbaa_phys) = pmm::alloc_frame_zeroed() else {
        log_fail!("USB", "xHCI", "Out of memory allocating the DCBAA");
        return;
    };
    let Some(input_ctx_phys) = pmm::alloc_frame_zeroed() else {
        log_fail!("USB", "xHCI", "Out of memory allocating the input context");
        return;
    };
    let Some(erst_phys) = pmm::alloc_frame_zeroed() else {
        log_fail!(
            "USB",
            "xHCI",
            "Out of memory allocating the event ring segment table"
        );
        return;
    };

    unsafe {
        let erst = pmm::phys_to_virt(erst_phys) as *mut u64;
        core::ptr::write_volatile(erst, event_ring.phys);
        core::ptr::write_volatile((erst as *mut u32).add(2), RING_SIZE as u32);
        core::ptr::write_volatile((erst as *mut u32).add(3), 0);
    }

    reg_write64_split(
        unsafe { core::ptr::addr_of_mut!((*op).dcbaap_lo) },
        unsafe { core::ptr::addr_of_mut!((*op).dcbaap_hi) },
        dcbaa_phys,
    );
    reg_write64_split(
        unsafe { core::ptr::addr_of_mut!((*op).crcr_lo) },
        unsafe { core::ptr::addr_of_mut!((*op).crcr_hi) },
        cmd_ring.phys | 1,
    );
    reg_write32(unsafe { core::ptr::addr_of_mut!((*rt).ir0_erstsz) }, 1);
    reg_write64_split(
        unsafe { core::ptr::addr_of_mut!((*rt).ir0_erstba_lo) },
        unsafe { core::ptr::addr_of_mut!((*rt).ir0_erstba_hi) },
        erst_phys,
    );
    reg_write64_split(
        unsafe { core::ptr::addr_of_mut!((*rt).ir0_erdp_lo) },
        unsafe { core::ptr::addr_of_mut!((*rt).ir0_erdp_hi) },
        event_ring.phys,
    );

    reg_write32(
        unsafe { core::ptr::addr_of_mut!((*op).config) },
        max_slots as u32,
    );

    reg_write32(unsafe { core::ptr::addr_of_mut!((*op).usbcmd) }, USBCMD_RUN);
    if !spin_until(1_000, || {
        reg_read32(unsafe { core::ptr::addr_of!((*op).usbsts) }) & USBSTS_HCH == 0
    }) {
        log_fail!(
            "USB",
            "xHCI",
            "Controller did not leave Halted state after Run"
        );
        return;
    }

    let mut controller = Controller {
        cap,
        op,
        rt,
        db,
        max_ports,
        context_size: if context_size_64 { 64 } else { 32 },
        cmd_ring,
        event_ring,
        event_cycle: 1,
        event_dequeue: 0,
        dcbaa_phys,
        input_ctx_phys,
        output_ctx_phys: 0,
        ep0_ring_phys: 0,
        slot_id: 0,
        devices: alloc::vec::Vec::new(),
        storage_devices: alloc::vec::Vec::new(),
        scanned_ports: alloc::vec::Vec::new(),
        last_port_scan_ms: 0,
    };

    log_ok!(
        "USB",
        "xHCI",
        "Controller running, scanning {} port(s) for HID input",
        max_ports
    );

    let mut connected_ports = 0u8;
    for port in 1..=max_ports {
        let portsc =
            reg_read32(unsafe { core::ptr::addr_of!((*port_regs(&controller, port)).portsc) });
        if portsc & PORTSC_CCS != 0 {
            connected_ports += 1;
            log_ok!(
                "USB",
                "xHCI",
                "Root port {} connected: PORTSC={:#010x}, speed code {}, enabled={}",
                port,
                portsc,
                (portsc & PORTSC_SPEED_MASK) >> 10,
                portsc & PORTSC_PED != 0
            );
        } else {
            log_debug!(
                "USB",
                "xHCI",
                "Root port {} disconnected: PORTSC={:#010x}",
                port,
                portsc
            );
        }
        if portsc & PORTSC_CCS != 0 && !controller.scanned_ports.contains(&port) {
            controller.scanned_ports.push(port);
        }
        if try_setup_hid(&mut controller, port) {
            let count = controller
                .devices
                .iter()
                .filter(|device| device.port == port)
                .count();
            if count != 0 {
                log_ok!("USB", "xHCI", "Configured {} boot HID interface(s) on port {}", count, port);
            } else if controller.storage_devices.iter().any(|device| device.port == port) {
                log_ok!("USB", "xHCI", "Configured mass storage on port {}", port);
            }

            controller.slot_id = 0;
            *EP0_RING_STATE.lock() = (0, 1);
        }
        if controller.slot_id != 0
            || controller.output_ctx_phys != 0
            || controller.ep0_ring_phys != 0
        {
            if !cleanup_failed_setup(&mut controller) {

                break;
            }
        }
    }

    log_ok!(
        "USB",
        "xHCI",
        "{} of {} root ports report a connected device",
        connected_ports,
        max_ports
    );

    if controller.devices.is_empty() && controller.storage_devices.is_empty() {
        log_ok!(
            "USB",
            "xHCI",
            "No supported USB HID or mass storage device found on any port"
        );
    } else {
        log_ok!(
            "USB",
            "xHCI",
            "Configured {} USB HID device(s)",
            controller.devices.len()
        );
    }
    CONTROLLERS.lock().push(controller);
    publish_storage_devices();
}

fn publish_storage_devices() {
    let mut controllers = CONTROLLERS.lock();
    for (controller_index, controller) in controllers.iter_mut().enumerate() {
        for disk in &mut controller.storage_devices {
            if disk.registered { continue; }
            let (name, device) = super::storage::UsbDisk::leak(controller_index, disk.id, disk.capacity);
            crate::block::register_device(name, device);
            disk.registered = true;
            log_ok!("USB", "Storage", "Registered /device/{} ({} MiB)",
                name, disk.capacity.saturating_mul(512) / (1024 * 1024));
        }
    }
}

fn reset_controller(op: *mut OpRegs) -> bool {

    if reg_read32(unsafe { core::ptr::addr_of!((*op).usbcmd) }) & USBCMD_RUN != 0 {
        reg_write32(unsafe { core::ptr::addr_of_mut!((*op).usbcmd) }, 0);
        if !spin_until(1_000, || {
            reg_read32(unsafe { core::ptr::addr_of!((*op).usbsts) }) & USBSTS_HCH != 0
        }) {
            return false;
        }
    }
    reg_write32(
        unsafe { core::ptr::addr_of_mut!((*op).usbcmd) },
        USBCMD_HCRST,
    );
    if !spin_until(1_000, || {
        reg_read32(unsafe { core::ptr::addr_of!((*op).usbcmd) }) & USBCMD_HCRST == 0
    }) {
        return false;
    }
    spin_until(1_000, || {
        reg_read32(unsafe { core::ptr::addr_of!((*op).usbsts) }) & USBSTS_CNR == 0
    })
}

fn port_regs(c: &Controller, port: u8) -> *mut PortRegs {
    let op_base = c.op as *mut u8;
    unsafe { op_base.add(0x400 + (port as usize - 1) * 0x10) as *mut PortRegs }
}

fn cleanup_failed_setup(c: &mut Controller) -> bool {
    if c.slot_id != 0 {
        let slot = c.slot_id;
        let disabled = do_command(
            c,
            Trb {
                parameter: 0,
                status: 0,
                control: make_type(TRB_TYPE_DISABLE_SLOT) | ((slot as u32) << 24),
            },
        )
        .map(|(code, _)| code == COMPLETION_SUCCESS)
        .unwrap_or(false);
        if !disabled {
            log_fail!("USB", "xHCI", "Could not disable failed slot {}", slot);
            return false;
        }
        unsafe {
            let dcbaa = pmm::phys_to_virt(c.dcbaa_phys) as *mut u64;
            core::ptr::write_volatile(dcbaa.add(slot as usize), 0);
        }
        let mut index = 0;
        while index < c.devices.len() {
            if c.devices[index].slot_id == slot {
                let device = c.devices.remove(index);
                if device.protocol == HID_PROTOCOL_MOUSE {
                    super::mouse::clear_buttons();
                }
                free_hid_device(device);
            } else {
                index += 1;
            }
        }
        c.slot_id = 0;
    }

    unsafe {
        if c.output_ctx_phys != 0 {
            pmm::free_frame(c.output_ctx_phys);
            c.output_ctx_phys = 0;
        }
        if c.ep0_ring_phys != 0 {
            pmm::free_frame(c.ep0_ring_phys);
            c.ep0_ring_phys = 0;
        }
        core::ptr::write_bytes(
            pmm::phys_to_virt(c.input_ctx_phys),
            0,
            pmm::PAGE_SIZE as usize,
        );
    }
    *EP0_RING_STATE.lock() = (0, 1);
    true
}

fn do_command(c: &mut Controller, trb: Trb) -> Option<(u32, u8)> {
    let slot_phys = unsafe { c.cmd_ring.push(trb) };
    reg_write32(c.db, 0); 

    let mut result = None;
    let ok = spin_until(1_000, || {
        if let Some(ev) = poll_event_ring(c) {
            if trb_type(ev.control) == TRB_TYPE_COMMAND_COMPLETION && ev.parameter == slot_phys {
                let completion_code = (ev.status >> 24) & 0xFF;
                let slot_id = ((ev.control >> 24) & 0xFF) as u8;
                result = Some((completion_code, slot_id));
                return true;
            }

            dispatch_hid_event(c, ev);
        }
        false
    });
    if !ok {
        return None;
    }
    result
}

fn poll_event_ring(c: &mut Controller) -> Option<Trb> {
    let trb = unsafe { core::ptr::read_volatile(c.event_ring.trbs.add(c.event_dequeue)) };
    if trb.control & TRB_CYCLE != c.event_cycle {
        return None; 
    }

    c.event_dequeue += 1;
    if c.event_dequeue == RING_SIZE {

        c.event_dequeue = 0;
        c.event_cycle ^= 1;
    }

    let erdp_phys = c.event_ring.phys + (c.event_dequeue as u64) * 16;
    reg_write64_split(
        unsafe { core::ptr::addr_of_mut!((*c.rt).ir0_erdp_lo) },
        unsafe { core::ptr::addr_of_mut!((*c.rt).ir0_erdp_hi) },
        erdp_phys | ERDP_BUSY as u64,
    );

    Some(trb)
}

fn try_setup_hid(c: &mut Controller, port: u8) -> bool {
    macro_rules! fail_setup {
        ($stage:literal) => {{
            log_fail!(
                "USB",
                "xHCI",
                "Port {} HID setup failed at {}",
                port,
                $stage
            );
            return false;
        }};
    }

    let regs = port_regs(c, port);

    let portsc = reg_read32(unsafe { core::ptr::addr_of!((*regs).portsc) });
    if portsc & PORTSC_CCS == 0 {
        return false; 
    }

    let Some(output_ctx_phys) = pmm::alloc_frame_zeroed() else {
        log_fail!(
            "USB",
            "xHCI",
            "Out of memory allocating port {} device context",
            port
        );
        return false;
    };
    c.output_ctx_phys = output_ctx_phys;
    let Some(ep0_ring) = Ring::new() else {
        log_fail!(
            "USB",
            "xHCI",
            "Out of memory allocating port {} EP0 ring",
            port
        );
        return false;
    };
    c.ep0_ring_phys = ep0_ring.phys;

    let initial_speed = (portsc & PORTSC_SPEED_MASK) >> 10;
    let warm_reset = port_uses_usb3(c, port) || initial_speed == 4;
    let (reset_bit, reset_change) = if warm_reset {
        (PORTSC_WPR, PORTSC_WRC)
    } else {
        (PORTSC_PR, PORTSC_PRC)
    };

    reg_write32(
        unsafe { core::ptr::addr_of_mut!((*regs).portsc) },
        (portsc & !PORTSC_WRITE_ONE_MASK) | reset_change,
    );
    let before_reset = reg_read32(unsafe { core::ptr::addr_of!((*regs).portsc) });
    reg_write32(
        unsafe { core::ptr::addr_of_mut!((*regs).portsc) },
        (before_reset & !PORTSC_WRITE_ONE_MASK) | reset_bit,
    );
    if !spin_until(1_000, || {
        reg_read32(unsafe { core::ptr::addr_of!((*regs).portsc) }) & reset_change != 0
    }) {
        log_fail!(
            "USB",
            "xHCI",
            "Port {} reset timed out (PORTSC={:#010x})",
            port,
            reg_read32(unsafe { core::ptr::addr_of!((*regs).portsc) })
        );
        return false;
    }

    let after_reset = reg_read32(unsafe { core::ptr::addr_of!((*regs).portsc) });
    reg_write32(
        unsafe { core::ptr::addr_of_mut!((*regs).portsc) },
        (after_reset & !PORTSC_WRITE_ONE_MASK) | reset_change | PORTSC_CSC,
    );

    if after_reset & PORTSC_PED == 0 {
        log_fail!(
            "USB",
            "xHCI",
            "Port {} reset completed but port remains disabled (PORTSC={:#010x})",
            port,
            after_reset
        );
        return false; 
    }
    let speed = (after_reset & PORTSC_SPEED_MASK) >> 10;
    if !(1..=4).contains(&speed) {
        log_fail!(
            "USB",
            "xHCI",
            "Port {} reset left an unusable link (PORTSC={:#010x}, speed code {})",
            port,
            after_reset,
            speed
        );
        return false;
    }
    log_debug!(
        "USB",
        "xHCI",
        "Port {} link after reset: PORTSC={:#010x}, speed code {}",
        port,
        after_reset,
        speed
    );

    let Some((code, slot_id)) = do_command(
        c,
        Trb {
            parameter: 0,
            status: 0,
            control: make_type(TRB_TYPE_ENABLE_SLOT),
        },
    ) else {
        fail_setup!("Enable Slot command timeout");
    };
    if code != COMPLETION_SUCCESS || slot_id == 0 {
        log_fail!(
            "USB",
            "xHCI",
            "Port {} Enable Slot failed (completion {}, slot {})",
            port,
            code,
            slot_id
        );
        return false;
    }
    c.slot_id = slot_id;

    unsafe {
        core::ptr::write_bytes(
            pmm::phys_to_virt(c.input_ctx_phys),
            0,
            pmm::PAGE_SIZE as usize,
        );
        core::ptr::write_bytes(
            pmm::phys_to_virt(c.output_ctx_phys),
            0,
            pmm::PAGE_SIZE as usize,
        );
    }

    unsafe {
        let control_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 0);
        *control_ctx.add(1) = 0b11; 

        let slot_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 1);

        *slot_ctx = (speed << 20) | (1 << 27);

        *slot_ctx.add(1) = (port as u32) << 16;

        let ep0_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 2);

        let max_packet0: u32 = match speed {
            1 | 2 => 8, 
            3 => 64,    
            4 => 512,   
            _ => fail_setup!("unsupported device speed"),
        };
        *ep0_ctx.add(1) = (4 << 3) | (max_packet0 << 16) | (3 << 1); 
        *ep0_ctx.add(2) = (c.ep0_ring_phys | 1) as u32; 
        *ep0_ctx.add(3) = (c.ep0_ring_phys >> 32) as u32;
    }

    unsafe {
        let dcbaa = pmm::phys_to_virt(c.dcbaa_phys) as *mut u64;
        core::ptr::write_volatile(dcbaa.add(c.slot_id as usize), c.output_ctx_phys);
    }

    let Some((code, _)) = do_command(
        c,
        Trb {
            parameter: c.input_ctx_phys,
            status: 0,
            control: make_type(TRB_TYPE_ADDRESS_DEVICE) | ((c.slot_id as u32) << 24),
        },
    ) else {
        fail_setup!("Address Device command timeout");
    };
    if code != COMPLETION_SUCCESS {
        log_fail!(
            "USB",
            "xHCI",
            "Port {} Address Device failed (completion {})",
            port,
            code
        );
        return false;
    }

    if speed == 1 || speed == 2 {
        let Some(device_desc_phys) = pmm::alloc_frame_zeroed() else {
            fail_setup!("allocating device descriptor buffer");
        };
        if !control_transfer_in(
            c,
            setup_packet(
                REQ_TYPE_STD_DEVICE_IN,
                REQ_GET_DESCRIPTOR,
                DESC_TYPE_DEVICE,
                0,
                8,
            ),
            device_desc_phys,
            8,
        ) {
            fail_setup!("reading full-speed device descriptor");
        }
        let packet_size =
            unsafe { core::ptr::read_volatile(pmm::phys_to_virt(device_desc_phys).add(7)) };
        let valid_packet_size = match speed {
            1 => matches!(packet_size, 8 | 16 | 32 | 64),
            2 => packet_size == 8,
            _ => false,
        };
        if !valid_packet_size {
            fail_setup!("invalid EP0 max packet size");
        }
        if packet_size != 8 {
            unsafe {
                let control_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 0);
                core::ptr::write_bytes(control_ctx, 0, c.context_size / 4);
                *control_ctx.add(1) = 1 << 1; 
                let ep0_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 2);
                *ep0_ctx.add(1) = (4 << 3) | ((packet_size as u32) << 16) | (3 << 1);
                *ep0_ctx.add(2) = (c.ep0_ring_phys | 1) as u32;
                *ep0_ctx.add(3) = (c.ep0_ring_phys >> 32) as u32;
            }
            let Some((code, _)) = do_command(
                c,
                Trb {
                    parameter: c.input_ctx_phys,
                    status: 0,
                    control: make_type(TRB_TYPE_EVALUATE_CONTEXT) | ((c.slot_id as u32) << 24),
                },
            ) else {
                fail_setup!("Evaluate Context command timeout");
            };
            if code != COMPLETION_SUCCESS {
                log_fail!(
                    "USB",
                    "xHCI",
                    "Port {} Evaluate Context failed (completion {})",
                    port,
                    code
                );
                return false;
            }
        }
    }

    let Some(desc_phys) = pmm::alloc_frame_zeroed() else {
        fail_setup!("allocating configuration descriptor buffer");
    };
    if !control_transfer_in(
        c,
        setup_packet(
            REQ_TYPE_STD_DEVICE_IN,
            REQ_GET_DESCRIPTOR,
            DESC_TYPE_CONFIGURATION,
            0,
            9,
        ),
        desc_phys,
        9,
    ) {
        fail_setup!("reading configuration descriptor header");
    }

    let desc = pmm::phys_to_virt(desc_phys);
    let total_length = unsafe { core::ptr::read_volatile(desc.add(2)) as usize }
        | ((unsafe { core::ptr::read_volatile(desc.add(3)) as usize }) << 8);
    if !(9..=pmm::PAGE_SIZE as usize).contains(&total_length) {
        fail_setup!("invalid configuration descriptor length");
    }
    if !control_transfer_in(
        c,
        setup_packet(
            REQ_TYPE_STD_DEVICE_IN,
            REQ_GET_DESCRIPTOR,
            DESC_TYPE_CONFIGURATION,
            0,
            total_length as u16,
        ),
        desc_phys,
        total_length as u32,
    ) {
        fail_setup!("reading full configuration descriptor");
    }
    let config = unsafe { core::slice::from_raw_parts(desc, total_length) };
    if let Some(endpoints) = find_mass_storage(config) {
        if try_setup_mass_storage(c, port, endpoints) { return true; }
        fail_setup!("initializing USB mass storage interface");
    }
    let hid_endpoints = find_boot_hid(config, speed);
    if hid_endpoints.is_empty() {
        fail_setup!("finding HID boot interface");
    }

    if !control_transfer_out(
        c,
        setup_packet(
            REQ_TYPE_STD_DEVICE_OUT,
            REQ_SET_CONFIGURATION,
            hid_endpoints[0].configuration as u16,
            0,
            0,
        ),
    ) {
        fail_setup!("SET_CONFIGURATION request");
    }

    let first_device_index = c.devices.len();
    for endpoint in hid_endpoints {
        let Some(interval) = xhci_interval(speed, endpoint.interval) else {
            fail_setup!("decoding HID polling interval");
        };

        if endpoint.protocol == HID_PROTOCOL_KEYBOARD
            && !control_transfer_out(
                c,
                setup_packet(
                    REQ_TYPE_CLASS_IFACE_OUT,
                    HID_REQ_SET_PROTOCOL,
                    HID_PROTOCOL_BOOT,
                    endpoint.interface as u16,
                    0,
                ),
            )
        {
            fail_setup!("HID keyboard SET_PROTOCOL request");
        }

        let _ = control_transfer_out(
            c,
            setup_packet(
                REQ_TYPE_CLASS_IFACE_OUT,
                HID_REQ_SET_IDLE,
                0,
                endpoint.interface as u16,
                0,
            ),
        );

        let Some(ring) = Ring::new() else {
            fail_setup!("allocating HID transfer ring");
        };

        c.devices.push(UsbHidDevice {
            port,
            slot_id: c.slot_id,
            dci: endpoint.dci,
            protocol: endpoint.protocol,
            report_len: endpoint.report_len,
            ring,
            output_ctx_phys: 0,
            ep0_ring_phys: 0,
            report_phys: [0; KEYBOARD_REPORT_QUEUE_DEPTH],
            report_enqueue: 0,
            report_dequeue: 0,
            prev_keys: [0; 6],
            prev_modifiers: 0,
            mouse_report_logged: false,
        });

        unsafe {
            let control_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 0);
            core::ptr::write_bytes(control_ctx, 0, c.context_size / 4);

            *control_ctx.add(1) = (1 << 0) | (1 << endpoint.dci);
            let slot_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 1);
            let output_slot_ctx = pmm::phys_to_virt(c.output_ctx_phys) as *const u32;
            for dword in 0..(c.context_size / 4) {
                *slot_ctx.add(dword) = core::ptr::read_volatile(output_slot_ctx.add(dword));
            }
            let context_entries_mask = 0x1F << 27;
            *slot_ctx = (*slot_ctx & !context_entries_mask) | ((endpoint.dci as u32) << 27);

            let ep_ctx =
                input_context_entry(c.input_ctx_phys, c.context_size, endpoint.dci as usize + 1);
            const EP_TYPE_INTERRUPT_IN: u32 = 7;
            *ep_ctx.add(1) =
                (EP_TYPE_INTERRUPT_IN << 3) | ((endpoint.max_packet as u32) << 16) | (3 << 1);
            *ep_ctx = interval << 16;
            *ep_ctx.add(2) = (c.devices.last().unwrap().ring.phys | 1) as u32;
            *ep_ctx.add(3) = (c.devices.last().unwrap().ring.phys >> 32) as u32;

            *ep_ctx.add(4) = endpoint.report_len as u32 | ((endpoint.max_packet as u32) << 16);
        }

        let Some((code, _)) = do_command(
            c,
            Trb {
                parameter: c.input_ctx_phys,
                status: 0,
                control: make_type(TRB_TYPE_CONFIGURE_ENDPOINT) | ((c.slot_id as u32) << 24),
            },
        ) else {
            fail_setup!("Configure Endpoint command timeout");
        };
        if code != COMPLETION_SUCCESS {
            log_fail!(
                "USB",
                "xHCI",
                "Port {} interface {} Configure Endpoint failed (completion {})",
                port,
                endpoint.interface,
                code
            );
            return false;
        }

        for index in 0..KEYBOARD_REPORT_QUEUE_DEPTH {
            let Some(phys) = pmm::alloc_frame_zeroed() else {
                fail_setup!("allocating HID report buffers");
            };
            c.devices.last_mut().unwrap().report_phys[index] = phys;
        }
        let device = c.devices.last_mut().unwrap();
        for _ in 0..KEYBOARD_REPORT_QUEUE_DEPTH {
            queue_report_read(device);
        }
        ring_endpoint_doorbell(c.db, device);
        log_ok!(
            "USB",
            "xHCI",
            "Port {} boot {} interface {} ready",
            port,
            if endpoint.protocol == HID_PROTOCOL_MOUSE {
                "mouse"
            } else {
                "keyboard"
            },
            endpoint.interface
        );
    }

    c.devices[first_device_index].output_ctx_phys = c.output_ctx_phys;
    c.devices[first_device_index].ep0_ring_phys = c.ep0_ring_phys;
    c.output_ctx_phys = 0;
    c.ep0_ring_phys = 0;

    true
}

static EP0_RING_STATE: IrqMutex<(usize, u32)> = IrqMutex::new((0, 1));

fn control_transfer_in(c: &mut Controller, setup: u64, buf_phys: u64, len: u32) -> bool {
    control_transfer(c, setup, Some((buf_phys, len)), true)
}

fn control_transfer_out(c: &mut Controller, setup: u64) -> bool {
    control_transfer(c, setup, None, false)
}

fn control_transfer(
    c: &mut Controller,
    setup: u64,
    data: Option<(u64, u32)>,
    data_is_in: bool,
) -> bool {
    let (enqueue, cycle) = *EP0_RING_STATE.lock();
    let mut ring = Ring {
        trbs: pmm::phys_to_virt(c.ep0_ring_phys) as *mut Trb,
        phys: c.ep0_ring_phys,
        enqueue,
        cycle,
    };

    let transfer_type: u32 = match &data {
        Some(_) if data_is_in => 3, 
        Some(_) => 2,               
        None => 0,                  
    };

    let setup_trb = Trb {
        parameter: setup,
        status: 8, 
        control: make_type(TRB_TYPE_SETUP_STAGE) | TRB_CHAIN | (transfer_type << 16) | (1 << 6), 
    };
    unsafe { ring.push(setup_trb) };

    if let Some((buf_phys, len)) = data {
        let data_trb = Trb {
            parameter: buf_phys,
            status: len,
            control: make_type(TRB_TYPE_DATA_STAGE)
                | TRB_CHAIN
                | if data_is_in { 1 << 16 } else { 0 },
        };
        unsafe { ring.push(data_trb) };
    }

    let status_is_in = !data_is_in || data.is_none();
    let status_trb = Trb {
        parameter: 0,
        status: 0,
        control: make_type(TRB_TYPE_STATUS_STAGE)
            | TRB_IOC
            | if status_is_in { 1 << 16 } else { 0 },
    };
    let status_slot_phys = unsafe { ring.push(status_trb) };

    *EP0_RING_STATE.lock() = (ring.enqueue, ring.cycle);

    reg_write32(unsafe { c.db.add(c.slot_id as usize) }, 1);

    let mut saw_completion = false;
    let mut completion_code = None;
    let responded = spin_until(1_000, || {
        if let Some(ev) = poll_event_ring(c) {
            if trb_type(ev.control) == TRB_TYPE_TRANSFER_EVENT && ev.parameter == status_slot_phys {
                let code = (ev.status >> 24) & 0xFF;
                completion_code = Some(code);
                saw_completion = code == COMPLETION_SUCCESS || code == COMPLETION_SHORT_PACKET;
                return true;
            }
            dispatch_hid_event(c, ev);
        }
        false
    });
    if !responded || !saw_completion {
        if let Some(code) = completion_code {
            log_fail!(
                "USB",
                "xHCI",
                "Slot {} EP0 request {} (value {:#06x}, {} data bytes) failed with completion {}",
                c.slot_id,
                ((setup >> 8) & 0xff) as u8,
                ((setup >> 16) & 0xffff) as u16,
                data.map(|(_, len)| len).unwrap_or(0),
                code
            );
        } else {
            log_fail!(
                "USB",
                "xHCI",
                "Slot {} EP0 request {} (value {:#06x}, {} data bytes) timed out",
                c.slot_id,
                ((setup >> 8) & 0xff) as u8,
                ((setup >> 16) & 0xffff) as u16,
                data.map(|(_, len)| len).unwrap_or(0)
            );
        }
    }
    responded && saw_completion
}

fn try_setup_mass_storage(c: &mut Controller, port: u8, endpoints: MassStorageEndpoints) -> bool {
    if endpoints.bulk_in_packet > 1024 || endpoints.bulk_out_packet > 1024 {
        log_fail!("USB", "Storage", "Port {} reports unsupported bulk packet size", port);
        return false;
    }
    if !control_transfer_out(c, setup_packet(
        REQ_TYPE_STD_DEVICE_OUT,
        REQ_SET_CONFIGURATION,
        endpoints.configuration as u16,
        0,
        0,
    )) {
        log_fail!("USB", "Storage", "Port {} SET_CONFIGURATION failed", port);
        return false;
    }

    let Some(bulk_in_ring) = Ring::new() else { return false; };
    let Some(bulk_out_ring) = Ring::new() else {
        pmm::free_frame(bulk_in_ring.phys);
        return false;
    };
    let Some(buffer_phys) = pmm::alloc_frame_zeroed() else {
        pmm::free_frame(bulk_in_ring.phys);
        pmm::free_frame(bulk_out_ring.phys);
        return false;
    };
    let Some(csw_phys) = pmm::alloc_frame_zeroed() else {
        pmm::free_frame(bulk_in_ring.phys);
        pmm::free_frame(bulk_out_ring.phys);
        pmm::free_frame(buffer_phys);
        return false;
    };

    let max_dci = endpoints.bulk_in_dci.max(endpoints.bulk_out_dci) as usize;
    unsafe {
        let control_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 0);
        core::ptr::write_bytes(control_ctx, 0, c.context_size / 4);
        *control_ctx.add(1) = (1 << 0) | (1 << endpoints.bulk_in_dci) | (1 << endpoints.bulk_out_dci);

        let slot_ctx = input_context_entry(c.input_ctx_phys, c.context_size, 1);
        let output_slot_ctx = pmm::phys_to_virt(c.output_ctx_phys) as *const u32;
        for dword in 0..(c.context_size / 4) {
            *slot_ctx.add(dword) = core::ptr::read_volatile(output_slot_ctx.add(dword));
        }
        let context_entries_mask = 0x1f << 27;
        *slot_ctx = (*slot_ctx & !context_entries_mask) | ((max_dci as u32) << 27);

        configure_bulk_endpoint(c, endpoints.bulk_in_dci, endpoints.bulk_in_packet, bulk_in_ring.phys, true);
        configure_bulk_endpoint(c, endpoints.bulk_out_dci, endpoints.bulk_out_packet, bulk_out_ring.phys, false);
    }

    let Some((code, _)) = do_command(c, Trb {
        parameter: c.input_ctx_phys,
        status: 0,
        control: make_type(TRB_TYPE_CONFIGURE_ENDPOINT) | ((c.slot_id as u32) << 24),
    }) else {
        pmm::free_frame(bulk_in_ring.phys);
        pmm::free_frame(bulk_out_ring.phys);
        pmm::free_frame(buffer_phys);
        pmm::free_frame(csw_phys);
        return false;
    };
    if code != COMPLETION_SUCCESS {
        log_fail!("USB", "Storage", "Port {} Configure Endpoint failed (completion {})", port, code);
        pmm::free_frame(bulk_in_ring.phys);
        pmm::free_frame(bulk_out_ring.phys);
        pmm::free_frame(buffer_phys);
        pmm::free_frame(csw_phys);
        return false;
    }

    let mut disk = UsbMassStorage {
        id: NEXT_STORAGE_ID.fetch_add(1, Ordering::Relaxed),
        port,
        slot_id: c.slot_id,
        interface: endpoints.interface,
        bulk_in_dci: endpoints.bulk_in_dci,
        bulk_out_dci: endpoints.bulk_out_dci,
        bulk_in_packet: endpoints.bulk_in_packet,
        bulk_out_packet: endpoints.bulk_out_packet,
        bulk_in_ring,
        bulk_out_ring,
        output_ctx_phys: c.output_ctx_phys,
        ep0_ring_phys: c.ep0_ring_phys,
        buffer_phys,
        csw_phys,
        capacity: 0,
        next_tag: 1,
        registered: false,
    };
    c.output_ctx_phys = 0;
    c.ep0_ring_phys = 0;

    let inquiry = [0x12, 0, 0, 0, 36, 0];
    if mass_storage_command(c, &mut disk, &inquiry, 36, true) != Ok(36) {
        log_fail!("USB", "Storage", "Port {} SCSI INQUIRY failed", port);
        free_mass_storage(disk);
        return false;
    }
    let capacity_cdb = [0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut capacity = None;
    for attempt in 0..10 {
        if mass_storage_command(c, &mut disk, &capacity_cdb, 8, true) == Ok(8) {
            let data = unsafe { core::slice::from_raw_parts(pmm::phys_to_virt(disk.buffer_phys), 8) };
            let last_lba = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as u64;
            let block_size = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
            if block_size == crate::fat32::SECTOR_SIZE as u32 && last_lba != u32::MAX as u64 {
                capacity = Some(last_lba + 1);
                break;
            }
            if last_lba == u32::MAX as u64 {
                let capacity16 = [0x9e, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0];
                if mass_storage_command(c, &mut disk, &capacity16, 32, true) == Ok(32) {
                    let data = unsafe { core::slice::from_raw_parts(pmm::phys_to_virt(disk.buffer_phys), 32) };
                    let last_lba = u64::from_be_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
                    let block_size = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
                    if block_size == crate::fat32::SECTOR_SIZE as u32 && last_lba != u64::MAX {
                        capacity = Some(last_lba + 1);
                        break;
                    }
                }
            }
        }
        if attempt != 9 { crate::scheduler::sleep_ms(100); }
    }
    let Some(capacity) = capacity else {
        log_fail!("USB", "Storage", "Port {} could not read 512-byte SCSI capacity", port);
        free_mass_storage(disk);
        return false;
    };
    disk.capacity = capacity;
    c.storage_devices.push(disk);
    log_ok!("USB", "Storage", "Flash drive ready on port {}: {} sectors ({} MiB)",
        port, capacity, capacity.saturating_mul(512) / (1024 * 1024));
    true
}

unsafe fn configure_bulk_endpoint(c: &Controller, dci: u8, max_packet: u16, ring_phys: u64, input: bool) {
    let ep_ctx = input_context_entry(c.input_ctx_phys, c.context_size, dci as usize + 1);
    let endpoint_type = if input { 6u32 } else { 2u32 };
    *ep_ctx = 0;
    *ep_ctx.add(1) = (endpoint_type << 3) | (3 << 1) | ((max_packet as u32) << 16);
    *ep_ctx.add(2) = (ring_phys | 1) as u32;
    *ep_ctx.add(3) = (ring_phys >> 32) as u32;
    *ep_ctx.add(4) = max_packet as u32;
}

fn free_mass_storage(disk: UsbMassStorage) {
    pmm::free_frame(disk.bulk_in_ring.phys);
    pmm::free_frame(disk.bulk_out_ring.phys);
    pmm::free_frame(disk.buffer_phys);
    pmm::free_frame(disk.csw_phys);
    if disk.output_ctx_phys != 0 { pmm::free_frame(disk.output_ctx_phys); }
    if disk.ep0_ring_phys != 0 { pmm::free_frame(disk.ep0_ring_phys); }
}

fn bulk_transfer(c: &mut Controller, disk: &mut UsbMassStorage, input: bool, buffer_phys: u64, length: u32) -> Result<usize, ()> {
    if length == 0 || length > pmm::PAGE_SIZE as u32 { return Err(()); }
    let (dci, slot_id, trb_phys) = if input {
        let slot = unsafe { disk.bulk_in_ring.push(Trb {
            parameter: buffer_phys,
            status: length,
            control: make_type(TRB_TYPE_NORMAL) | TRB_IOC | TRB_ISP,
        }) };
        (disk.bulk_in_dci, disk.slot_id, slot)
    } else {
        let slot = unsafe { disk.bulk_out_ring.push(Trb {
            parameter: buffer_phys,
            status: length,
            control: make_type(TRB_TYPE_NORMAL) | TRB_IOC | TRB_ISP,
        }) };
        (disk.bulk_out_dci, disk.slot_id, slot)
    };
    reg_write32(unsafe { c.db.add(slot_id as usize) }, dci as u32);
    let mut completion = None;
    let finished = spin_until(3_000, || {
        while let Some(event) = poll_event_ring(c) {
            if trb_type(event.control) == TRB_TYPE_TRANSFER_EVENT
                && event.parameter == trb_phys
                && ((event.control >> 24) as u8) == slot_id
            {
                completion = Some(((event.status >> 24) & 0xff, (event.status & 0x00ff_ffff) as usize));
                return true;
            }
            dispatch_hid_event(c, event);
        }
        false
    });
    if !finished { return Err(()); }
    let (code, residual) = completion.ok_or(())?;
    if code != COMPLETION_SUCCESS && code != COMPLETION_SHORT_PACKET { return Err(()); }
    Ok((length as usize).saturating_sub(residual).min(length as usize))
}

fn mass_storage_command(c: &mut Controller, disk: &mut UsbMassStorage, cdb: &[u8], data_len: u32, input: bool) -> Result<usize, ()> {
    if cdb.is_empty() || cdb.len() > 16 || data_len > pmm::PAGE_SIZE as u32 { return Err(()); }
    let tag = disk.next_tag;
    disk.next_tag = disk.next_tag.wrapping_add(1).max(1);
    let cbw = unsafe { core::slice::from_raw_parts_mut(pmm::phys_to_virt(disk.buffer_phys), 31) };
    cbw.fill(0);
    cbw[0..4].copy_from_slice(&MSC_CBW_SIGNATURE.to_le_bytes());
    cbw[4..8].copy_from_slice(&tag.to_le_bytes());
    cbw[8..12].copy_from_slice(&data_len.to_le_bytes());
    cbw[12] = if input { 0x80 } else { 0 };
    cbw[14] = cdb.len() as u8;
    cbw[15..15 + cdb.len()].copy_from_slice(cdb);
    if bulk_transfer(c, disk, false, disk.buffer_phys, 31)? != 31 { return Err(()); }

    let transferred = if data_len == 0 {
        0
    } else {
        bulk_transfer(c, disk, input, disk.buffer_phys, data_len)?
    };
    let csw = unsafe { core::slice::from_raw_parts_mut(pmm::phys_to_virt(disk.csw_phys), 13) };
    csw.fill(0);
    if bulk_transfer(c, disk, true, disk.csw_phys, 13)? != 13 { return Err(()); }
    let csw = unsafe { core::slice::from_raw_parts(pmm::phys_to_virt(disk.csw_phys), 13) };
    let signature = u32::from_le_bytes([csw[0], csw[1], csw[2], csw[3]]);
    let returned_tag = u32::from_le_bytes([csw[4], csw[5], csw[6], csw[7]]);
    let status = csw[12];
    if signature != MSC_CSW_SIGNATURE || returned_tag != tag || status != 0 { return Err(()); }
    Ok(transferred)
}

fn with_storage_device<T>(controller_index: usize, id: u64, operation: impl FnOnce(&mut Controller, &mut UsbMassStorage) -> Result<T, crate::block::BlockError>) -> Result<T, crate::block::BlockError> {
    while POLLING.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        crate::scheduler::yield_now();
    }
    let mut controllers = {
        let mut registry = CONTROLLERS.lock();
        core::mem::take(&mut *registry)
    };
    let result = if let Some(controller) = controllers.get_mut(controller_index) {
        if let Some(index) = controller.storage_devices.iter().position(|device| device.id == id) {
            let mut device = controller.storage_devices.remove(index);
            let result = operation(controller, &mut device);
            controller.storage_devices.insert(index.min(controller.storage_devices.len()), device);
            result
        } else {
            Err(crate::block::BlockError::BadBuffer)
        }
    } else {
        Err(crate::block::BlockError::BadBuffer)
    };
    *CONTROLLERS.lock() = controllers;
    POLLING.store(false, Ordering::Release);
    result
}

pub fn read_storage_sector(controller_index: usize, id: u64, lba: u64, out: &mut [u8; crate::fat32::SECTOR_SIZE]) -> Result<(), crate::block::BlockError> {
    with_storage_device(controller_index, id, |controller, disk| {
        let cdb = read_write_cdb(lba, false);
        let count = mass_storage_command(controller, disk, &cdb[..cdb_length(lba)], crate::fat32::SECTOR_SIZE as u32, true)
            .map_err(|_| crate::block::BlockError::BadBuffer)?;
        if count != crate::fat32::SECTOR_SIZE { return Err(crate::block::BlockError::BadBuffer); }
        let data = unsafe { core::slice::from_raw_parts(pmm::phys_to_virt(disk.buffer_phys), crate::fat32::SECTOR_SIZE) };
        out.copy_from_slice(data);
        Ok(())
    })
}

pub fn write_storage_sector(controller_index: usize, id: u64, lba: u64, input: &[u8; crate::fat32::SECTOR_SIZE]) -> Result<(), crate::block::BlockError> {
    with_storage_device(controller_index, id, |controller, disk| {
        let data = unsafe { core::slice::from_raw_parts_mut(pmm::phys_to_virt(disk.buffer_phys), crate::fat32::SECTOR_SIZE) };
        data.copy_from_slice(input);
        let cdb = read_write_cdb(lba, true);
        let count = mass_storage_command(controller, disk, &cdb[..cdb_length(lba)], crate::fat32::SECTOR_SIZE as u32, false)
            .map_err(|_| crate::block::BlockError::BadBuffer)?;
        if count != crate::fat32::SECTOR_SIZE { return Err(crate::block::BlockError::BadBuffer); }
        Ok(())
    })
}

pub fn flush_storage(controller_index: usize, id: u64) -> Result<(), crate::block::BlockError> {
    // BOT writes complete synchronously. A number of flash devices stall the
    // optional SYNCHRONIZE CACHE command, so keep flush compatible with them.
    with_storage_device(controller_index, id, |_controller, _disk| Ok(()))
}

fn cdb_length(lba: u64) -> usize { if lba <= u32::MAX as u64 { 10 } else { 16 } }

fn read_write_cdb(lba: u64, write: bool) -> [u8; 16] {
    let mut cdb = [0u8; 16];
    if lba <= u32::MAX as u64 {
        cdb[0] = if write { 0x2a } else { 0x28 };
        cdb[2..6].copy_from_slice(&(lba as u32).to_be_bytes());
        cdb[8] = 1;
    } else {
        cdb[0] = if write { 0x8a } else { 0x88 };
        cdb[2..10].copy_from_slice(&lba.to_be_bytes());
        cdb[13] = 1;
    }
    cdb
}

fn queue_report_read(keyboard: &mut UsbHidDevice) {
    let buffer = keyboard.report_enqueue;
    let trb = Trb {
        parameter: keyboard.report_phys[buffer],
        status: keyboard.report_len as u32,
        control: make_type(TRB_TYPE_NORMAL) | TRB_IOC,
    };
    unsafe { keyboard.ring.push(trb) };
    keyboard.report_enqueue = (buffer + 1) % KEYBOARD_REPORT_QUEUE_DEPTH;
}

fn ring_endpoint_doorbell(db: *mut u32, keyboard: &UsbHidDevice) {
    reg_write32(
        unsafe { db.add(keyboard.slot_id as usize) },
        keyboard.dci as u32,
    );
}

fn submit_report_read(db: *mut u32, keyboard: &mut UsbHidDevice) {
    queue_report_read(keyboard);
    ring_endpoint_doorbell(db, keyboard);
}

pub fn poll() {
    if POLLING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    let mut controllers = {
        let mut registry = CONTROLLERS.lock();
        core::mem::take(&mut *registry)
    };
    for c in controllers.iter_mut() {
        while let Some(ev) = poll_event_ring(c) {
            dispatch_hid_event(c, ev);
        }
        poll_root_ports(c);
    }
    *CONTROLLERS.lock() = controllers;
    publish_storage_devices();
    POLLING.store(false, Ordering::Release);
}

pub fn service() {
    loop {
        poll();
        crate::scheduler::sleep_ms(10);
    }
}

fn poll_root_ports(c: &mut Controller) {

    if c.slot_id != 0 || c.output_ctx_phys != 0 || c.ep0_ring_phys != 0 {
        return;
    }
    let now = crate::pit::uptime_ms();
    if now.saturating_sub(c.last_port_scan_ms) < 250 {
        return;
    }
    c.last_port_scan_ms = now;

    for port in 1..=c.max_ports {
        let portsc = reg_read32(unsafe { core::ptr::addr_of!((*port_regs(c, port)).portsc) });
        if portsc & PORTSC_CCS == 0 {
            let slot_id = c.devices.iter().find(|device| device.port == port).map(|device| device.slot_id)
                .or_else(|| c.storage_devices.iter().find(|device| device.port == port).map(|device| device.slot_id));
            if let Some(slot_id) = slot_id {
                let mut disconnected = alloc::vec::Vec::new();
                let mut index = 0;
                while index < c.devices.len() {
                    if c.devices[index].port == port && c.devices[index].slot_id == slot_id {
                        disconnected.push(c.devices.remove(index));
                    } else {
                        index += 1;
                    }
                }
                let mut disconnected_storage = alloc::vec::Vec::new();
                let mut index = 0;
                while index < c.storage_devices.len() {
                    if c.storage_devices[index].port == port && c.storage_devices[index].slot_id == slot_id {
                        disconnected_storage.push(c.storage_devices.remove(index));
                    } else {
                        index += 1;
                    }
                }
                if !disable_hid_slot(c, slot_id, port) {

                    c.devices.extend(disconnected);
                    c.storage_devices.extend(disconnected_storage);
                    continue;
                }
                for device in disconnected {
                    if device.protocol == HID_PROTOCOL_MOUSE {
                        super::mouse::clear_buttons();
                    }
                    free_hid_device(device);
                }
                for disk in disconnected_storage { free_mass_storage(disk); }
                log_ok!("USB", "xHCI", "USB device disconnected from port {}", port);
            }
            c.scanned_ports.retain(|scanned| *scanned != port);
            continue;
        }

        if c.scanned_ports.contains(&port) {
            continue;
        }
        c.scanned_ports.push(port);
        if try_setup_hid(c, port) {
            log_ok!("USB", "xHCI", "USB HID device connected on port {}", port);
            c.slot_id = 0;
            *EP0_RING_STATE.lock() = (0, 1);
        } else if !cleanup_failed_setup(c) {

            return;
        }
    }
}

fn disable_hid_slot(c: &mut Controller, slot_id: u8, port: u8) -> bool {
    let result = do_command(
        c,
        Trb {
            parameter: 0,
            status: 0,
            control: make_type(TRB_TYPE_DISABLE_SLOT) | ((slot_id as u32) << 24),
        },
    )
    .map(|(code, _)| code == COMPLETION_SUCCESS)
    .unwrap_or(false);
    if result {
        unsafe {
            let dcbaa = pmm::phys_to_virt(c.dcbaa_phys) as *mut u64;
            core::ptr::write_volatile(dcbaa.add(slot_id as usize), 0);
        }
    } else {
        log_fail!(
            "USB",
            "xHCI",
            "Could not disable slot {} after disconnect on port {}",
            slot_id,
            port
        );
    }
    result
}

fn free_hid_device(keyboard: UsbHidDevice) {
    for phys in keyboard.report_phys {
        if phys != 0 {
            pmm::free_frame(phys);
        }
    }
    pmm::free_frame(keyboard.ring.phys);
    if keyboard.output_ctx_phys != 0 {
        pmm::free_frame(keyboard.output_ctx_phys);
    }
    if keyboard.ep0_ring_phys != 0 {
        pmm::free_frame(keyboard.ep0_ring_phys);
    }
}

fn dispatch_hid_event(c: &mut Controller, ev: Trb) {
    if trb_type(ev.control) != TRB_TYPE_TRANSFER_EVENT {
        return; 
    }
    let slot_id = (ev.control >> 24) as u8;
    let dci = ((ev.control >> 16) & 0x1f) as u8;
    let db = c.db;
    let Some(keyboard) = c
        .devices
        .iter_mut()
        .find(|k| k.slot_id == slot_id && k.dci == dci)
    else {
        return;
    };
    let completion_code = (ev.status >> 24) & 0xFF;
    if completion_code == COMPLETION_SUCCESS || completion_code == COMPLETION_SHORT_PACKET {
        let requested = keyboard.report_len as usize;
        let residual = (ev.status & 0x00ff_ffff) as usize;
        let actual_len = requested.saturating_sub(residual).min(requested);
        handle_hid_report(keyboard, keyboard.report_dequeue, actual_len);
    } else {
        if keyboard.protocol == HID_PROTOCOL_KEYBOARD {

            for &code in &keyboard.prev_keys {
                if code != 0 {
                    crate::input::push(code, false, 0);
                }
            }
            for modifier in 0..8 {
                if keyboard.prev_modifiers & (1 << modifier) != 0 {
                    crate::input::push(0xE0 + modifier, false, 0);
                }
            }
            keyboard.prev_keys = [0; 6];
            keyboard.prev_modifiers = 0;
        } else {
            super::mouse::clear_buttons();
        }
    }
    keyboard.report_dequeue = (keyboard.report_dequeue + 1) % KEYBOARD_REPORT_QUEUE_DEPTH;

    submit_report_read(db, keyboard);
}

#[rustfmt::skip]
const HID_UNSHIFTED: [u8; 0x39] = [
    0,    0,    0,    0,    b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', 
    b'k', b'l', b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v',             
    b'w', b'x', b'y', b'z', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8',             
    b'9', b'0', b'\n', 0x1B, 0x08, b'\t', b' ', b'-', b'=', b'[', b']',                 
    b'\\', 0,   b';', b'\'', b'`', b',', b'.', b'/',                                    
];

#[rustfmt::skip]
const HID_SHIFTED: [u8; 0x39] = [
    0,    0,    0,    0,    b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J',
    b'K', b'L', b'M', b'N', b'O', b'P', b'Q', b'R', b'S', b'T', b'U', b'V',
    b'W', b'X', b'Y', b'Z', b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*',
    b'(', b')', b'\n', 0x1B, 0x08, b'\t', b' ', b'_', b'+', b'{', b'}',
    b'|', 0,    b':', b'"', b'~', b'<', b'>', b'?',
];

const HID_KEYCODE_CAPS_LOCK: u8 = 0x39;

const MOD_LCTRL: u8 = 1 << 0;
const MOD_LSHIFT: u8 = 1 << 1;
const MOD_LALT: u8 = 1 << 2;
const MOD_LGUI: u8 = 1 << 3;
const MOD_RCTRL: u8 = 1 << 4;
const MOD_RSHIFT: u8 = 1 << 5;
const MOD_RALT: u8 = 1 << 6;
const MOD_RGUI: u8 = 1 << 7;

struct RingBuffer {
    buf: [u8; 256],
    head: usize,
    tail: usize,
    len: usize,
}
impl RingBuffer {
    const fn new() -> Self {
        RingBuffer {
            buf: [0; 256],
            head: 0,
            tail: 0,
            len: 0,
        }
    }
    fn push(&mut self, byte: u8) {
        if self.len == 256 {
            self.tail = (self.tail + 1) % 256;
            self.len -= 1;
        }
        self.buf[self.head] = byte;
        self.head = (self.head + 1) % 256;
        self.len += 1;
    }
    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let b = self.buf[self.tail];
        self.tail = (self.tail + 1) % 256;
        self.len -= 1;
        Some(b)
    }
}
static ASCII_QUEUE: IrqMutex<RingBuffer> = IrqMutex::new(RingBuffer::new());

fn push_ascii_escape(sequence: &[u8]) {
    let mut queue = ASCII_QUEUE.lock();
    for &byte in sequence { queue.push(byte); }
}

fn handle_hid_report(keyboard: &mut UsbHidDevice, buffer: usize, actual_len: usize) {
    let report = unsafe {
        core::slice::from_raw_parts(pmm::phys_to_virt(keyboard.report_phys[buffer]), actual_len)
    };
    if keyboard.protocol == HID_PROTOCOL_MOUSE {
        if !keyboard.mouse_report_logged {
            keyboard.mouse_report_logged = true;
            log_ok!(
                "USB",
                "Mouse",
                "First interrupt report on port {} ({} bytes): {:02x?}",
                keyboard.port,
                report.len(),
                report
            );
        }
        super::mouse::handle_report(report);
        return;
    }
    let Ok(report) = <&[u8; 8]>::try_from(report) else {
        return;
    };
    let keys = [report[2], report[3], report[4], report[5], report[6], report[7]];
    if keys.iter().any(|code| (1..=3).contains(code)) { return; } 
    let previous_keys = keyboard.prev_keys;
    let mut focused = false;
    for modifier in 0..8 {
        let mask = 1 << modifier;
        if (keyboard.prev_modifiers ^ report[0]) & mask != 0 {
            focused |= crate::input::push(0xE0 + modifier, report[0] & mask != 0, report[0]);
        }
    }
    for &code in &keyboard.prev_keys {
        if code != 0 && !keys.contains(&code) {
            focused |= crate::input::push(code, false, report[0]);
        }
    }
    for &code in &keys {
        if code != 0 && !keyboard.prev_keys.contains(&code) {
            focused |= crate::input::push(code, true, report[0]);
        }
    }
    keyboard.prev_modifiers = report[0];
    let mut caps = CAPS_LOCK.load(Ordering::Relaxed);
    let (bytes, count) = translate_boot_report(&mut keyboard.prev_keys, &mut caps, report);
    CAPS_LOCK.store(caps, Ordering::Relaxed);
    if focused { return; }

    for &code in &keys {
        if code == 0 || previous_keys.contains(&code) { continue; }
        match code {
            0x4a => push_ascii_escape(b"\x1b[H"), 
            0x4c => push_ascii_escape(b"\x1b[3~"), 
            0x4d => push_ascii_escape(b"\x1b[F"), 
            0x4f => push_ascii_escape(b"\x1b[C"), 
            0x50 => push_ascii_escape(b"\x1b[D"), 
            0x51 => push_ascii_escape(b"\x1b[B"), 
            0x52 => push_ascii_escape(b"\x1b[A"), 
            _ => {}
        }
    }
    let mut queue = ASCII_QUEUE.lock();
    for byte in bytes[..count].iter().copied() {
        queue.push(byte);
    }
}

fn translate_boot_report(
    prev_keys: &mut [u8; 6],
    caps_lock: &mut bool,
    report: &[u8; 8],
) -> ([u8; 6], usize) {
    let control = report[0] & (MOD_LCTRL | MOD_RCTRL) != 0;
    let shift = report[0] & (MOD_LSHIFT | MOD_RSHIFT) != 0;
    let keys = [
        report[2], report[3], report[4], report[5], report[6], report[7],
    ];
    let mut output = [0; 6];
    let mut count = 0;

    for &code in &keys {
        if code == 0 || prev_keys.contains(&code) {
            continue;
        }
        if code == HID_KEYCODE_CAPS_LOCK {
            *caps_lock = !*caps_lock;
            continue;
        }
        if (code as usize) >= HID_UNSHIFTED.len() {
            continue;
        }
        let layout = crate::input::keyboard_layout();
        let mut ascii = if shift {
            HID_SHIFTED[code as usize]
        } else {
            HID_UNSHIFTED[code as usize]
        };
        if layout == 1 {
            // German QWERTZ: letter swap and common punctuation positions.
            if code == 0x1c || code == 0x1d {
                let other = if code == 0x1c { 0x1d } else { 0x1c };
                ascii = if shift { HID_SHIFTED[other] } else { HID_UNSHIFTED[other] };
            } else {
                ascii = match (code, shift) {
                    (0x2d, false) => 0xdf, (0x2d, true) => b'?',
                    (0x2e, false) => b'\'', (0x2e, true) => b'`',
                    (0x2f, false) => b'[', (0x2f, true) => b'{',
                    (0x30, false) => b']', (0x30, true) => b'}',
                    (0x31, false) => b'#', (0x31, true) => b'\'',
                    (0x33, false) => b';', (0x33, true) => b':',
                    (0x34, false) => b'\'', (0x34, true) => b'"',
                    (0x35, false) => b'^', (0x35, true) => b'~',
                    _ => ascii,
                };
            }
        } else if layout == 2 {
            // Dutch QWERTY keeps the letters and uses the Dutch number row.
            if (0x1e..=0x27).contains(&code) && shift {
                ascii = *b"!\"#$%&/()=".get((code - 0x1e) as usize).unwrap_or(&ascii);
            } else if code == 0x34 {
                ascii = if shift { b'\'' } else { b';' };
            }
        }
        if ascii == 0 {
            continue;
        }
        if *caps_lock && ascii.is_ascii_lowercase() {
            ascii = ascii.to_ascii_uppercase();
        } else if *caps_lock && ascii.is_ascii_uppercase() && shift {
            ascii = ascii.to_ascii_lowercase();
        }
        if control && ascii.is_ascii_alphabetic() {
            ascii = ascii.to_ascii_uppercase() & 0x1F;
        }
        output[count] = ascii;
        count += 1;
    }
    *prev_keys = keys;
    (output, count)
}

pub fn self_test() {
    let mut prev = [0; 6];
    let mut caps = false;
    let mut report = [0; 8];

    report[2] = 0x04; 
    let (pressed, pressed_len) = translate_boot_report(&mut prev, &mut caps, &report);
    let (_, held_len) = translate_boot_report(&mut prev, &mut caps, &report);
    report[2] = 0; 
    let (_, released_len) = translate_boot_report(&mut prev, &mut caps, &report);
    report[0] = MOD_LSHIFT;
    report[2] = 0x04;
    let (shifted, shifted_len) = translate_boot_report(&mut prev, &mut caps, &report);
    report[0] = 0;
    report[2] = HID_KEYCODE_CAPS_LOCK;
    let (_, caps_len) = translate_boot_report(&mut prev, &mut caps, &report);
    report[2] = 0;
    let _ = translate_boot_report(&mut prev, &mut caps, &report);
    report[2] = 0x04;
    let (capitalized, capitalized_len) = translate_boot_report(&mut prev, &mut caps, &report);

    let ok = pressed_len == 1
        && pressed[0] == b'a'
        && held_len == 0
        && released_len == 0
        && shifted_len == 1
        && shifted[0] == b'A'
        && caps_len == 0
        && caps
        && capitalized_len == 1
        && capitalized[0] == b'A';
    if ok {
        log_ok!(
            "USB",
            "HIDSelfTest",
            "Boot-keyboard report translation passed"
        );
    } else {
        log_fail!(
            "USB",
            "HIDSelfTest",
            "Boot-keyboard report translation failed"
        );
    }
}

pub fn try_read_byte() -> Option<u8> {
    ASCII_QUEUE.lock().pop()
}

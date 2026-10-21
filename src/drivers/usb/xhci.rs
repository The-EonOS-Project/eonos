


























#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, Ordering};

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

const DESC_TYPE_DEVICE: u16 = 1 << 8;
const DESC_TYPE_CONFIGURATION: u16 = 2 << 8;
const HID_PROTOCOL_BOOT: u16 = 0;
const HID_PROTOCOL_KEYBOARD: u8 = 1;
const HID_PROTOCOL_MOUSE: u8 = 2;

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
            log_ok!(
                "USB",
                "xHCI",
                "Configured {} boot HID interface(s) on port {}",
                count,
                port
            );
            
            
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

    if controller.devices.is_empty() {
        log_ok!(
            "USB",
            "xHCI",
            "No supported USB HID device found on any port"
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
            if let Some(index) = c.devices.iter().position(|device| device.port == port) {
                let slot_id = c.devices[index].slot_id;
                let mut disconnected = alloc::vec::Vec::new();
                let mut index = 0;
                while index < c.devices.len() {
                    if c.devices[index].port == port && c.devices[index].slot_id == slot_id {
                        disconnected.push(c.devices.remove(index));
                    } else {
                        index += 1;
                    }
                }
                if !disable_hid_slot(c, slot_id, port) {
                    
                    
                    c.devices.extend(disconnected);
                    continue;
                }
                for device in disconnected {
                    if device.protocol == HID_PROTOCOL_MOUSE {
                        super::mouse::clear_buttons();
                    }
                    free_hid_device(device);
                }
                log_ok!(
                    "USB",
                    "xHCI",
                    "USB HID device disconnected from port {}",
                    port
                );
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
        let mut ascii = if shift {
            HID_SHIFTED[code as usize]
        } else {
            HID_UNSHIFTED[code as usize]
        };
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

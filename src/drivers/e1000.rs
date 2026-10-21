use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{fence, Ordering};
use spin::Mutex;

const RX_COUNT: usize = 16;
const TX_COUNT: usize = 16;
const MAX_FRAME: usize = 1518;
const CF8: u16 = 0xcf8;
const CFC: u16 = 0xcfc;

const REG_CTRL: usize = 0x0000;
const REG_STATUS: usize = 0x0008;
const REG_IMC: usize = 0x00d8;
const REG_RCTL: usize = 0x0100;
const REG_TCTL: usize = 0x0400;
const REG_TIPG: usize = 0x0410;
const REG_RAL: usize = 0x5400;
const REG_RAH: usize = 0x5404;
const REG_RDBAL: usize = 0x2800;
const REG_RDBAH: usize = 0x2804;
const REG_RDLEN: usize = 0x2808;
const REG_RDH: usize = 0x2810;
const REG_RDT: usize = 0x2818;
const REG_TDBAL: usize = 0x3800;
const REG_TDBAH: usize = 0x3804;
const REG_TDLEN: usize = 0x3808;
const REG_TDH: usize = 0x3810;
const REG_TDT: usize = 0x3818;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RxDescriptor {
    address: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TxDescriptor {
    address: u64,
    length: u16,
    cso: u8,
    command: u8,
    status: u8,
    css: u8,
    special: u16,
}

struct DmaPage {
    physical: u64,
    virtual_address: *mut u8,
}
unsafe impl Send for DmaPage {}

impl DmaPage {
    fn allocate() -> Option<Self> {
        let physical = crate::pmm::alloc_contiguous(1)?;
        let virtual_address = crate::pmm::phys_to_virt(physical);
        unsafe { core::ptr::write_bytes(virtual_address, 0, 4096) };
        Some(Self { physical, virtual_address })
    }
}

impl Drop for DmaPage {
    fn drop(&mut self) {
        crate::pmm::free_frames(self.physical, 1);
    }
}

unsafe impl Send for E1000 {}
struct E1000 {
    registers: *mut u8,
    mac: [u8; 6],
    rx_ring: *mut RxDescriptor,
    tx_ring: *mut TxDescriptor,
    _rx_ring_page: DmaPage,
    _tx_ring_page: DmaPage,
    rx_buffers: Vec<DmaPage>,
    tx_buffers: Vec<DmaPage>,
    rx_next: usize,
    tx_next: usize,
    received: u64,
    transmitted: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NetworkInfo {
    pub mac: [u8; 6],
    pub link_up: u8,
    pub _reserved: u8,
    pub received: u64,
    pub transmitted: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum Error {
    NoDevice,
    LinkDown,
    Busy,
    TxBusy,
    Timeout,
    ArpTimeout,
    InvalidFrame,
    TcpReset,
    TcpNotConnected,
}

static DEVICE: Mutex<Option<E1000>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct Location { bus: u8, device: u8, function: u8 }

unsafe fn outl(port: u16, value: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
}
unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags));
    value
}
unsafe fn outw(port: u16, value: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
}
fn config_address(loc: Location, offset: u8) -> u32 {
    (1 << 31) | ((loc.bus as u32) << 16) | ((loc.device as u32) << 11)
        | ((loc.function as u32) << 8) | (offset as u32 & 0xfc)
}
fn pci_read(loc: Location, offset: u8) -> u32 {
    unsafe { outl(CF8, config_address(loc, offset)); inl(CFC) }
}
fn pci_enable_dma(loc: Location) -> bool {
    let command = (pci_read(loc, 4) as u16) | 0x0006;
    unsafe { outl(CF8, config_address(loc, 4)); outw(CFC, command); }
    pci_read(loc, 4) as u16 & 0x0006 == 0x0006
}

fn find_device() -> Option<(Location, u64, u16)> {
    for bus in 0..=255u16 {
        for device in 0..32u8 {
            let first = Location { bus: bus as u8, device, function: 0 };
            if pci_read(first, 0) as u16 == 0xffff { continue; }
            let functions = if pci_read(first, 0x0c) & (1 << 23) != 0 { 8 } else { 1 };
            for function in 0..functions {
                let loc = Location { function, ..first };
                let id = pci_read(loc, 0);
                let vendor = id as u16;
                let product = (id >> 16) as u16;
                if vendor != 0x8086 || !matches!(product, 0x100e | 0x100f | 0x10d3) { continue; }
                let class = pci_read(loc, 8) >> 8 & 0x00ff_ffff;
                if class != 0x020000 { continue; }
                let bar0 = pci_read(loc, 0x10);
                if bar0 & 1 != 0 { continue; }
                let base = (bar0 as u64) & !0xf;
                let is_64 = (bar0 >> 1) & 3 == 2;
                let base = if is_64 { base | ((pci_read(loc, 0x14) as u64) << 32) } else { base };
                if base != 0 { return Some((loc, base, product)); }
            }
        }
    }
    None
}

fn reg_read(base: *mut u8, offset: usize) -> u32 {
    unsafe { read_volatile(base.add(offset) as *const u32) }
}
fn reg_write(base: *mut u8, offset: usize, value: u32) {
    unsafe { write_volatile(base.add(offset) as *mut u32, value) }
}

pub fn init() {
    let Some((location, physical_bar, product)) = find_device() else {
        crate::log_ok!("Ethernet", "Init", "No supported Intel E1000 NIC found");
        return;
    };
    if !pci_enable_dma(location) {
        crate::log_fail!("Ethernet", "Init", "Could not enable PCI memory decoding and bus mastering");
        return;
    }
    let registers = crate::vmm::map_mmio(physical_bar, 0x6000);
    reg_write(registers, REG_IMC, 0xffff_ffff);
    let control = reg_read(registers, REG_CTRL);
    reg_write(registers, REG_CTRL, control | (1 << 26));
    for _ in 0..1_000_000 {
        if reg_read(registers, REG_CTRL) & (1 << 26) == 0 { break; }
        core::hint::spin_loop();
    }
    reg_write(registers, REG_IMC, 0xffff_ffff);
    let _ = reg_read(registers, 0x00c0); 

    let Some(rx_ring_page) = DmaPage::allocate() else { crate::log_fail!("Ethernet", "Init", "DMA memory unavailable"); return; };
    let Some(tx_ring_page) = DmaPage::allocate() else { crate::log_fail!("Ethernet", "Init", "DMA memory unavailable"); return; };
    let rx_ring = rx_ring_page.virtual_address as *mut RxDescriptor;
    let tx_ring = tx_ring_page.virtual_address as *mut TxDescriptor;
    let mut rx_buffers = Vec::with_capacity(RX_COUNT);
    let mut tx_buffers = Vec::with_capacity(TX_COUNT);
    for index in 0..RX_COUNT {
        let Some(buffer) = DmaPage::allocate() else { crate::log_fail!("Ethernet", "Init", "DMA memory unavailable"); return; };
        unsafe { write_volatile(core::ptr::addr_of_mut!((*rx_ring.add(index)).address), buffer.physical); }
        rx_buffers.push(buffer);
    }
    for index in 0..TX_COUNT {
        let Some(buffer) = DmaPage::allocate() else { crate::log_fail!("Ethernet", "Init", "DMA memory unavailable"); return; };
        unsafe {
            write_volatile(core::ptr::addr_of_mut!((*tx_ring.add(index)).address), buffer.physical);
            write_volatile(core::ptr::addr_of_mut!((*tx_ring.add(index)).status), 1);
        }
        tx_buffers.push(buffer);
    }

    reg_write(registers, REG_RDBAL, rx_ring_page.physical as u32);
    reg_write(registers, REG_RDBAH, (rx_ring_page.physical >> 32) as u32);
    reg_write(registers, REG_RDLEN, (RX_COUNT * core::mem::size_of::<RxDescriptor>()) as u32);
    reg_write(registers, REG_RDH, 0);
    reg_write(registers, REG_RDT, (RX_COUNT - 1) as u32);
    reg_write(registers, REG_TDBAL, tx_ring_page.physical as u32);
    reg_write(registers, REG_TDBAH, (tx_ring_page.physical >> 32) as u32);
    reg_write(registers, REG_TDLEN, (TX_COUNT * core::mem::size_of::<TxDescriptor>()) as u32);
    reg_write(registers, REG_TDH, 0);
    reg_write(registers, REG_TDT, 0);
    reg_write(registers, REG_TIPG, 0x0060_200a);
    reg_write(registers, REG_TCTL, (1 << 1) | (1 << 3) | (0x10 << 4) | (0x40 << 12));
    reg_write(registers, REG_RCTL, (1 << 1) | (1 << 15) | (1 << 26));

    let low = reg_read(registers, REG_RAL);
    let high = reg_read(registers, REG_RAH);
    let mac = [low as u8, (low >> 8) as u8, (low >> 16) as u8, (low >> 24) as u8,
        high as u8, (high >> 8) as u8];
    if high & (1 << 31) == 0 || mac == [0; 6] {
        reg_write(registers, REG_RCTL, 0);
        reg_write(registers, REG_TCTL, 0);
        crate::log_fail!("Ethernet", "Init", "NIC has no valid station MAC address");
        return;
    }
    fence(Ordering::SeqCst);
    *DEVICE.lock() = Some(E1000 {
        registers, mac, rx_ring, tx_ring, _rx_ring_page: rx_ring_page, _tx_ring_page: tx_ring_page,
        rx_buffers, tx_buffers, rx_next: 0, tx_next: 0, received: 0, transmitted: 0,
    });
    let link = if reg_read(registers, REG_STATUS) & 2 != 0 { "up" } else { "down" };
    crate::log_ok!("Ethernet", "Init", "Intel E1000 {:04x} at {:02x}:{:02x}.{} MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, link {} (polling)",
        product, location.bus, location.device, location.function,
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], link);
}

pub fn info() -> Result<NetworkInfo, Error> {
    let device = DEVICE.lock();
    let device = device.as_ref().ok_or(Error::NoDevice)?;
    Ok(NetworkInfo {
        mac: device.mac,
        link_up: ((reg_read(device.registers, REG_STATUS) & 2 != 0) as u8),
        _reserved: 0,
        received: device.received,
        transmitted: device.transmitted,
    })
}

pub fn transmit(frame: &[u8]) -> Result<(), Error> {
    if !(14..=MAX_FRAME).contains(&frame.len()) { return Err(Error::InvalidFrame); }
    let mut guard = DEVICE.lock();
    let device = guard.as_mut().ok_or(Error::NoDevice)?;
    if reg_read(device.registers, REG_STATUS) & 2 == 0 { return Err(Error::LinkDown); }
    let index = device.tx_next;
    let status = unsafe { read_volatile(core::ptr::addr_of!((*device.tx_ring.add(index)).status)) };
    if status & 1 == 0 { return Err(Error::TxBusy); }
    unsafe {
        core::ptr::copy_nonoverlapping(frame.as_ptr(), device.tx_buffers[index].virtual_address, frame.len());
        write_volatile(core::ptr::addr_of_mut!((*device.tx_ring.add(index)).length), frame.len() as u16);
        write_volatile(core::ptr::addr_of_mut!((*device.tx_ring.add(index)).command), 0x0b); 
        write_volatile(core::ptr::addr_of_mut!((*device.tx_ring.add(index)).status), 0);
    }
    fence(Ordering::Release);
    device.tx_next = (index + 1) % TX_COUNT;
    reg_write(device.registers, REG_TDT, device.tx_next as u32);
    device.transmitted += 1;
    Ok(())
}

pub fn receive(output: &mut [u8]) -> Result<Option<usize>, Error> {
    let mut guard = DEVICE.lock();
    let device = guard.as_mut().ok_or(Error::NoDevice)?;
    let index = device.rx_next;
    let descriptor = unsafe { device.rx_ring.add(index) };
    let status = unsafe { read_volatile(core::ptr::addr_of!((*descriptor).status)) };
    if status & 1 == 0 { return Ok(None); }
    let length = unsafe { read_volatile(core::ptr::addr_of!((*descriptor).length)) as usize };
    if length > output.len() || length > MAX_FRAME {
        unsafe { write_volatile(core::ptr::addr_of_mut!((*descriptor).status), 0); }
        device.rx_next = (index + 1) % RX_COUNT;
        reg_write(device.registers, REG_RDT, index as u32);
        return Err(Error::InvalidFrame);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(device.rx_buffers[index].virtual_address, output.as_mut_ptr(), length);
        write_volatile(core::ptr::addr_of_mut!((*descriptor).length), 0);
        write_volatile(core::ptr::addr_of_mut!((*descriptor).status), 0);
    }
    device.rx_next = (index + 1) % RX_COUNT;
    reg_write(device.registers, REG_RDT, index as u32);
    device.received += 1;
    Ok(Some(length))
}

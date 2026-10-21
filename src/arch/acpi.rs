#![allow(dead_code)]

use crate::vmm;
use crate::{log_debug, log_fail, log_ok};
use limine::request::RsdpRequest;

fn map_table(phys: u64, len: u64) -> *const u8 {
    vmm::map_mmio(phys, len) as *const u8
}

#[used]
#[link_section = ".requests"]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[repr(C, packed)]
struct RsdpV1 {
    signature: [u8; 8], 
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[repr(C, packed)]
struct RsdpV2 {
    v1: RsdpV1,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    _reserved: [u8; 3],
}

#[repr(C, packed)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

fn checksum_ok(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b)) == 0
}

pub struct MadtInfo {
    pub local_apic_addr: u64,
    pub ioapics: [IoApicInfo; MAX_IOAPICS],
    pub ioapic_count: usize,
    pub overrides: [IsaOverride; MAX_OVERRIDES],
    pub override_count: usize,
}

const MAX_IOAPICS: usize = 4;
const MAX_OVERRIDES: usize = 16;

#[derive(Clone, Copy)]
pub struct IoApicInfo {
    pub id: u8,
    pub mmio_base: u32,
    pub gsi_base: u32,
}

#[derive(Clone, Copy)]
pub struct IsaOverride {
    pub isa_irq: u8,
    pub gsi: u32,

    pub flags: u16,
}

impl MadtInfo {

    pub fn override_for(&self, irq: u8) -> Option<(u32, u16)> {
        self.overrides[..self.override_count]
            .iter()
            .find(|o| o.isa_irq == irq)
            .map(|o| (o.gsi, o.flags))
    }
}

pub fn find_madt() -> Option<MadtInfo> {
    let Some(madt_addr) = find_table_address(b"APIC") else {
        log_fail!("ACPI", "Init", "No MADT (APIC table) found");
        return None;
    };
    parse_madt(madt_addr)
}

fn find_table_address(signature: &[u8; 4]) -> Option<u64> {
    let Some(response) = RSDP_REQUEST.get_response() else {
        log_fail!("ACPI", "Init", "Limine did not return an RSDP response");
        return None;
    };
    let rsdp_addr = response.address() as u64;
    if rsdp_addr == 0 {
        log_fail!("ACPI", "Init", "Limine gave no RSDP address");
        return None;
    }

    let rsdp_base = map_table(rsdp_addr, 4096);
    let v1 = unsafe { &*(rsdp_base as *const RsdpV1) };
    if &v1.signature != b"RSD PTR " {
        log_fail!("ACPI", "Init", "RSDP signature mismatch");
        return None;
    }

    let mut sdt_addr = v1.rsdt_address as u64;
    let mut sdt_is_xsdt = false;
    if v1.revision >= 2 {
        let v2 = unsafe { &*(rsdp_base as *const RsdpV2) };
        let len = v2.length as usize;
        let v2_ok = (core::mem::size_of::<RsdpV2>()..=4096).contains(&len)
            && checksum_ok(unsafe { core::slice::from_raw_parts(rsdp_base, len) });
        if v2_ok && v2.xsdt_address != 0 {
            sdt_addr = v2.xsdt_address;
            sdt_is_xsdt = true;
        } else {
            let v1_bytes =
                unsafe { core::slice::from_raw_parts(rsdp_base, core::mem::size_of::<RsdpV1>()) };
            if !checksum_ok(v1_bytes) {
                log_fail!("ACPI", "Init", "RSDP v1 checksum failed");
                return None;
            }
            log_debug!("ACPI", "Init", "RSDP v2 checksum failed; falling back to RSDT");
        }
    } else {
        let v1_bytes =
            unsafe { core::slice::from_raw_parts(rsdp_base, core::mem::size_of::<RsdpV1>()) };
        if !checksum_ok(v1_bytes) {
            log_fail!("ACPI", "Init", "RSDP v1 checksum failed");
            return None;
        }
    }

    if sdt_addr == 0 {
        log_fail!("ACPI", "Init", "No usable RSDT/XSDT address");
        return None;
    }

    let sdt_base = map_table(sdt_addr, 0x10000);
    let sdt_header = unsafe { &*(sdt_base as *const SdtHeader) };
    let sdt_len = sdt_header.length as usize;
    let expected_sig = if sdt_is_xsdt { b"XSDT" } else { b"RSDT" };
    if &sdt_header.signature != expected_sig
        || sdt_len < core::mem::size_of::<SdtHeader>()
        || sdt_len > 0x10000
        || (sdt_len - core::mem::size_of::<SdtHeader>()) % if sdt_is_xsdt { 8 } else { 4 } != 0
    {
        log_fail!("ACPI", "Init", "Root table has an invalid signature or length");
        return None;
    }
    let sdt_bytes = unsafe { core::slice::from_raw_parts(sdt_base, sdt_len) };
    if !checksum_ok(sdt_bytes) {
        log_fail!(
            "ACPI",
            "Init",
            "{} checksum failed",
            if sdt_is_xsdt { "XSDT" } else { "RSDT" }
        );
        return None;
    }

    let entries_start = core::mem::size_of::<SdtHeader>();
    let entry_size = if sdt_is_xsdt { 8 } else { 4 };
    let mut offset = entries_start;
    while offset < sdt_len {

        let addr = unsafe {
            if sdt_is_xsdt {
                core::ptr::read_unaligned(sdt_base.add(offset) as *const u64)
            } else {
                core::ptr::read_unaligned(sdt_base.add(offset) as *const u32) as u64
            }
        };
        if table_signature_is(addr, signature) {
            log_debug!("ACPI", "Table", "Found {:?} at {:#x} via {}", signature, addr, if sdt_is_xsdt { "XSDT" } else { "RSDT" });
            return Some(addr);
        }
        offset += entry_size;
    }

    if sdt_is_xsdt {
        let rsdt_addr = unsafe {
            core::ptr::read_unaligned(rsdp_base.add(16) as *const u32) as u64
        };
        if rsdt_addr != 0 {
            let rsdt = map_table(rsdt_addr, 0x10000);
            let header = unsafe { &*(rsdt as *const SdtHeader) };
            let len = header.length as usize;
            if &header.signature == b"RSDT"
                && (core::mem::size_of::<SdtHeader>()..=0x10000).contains(&len)
                && (len - core::mem::size_of::<SdtHeader>()) % 4 == 0
                && checksum_ok(unsafe { core::slice::from_raw_parts(rsdt, len) })
            {
                let mut offset = core::mem::size_of::<SdtHeader>();
                while offset < len {
                    let addr = unsafe {
                        core::ptr::read_unaligned(rsdt.add(offset) as *const u32) as u64
                    };
                    if table_signature_is(addr, signature) {
                        log_debug!("ACPI", "Table", "Found {:?} at {:#x} via RSDT fallback", signature, addr);
                        return Some(addr);
                    }
                    offset += 4;
                }
            } else {
                log_debug!("ACPI", "Table", "RSDT fallback has invalid signature, length, or checksum");
            }
        }
    }
    log_debug!("ACPI", "Init", "ACPI table {:?} not found", signature);
    None
}

pub fn shutdown() -> bool {
    let Some(fadt_addr) = find_table_address(b"FACP") else {
        log_fail!("ACPI", "PowerOff", "FADT (FACP) table not found");
        return false;
    };
    let fadt = map_table(fadt_addr, 4096);
    let fadt_header = unsafe { &*(fadt as *const SdtHeader) };
    let fadt_len = fadt_header.length as usize;
    if fadt_len < 90 || fadt_len > 0x10000 {
        log_fail!("ACPI", "PowerOff", "FADT has unsupported length {}", fadt_len);
        return false;
    }
    let fadt = map_table(fadt_addr, fadt_len as u64);
    let fadt_bytes = unsafe { core::slice::from_raw_parts(fadt, fadt_len) };
    if &fadt_header.signature != b"FACP" || !checksum_ok(fadt_bytes) {
        log_fail!("ACPI", "PowerOff", "FADT signature or checksum is invalid");
        return false;
    }

    let dsdt_legacy = unsafe { core::ptr::read_unaligned(fadt.add(40) as *const u32) } as u64;
    let dsdt_extended = if fadt_len >= 148 {
        unsafe { core::ptr::read_unaligned(fadt.add(140) as *const u64) }
    } else {
        0
    };
    let dsdt_addr = if dsdt_extended != 0 { dsdt_extended } else { dsdt_legacy };
    if dsdt_addr == 0 {
        log_fail!("ACPI", "PowerOff", "FADT has no DSDT address");
        return false;
    }
    let dsdt_header = map_table(dsdt_addr, 4096);
    let dsdt_len = unsafe { core::ptr::read_unaligned(dsdt_header.add(4) as *const u32) } as usize;
    if !(core::mem::size_of::<SdtHeader>()..=0x10_0000).contains(&dsdt_len) {
        log_fail!("ACPI", "PowerOff", "DSDT has invalid length {}", dsdt_len);
        return false;
    }
    let dsdt = map_table(dsdt_addr, dsdt_len as u64);
    let dsdt_bytes = unsafe { core::slice::from_raw_parts(dsdt, dsdt_len) };
    let dsdt_sdt = unsafe { &*(dsdt as *const SdtHeader) };
    if &dsdt_sdt.signature != b"DSDT" || !checksum_ok(dsdt_bytes) {
        log_fail!("ACPI", "PowerOff", "DSDT signature or checksum is invalid");
        return false;
    }
    let Some((sleep_type_a, sleep_type_b)) = find_s5_sleep_types(&dsdt_bytes[36..]) else {
        log_fail!("ACPI", "PowerOff", "DSDT has no supported _S5_ sleep package");
        return false;
    };

    let pm1_len = unsafe { core::ptr::read_volatile(fadt.add(89)) };
    if pm1_len < 2 {
        log_fail!("ACPI", "PowerOff", "FADT PM1 control block is too short");
        return false;
    }
    let pm1a = fadt_io_port(fadt, fadt_len, 172, 64);
    let pm1b = fadt_io_port(fadt, fadt_len, 184, 68);
    let Some(pm1a) = pm1a else {
        log_fail!("ACPI", "PowerOff", "FADT has no usable PM1a control register");
        return false;
    };

    let sci_enabled = unsafe { inw(pm1a) & 1 != 0 };
    if !sci_enabled && fadt_len > 53 {
        let smi_command = unsafe { core::ptr::read_unaligned(fadt.add(48) as *const u32) };
        let acpi_enable = unsafe { core::ptr::read_volatile(fadt.add(52)) };
        if smi_command != 0 && acpi_enable != 0 && smi_command <= u16::MAX as u32 {
            unsafe { outb(smi_command as u16, acpi_enable) };
            for _ in 0..1_000_000 {
                if unsafe { inw(pm1a) } & 1 != 0 {
                    break;
                }
                core::hint::spin_loop();
            }
        }
    }
    if unsafe { inw(pm1a) } & 1 == 0 {
        log_fail!("ACPI", "PowerOff", "ACPI mode did not become enabled");
        return false;
    }

    log_ok!("ACPI", "PowerOff", "Entering S5 via PM1a port {:#x}", pm1a);
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    unsafe {
        write_sleep_control(pm1a, sleep_type_a);
        if let Some(pm1b) = pm1b {
            write_sleep_control(pm1b, sleep_type_b);
        }
    }
    true
}

pub fn reset() -> bool {
    let Some(fadt_addr) = find_table_address(b"FACP") else {
        return false;
    };
    let header = map_table(fadt_addr, 4096);
    let fadt_len = unsafe { core::ptr::read_unaligned(header.add(4) as *const u32) } as usize;
    if fadt_len < 129 || fadt_len > 0x10000 {
        return false;
    }
    let fadt = map_table(fadt_addr, fadt_len as u64);
    let bytes = unsafe { core::slice::from_raw_parts(fadt, fadt_len) };
    if !checksum_ok(bytes) {
        return false;
    }
    let flags = unsafe { core::ptr::read_unaligned(fadt.add(112) as *const u32) };
    if flags & (1 << 10) == 0 {
        return false;
    }
    let gas = unsafe { core::slice::from_raw_parts(fadt.add(116), 12) };
    let address_space = gas[0];
    let bit_width = gas[1];
    let address = unsafe { core::ptr::read_unaligned(fadt.add(120) as *const u64) };
    let value = unsafe { core::ptr::read_volatile(fadt.add(128)) };
    match (address_space, bit_width) {
        (1, 8) if address <= u16::MAX as u64 => unsafe { outb(address as u16, value) },
        (0, 8) if address != 0 => unsafe { (map_table(address, 1) as *mut u8).write_volatile(value) },
        _ => return false,
    }
    true
}

fn fadt_io_port(fadt: *const u8, length: usize, extended: usize, legacy: usize) -> Option<u16> {
    if length >= extended + 12 {
        let gas = unsafe { core::slice::from_raw_parts(fadt.add(extended), 12) };
        let address = unsafe { core::ptr::read_unaligned(fadt.add(extended + 4) as *const u64) };
        if gas[0] == 1 && gas[1] >= 16 && address != 0 && address <= u16::MAX as u64 - 1 {
            return Some(address as u16);
        }
    }
    let address = unsafe { core::ptr::read_unaligned(fadt.add(legacy) as *const u32) };
    (address != 0 && address <= u16::MAX as u32 - 1).then_some(address as u16)
}

fn find_s5_sleep_types(aml: &[u8]) -> Option<(u16, u16)> {
    for start in 0..aml.len().saturating_sub(6) {
        if aml[start] != 0x08 { 
            continue;
        }
        let mut name = start + 1;
        if aml.get(name) == Some(&0x5c) { 
            name += 1;
        }
        if aml.get(name..name + 4) != Some(b"_S5_") || aml.get(name + 4) != Some(&0x12) {
            continue;
        }
        let length_at = name + 5;
        let Some((package_end, body)) = aml_package_bounds(aml, length_at) else {
            continue;
        };
        if body >= package_end || aml[body] < 2 {
            continue;
        }
        let Some((type_a, after_a)) = aml_integer(aml, body + 1, package_end) else {
            continue;
        };
        let Some((type_b, _)) = aml_integer(aml, after_a, package_end) else {
            continue;
        };
        if type_a <= 7 && type_b <= 7 {
            return Some((type_a as u16, type_b as u16));
        }
    }
    None
}

fn aml_package_bounds(aml: &[u8], length_at: usize) -> Option<(usize, usize)> {
    let lead = *aml.get(length_at)?;
    let following = (lead >> 6) as usize;
    if length_at + following >= aml.len() {
        return None;
    }
    let mut length = if following == 0 { (lead & 0x3f) as usize } else { (lead & 0x0f) as usize };
    for index in 0..following {
        length |= (*aml.get(length_at + 1 + index)? as usize) << (4 + index * 8);
    }
    let body = length_at + 1 + following;
    let end = length_at.checked_add(length)?;
    (end <= aml.len() && body <= end).then_some((end, body))
}

fn aml_integer(aml: &[u8], at: usize, end: usize) -> Option<(u64, usize)> {
    let opcode = *aml.get(at)?;
    let (bytes, value) = match opcode {
        0x00 => return Some((0, at + 1)), 
        0x01 => return Some((1, at + 1)), 
        0xff => return Some((u64::MAX, at + 1)), 
        0x0a => (1, at + 1),
        0x0b => (2, at + 1),
        0x0c => (4, at + 1),
        0x0e => (8, at + 1),
        _ => return None,
    };
    let next = value.checked_add(bytes)?;
    if next > end {
        return None;
    }
    let mut result = 0u64;
    for index in 0..bytes {
        result |= (*aml.get(value + index)? as u64) << (index * 8);
    }
    Some((result, next))
}

unsafe fn write_sleep_control(port: u16, sleep_type: u16) {
    let current = inw(port);
    let value = (current & !((7 << 10) | (1 << 13))) | ((sleep_type & 7) << 10) | (1 << 13);
    outw(port, value);
}

unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    core::arch::asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags));
    value
}

unsafe fn outw(port: u16, value: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
}

unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

fn table_signature_is(phys_addr: u64, sig: &[u8; 4]) -> bool {
    if phys_addr == 0 {
        return false;
    }

    let base = map_table(phys_addr, 4096);
    let header = unsafe { &*(base as *const SdtHeader) };
    &header.signature == sig
}

#[repr(C, packed)]
struct MadtHeader {
    sdt: SdtHeader,
    local_apic_addr: u32,
    flags: u32,

}

const MADT_ENTRY_IOAPIC: u8 = 1;
const MADT_ENTRY_ISA_OVERRIDE: u8 = 2;
const MADT_ENTRY_LOCAL_APIC_OVERRIDE: u8 = 5;

fn parse_madt(madt_addr: u64) -> Option<MadtInfo> {

    let base = map_table(madt_addr, 0x10000);
    let header = unsafe { &*(base as *const MadtHeader) };
    let total_len = header.sdt.length as usize;
    if &header.sdt.signature != b"APIC"
        || total_len < core::mem::size_of::<MadtHeader>()
        || total_len > 0x10000
    {
        log_fail!("ACPI", "Init", "MADT has an invalid signature or length");
        return None;
    }
    let sdt_bytes = unsafe { core::slice::from_raw_parts(base, total_len) };
    if !checksum_ok(sdt_bytes) {
        log_fail!(
            "ACPI",
            "Init",
            "MADT checksum failed -- refusing to trust it"
        );
        return None;
    }

    let mut info = MadtInfo {
        local_apic_addr: header.local_apic_addr as u64,
        ioapics: [IoApicInfo {
            id: 0,
            mmio_base: 0,
            gsi_base: 0,
        }; MAX_IOAPICS],
        ioapic_count: 0,
        overrides: [IsaOverride {
            isa_irq: 0,
            gsi: 0,
            flags: 0,
        }; MAX_OVERRIDES],
        override_count: 0,
    };

    let entries_start = core::mem::size_of::<MadtHeader>();
    let mut offset = entries_start;
    while offset + 2 <= total_len {
        let entry_type = unsafe { core::ptr::read_volatile(base.add(offset)) };
        let entry_len = unsafe { core::ptr::read_volatile(base.add(offset + 1)) } as usize;
        if entry_len < 2 || offset + entry_len > total_len {
            log_fail!("ACPI", "Init", "MADT contains a malformed entry");
            return None;
        }

        match entry_type {
            MADT_ENTRY_IOAPIC if entry_len >= 12 => {
                if info.ioapic_count < MAX_IOAPICS {
                    let id = unsafe { core::ptr::read_volatile(base.add(offset + 2)) };
                    let mmio_base =
                        unsafe { core::ptr::read_unaligned(base.add(offset + 4) as *const u32) };
                    let gsi_base =
                        unsafe { core::ptr::read_unaligned(base.add(offset + 8) as *const u32) };
                    info.ioapics[info.ioapic_count] = IoApicInfo {
                        id,
                        mmio_base,
                        gsi_base,
                    };
                    info.ioapic_count += 1;
                }
            }
            MADT_ENTRY_ISA_OVERRIDE if entry_len >= 10 => {
                if info.override_count < MAX_OVERRIDES {
                    let isa_irq = unsafe { core::ptr::read_volatile(base.add(offset + 3)) };
                    let gsi =
                        unsafe { core::ptr::read_unaligned(base.add(offset + 4) as *const u32) };
                    let flags =
                        unsafe { core::ptr::read_unaligned(base.add(offset + 8) as *const u16) };
                    info.overrides[info.override_count] = IsaOverride {
                        isa_irq,
                        gsi,
                        flags,
                    };
                    info.override_count += 1;
                }
            }
            MADT_ENTRY_LOCAL_APIC_OVERRIDE if entry_len >= 12 => {

                let addr64 =
                    unsafe { core::ptr::read_unaligned(base.add(offset + 4) as *const u64) };
                if addr64 != 0 {
                    info.local_apic_addr = addr64;
                }
            }
            _ => {}
        }

        offset += entry_len;
    }

    if info.ioapic_count == 0 {
        log_fail!("ACPI", "Init", "MADT parsed but no I/O APIC entries found");
        return None;
    }

    log_ok!(
        "ACPI",
        "Init",
        "MADT parsed: Local APIC at {:#x}, {} I/O APIC(s), {} ISA override(s)",
        info.local_apic_addr,
        info.ioapic_count,
        info.override_count
    );

    Some(info)
}

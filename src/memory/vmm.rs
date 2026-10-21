#![allow(dead_code)]

use core::arch::asm;
use core::fmt;
use core::ptr::addr_of;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::pmm::{self, PAGE_SIZE};
use crate::sync::IrqMutex;
use crate::{log_debug, log_fail, log_ok};
use limine::request::ExecutableAddressRequest;

const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const HUGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;

pub const KERNEL_RX: u64 = PRESENT;
pub const KERNEL_RO: u64 = PRESENT | NO_EXECUTE;
pub const KERNEL_RW: u64 = PRESENT | WRITABLE | NO_EXECUTE;

pub const USER_RX: u64 = PRESENT | USER;
pub const USER_RO: u64 = PRESENT | USER | NO_EXECUTE;
pub const USER_RW: u64 = PRESENT | WRITABLE | USER | NO_EXECUTE;

pub const KERNEL_WC: u64 = PRESENT | WRITABLE | NO_EXECUTE | PWT;

pub const KERNEL_UC: u64 = PRESENT | WRITABLE | NO_EXECUTE | PCD;

const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const ENTRIES: usize = 512;
const HUGE_2M: u64 = 0x20_0000;
const HUGE_1G: u64 = 0x4000_0000;

const HUGE_2M_MASK: u64 = 0x000f_ffff_ffe0_0000;
const HUGE_1G_MASK: u64 = 0x000f_ffff_c000_0000;
const PAT_HUGE: u64 = 1 << 12;
const PAT_4K: u64 = 1 << 7;

const SELF_TEST_VIRT: u64 = 0xffff_ffff_c000_0000;

const SELF_TEST_USER_VIRT: u64 = 0x0000_0000_0010_0000;

const KERNEL_PML4_START: usize = 256;

extern "C" {
    static __kernel_text_start: u8;
    static __kernel_text_end: u8;
    static __kernel_rodata_start: u8;
    static __kernel_rodata_end: u8;
    static __kernel_data_start: u8;
    static __kernel_data_end: u8;
}

#[used]
#[link_section = ".requests"]
static KERNEL_ADDRESS_REQUEST: ExecutableAddressRequest = ExecutableAddressRequest::new();

#[repr(C, align(4096))]
struct Table([u64; ENTRIES]);

impl Table {
    const ZERO: Table = Table([0; ENTRIES]);
}

struct Vmm {
    pml4_phys: u64,
}

unsafe impl Send for Vmm {}

static VMM: IrqMutex<Option<Vmm>> = IrqMutex::new(None);

static KERNEL_CR3: AtomicU64 = AtomicU64::new(0);

pub const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;

struct Size(u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.0 >= (1 << 20) {
            write!(f, "{} MiB", self.0 >> 20)
        } else {
            write!(f, "{} KiB", self.0 >> 10)
        }
    }
}

const fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) & !(a - 1)
}

const fn align_down(v: u64, a: u64) -> u64 {
    v & !(a - 1)
}

fn table_ptr(phys: u64) -> *mut Table {
    pmm::phys_to_virt(phys) as *mut Table
}

fn index(virt: u64, level: usize) -> usize {
    ((virt >> (39 - level * 9)) & 0x1FF) as usize
}

unsafe fn child_table(parent: *mut Table, idx: usize) -> *mut Table {
    let entry = (*parent).0[idx];
    if entry & PRESENT != 0 {
        assert!(
            entry & HUGE == 0,
            "VMM: unexpected 1 GiB page while walking the page tables"
        );
        return table_ptr(entry & ADDR_MASK);
    }
    let phys = pmm::alloc_frame_zeroed().expect("VMM: out of memory for page tables");
    (*parent).0[idx] = phys | PRESENT | WRITABLE | USER;
    table_ptr(phys)
}

unsafe fn split_huge(pd: *mut Table, idx: usize) {
    let entry = (*pd).0[idx];
    let base = entry & HUGE_2M_MASK;

    let mut flags = entry & !ADDR_MASK & !HUGE;
    if entry & PAT_HUGE != 0 {
        flags |= PAT_4K;
    }

    let phys = pmm::alloc_frame_zeroed().expect("VMM: out of memory splitting a 2 MiB page");
    let pt = table_ptr(phys);
    for i in 0..ENTRIES {
        (*pt).0[i] = (base + i as u64 * PAGE_SIZE) | flags;
    }
    (*pd).0[idx] = phys | PRESENT | WRITABLE | USER;
}

unsafe fn split_1g_huge(pdpt: *mut Table, idx: usize) {
    let entry = (*pdpt).0[idx];
    let base = entry & HUGE_1G_MASK;
    let mut flags = entry & !ADDR_MASK & !HUGE;
    if entry & PAT_HUGE != 0 {
        flags |= PAT_HUGE;
    }

    let phys = pmm::alloc_frame_zeroed().expect("VMM: out of memory splitting a 1 GiB page");
    let pd = table_ptr(phys);
    for i in 0..ENTRIES {
        (*pd).0[i] = (base + i as u64 * HUGE_2M) | flags | PRESENT | HUGE;
    }
    (*pdpt).0[idx] = phys | PRESENT | WRITABLE | USER;
}

unsafe fn pd_for(pdpt: *mut Table, virt: u64) -> *mut Table {
    let idx = index(virt, 1);
    let entry = (*pdpt).0[idx];
    if entry & PRESENT != 0 && entry & HUGE != 0 {
        split_1g_huge(pdpt, idx);
        invlpg(virt);
    }
    child_table(pdpt, idx)
}

unsafe fn pt_for(pd: *mut Table, virt: u64) -> *mut Table {
    let idx = index(virt, 2);
    let entry = (*pd).0[idx];
    if entry & PRESENT != 0 && entry & HUGE != 0 {
        split_huge(pd, idx);
        invlpg(virt);
    }
    child_table(pd, idx)
}

unsafe fn leaf_for(pml4: *mut Table, virt: u64) -> Option<*mut u64> {
    let pml4_entry = (*pml4).0[index(virt, 0)];
    if pml4_entry & PRESENT == 0 || pml4_entry & HUGE != 0 {
        return None;
    }
    let pdpt = table_ptr(pml4_entry & ADDR_MASK);
    let pdpt_idx = index(virt, 1);
    let mut entry = (*pdpt).0[pdpt_idx];
    if entry & PRESENT == 0 {
        return None;
    }
    if entry & HUGE != 0 {
        split_1g_huge(pdpt, pdpt_idx);
        invlpg(virt);
        entry = (*pdpt).0[pdpt_idx];
    }
    let pd = table_ptr(entry & ADDR_MASK);
    let pd_idx = index(virt, 2);
    entry = (*pd).0[pd_idx];
    if entry & PRESENT == 0 {
        return None;
    }
    if entry & HUGE != 0 {
        split_huge(pd, pd_idx);
        invlpg(virt);
        entry = (*pd).0[pd_idx];
    }
    let pt = table_ptr(entry & ADDR_MASK);
    let leaf = core::ptr::addr_of_mut!((*pt).0[index(virt, 3)]);
    if *leaf & PRESENT == 0 {
        None
    } else {
        Some(leaf)
    }
}

unsafe fn map4k(pml4: *mut Table, virt: u64, phys: u64, flags: u64) {
    let pdpt = child_table(pml4, index(virt, 0));
    let pd = pd_for(pdpt, virt);
    let pt = pt_for(pd, virt);
    (*pt).0[index(virt, 3)] = (phys & ADDR_MASK) | flags | PRESENT;
}

unsafe fn map2m(pml4: *mut Table, virt: u64, phys: u64, flags: u64) {
    let pdpt = child_table(pml4, index(virt, 0));
    let pd = child_table(pdpt, index(virt, 1));
    let idx = index(virt, 2);
    let existing = (*pd).0[idx];
    assert!(
        existing & PRESENT == 0 || existing & HUGE != 0,
        "VMM: 2 MiB mapping would overwrite an existing page table"
    );
    (*pd).0[idx] = (phys & HUGE_2M_MASK) | flags | PRESENT | HUGE;
}

pub fn map_page(virt: u64, phys: u64, flags: u64) {
    let mut guard = VMM.lock();
    let Some(vmm) = guard.as_mut() else { return };
    unsafe {
        map4k(table_ptr(vmm.pml4_phys), virt, phys, flags);
        invlpg(virt);
    }
}

pub fn map_mmio(phys: u64, len: u64) -> *mut u8 {
    let hhdm = pmm::hhdm_offset();
    let start = phys & !(PAGE_SIZE - 1);
    let end = (phys + len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let mut p = start;
    while p < end {
        let virt = hhdm + p;
        if translate(virt).is_none() {
            map_page(virt, p, KERNEL_UC);
        }
        p += PAGE_SIZE;
    }
    (hhdm + phys) as *mut u8
}

pub fn unmap_page(virt: u64) -> Option<u64> {
    let guard = VMM.lock();
    let Some(vmm) = guard.as_ref() else {
        return None;
    };
    unsafe {
        let pml4 = table_ptr(vmm.pml4_phys);
        let leaf = leaf_for(pml4, virt)?;
        let phys = *leaf & ADDR_MASK;
        *leaf = 0;
        invlpg(virt);
        Some(phys)
    }
}

pub fn protect_page(virt: u64, flags: u64) -> bool {
    if !matches!(
        flags,
        KERNEL_RX | KERNEL_RO | KERNEL_RW | KERNEL_WC | KERNEL_UC
    ) {
        return false;
    }
    let guard = VMM.lock();
    let Some(vmm) = guard.as_ref() else {
        return false;
    };
    unsafe {
        let Some(leaf) = leaf_for(table_ptr(vmm.pml4_phys), virt) else {
            return false;
        };
        *leaf = (*leaf & ADDR_MASK) | flags | PRESENT;
        invlpg(virt);
    }
    true
}

unsafe fn walk(pml4_phys: u64, virt: u64) -> Option<u64> {
    let pml4 = table_ptr(pml4_phys);
    let e = (*pml4).0[index(virt, 0)];
    if e & PRESENT == 0 {
        return None;
    }
    let pdpt = table_ptr(e & ADDR_MASK);
    let e = (*pdpt).0[index(virt, 1)];
    if e & PRESENT == 0 {
        return None;
    }
    if e & HUGE != 0 {
        return Some((e & HUGE_1G_MASK) + (virt & (HUGE_1G - 1)));
    }
    let pd = table_ptr(e & ADDR_MASK);
    let e = (*pd).0[index(virt, 2)];
    if e & PRESENT == 0 {
        return None;
    }
    if e & HUGE != 0 {
        return Some((e & HUGE_2M_MASK) + (virt & (HUGE_2M - 1)));
    }
    let pt = table_ptr(e & ADDR_MASK);
    let leaf = (*pt).0[index(virt, 3)];
    if leaf & PRESENT == 0 {
        return None;
    }
    Some((leaf & ADDR_MASK) + (virt & (PAGE_SIZE - 1)))
}

pub fn translate(virt: u64) -> Option<u64> {
    let guard = VMM.lock();
    let vmm = guard.as_ref()?;
    unsafe { walk(vmm.pml4_phys, virt) }
}

pub fn try_translate(virt: u64) -> Option<u64> {
    let guard = VMM.try_lock()?;
    let vmm = guard.as_ref()?;
    unsafe { walk(vmm.pml4_phys, virt) }
}

pub fn is_active() -> bool {
    VMM.lock().is_some()
}

pub fn kernel_cr3() -> u64 {
    KERNEL_CR3.load(Ordering::Acquire)
}

pub fn current_cr3() -> u64 {
    unsafe { read_cr3() & ADDR_MASK }
}

pub fn current_user_page_mapped(virt: u64) -> bool {
    if virt >= USER_SPACE_END {
        return false;
    }
    unsafe { walk_user(current_cr3(), virt).is_some() }
}

pub fn map_current_user_page(virt: u64, phys: u64) -> bool {
    if virt >= USER_SPACE_END || virt & (PAGE_SIZE - 1) != 0 || phys & (PAGE_SIZE - 1) != 0 {
        return false;
    }
    let pml4_phys = current_cr3();
    if pml4_phys == 0
        || pml4_phys == kernel_cr3()
        || unsafe { walk_user(pml4_phys, virt).is_some() }
    {
        return false;
    }
    unsafe {
        map4k(table_ptr(pml4_phys), virt, phys, USER_RW);
        invlpg(virt);
    }
    true
}

pub fn unmap_current_user_page(virt: u64) -> Option<u64> {
    if virt >= USER_SPACE_END || virt & (PAGE_SIZE - 1) != 0 {
        return None;
    }
    let pml4_phys = current_cr3();
    if pml4_phys == 0 || pml4_phys == kernel_cr3() {
        return None;
    }
    let leaf = unsafe { leaf_for(table_ptr(pml4_phys), virt)? };
    if unsafe { *leaf & (PRESENT | USER | WRITABLE) != (PRESENT | USER | WRITABLE) } {
        return None;
    }
    let phys = unsafe {
        let phys = *leaf & ADDR_MASK;
        *leaf = 0;
        phys
    };
    unsafe { invlpg(virt) };
    Some(phys)
}

pub unsafe fn read_cr3() -> u64 {
    let v: u64;
    asm!("mov {}, cr3", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

pub unsafe fn write_cr3(phys: u64) {
    asm!("mov cr3, {}", in(reg) phys, options(nostack, preserves_flags));
}

unsafe fn invlpg(virt: u64) {
    asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
}

unsafe fn enable_nxe() {
    const EFER: u32 = 0xC000_0080;
    let lo: u32;
    let hi: u32;
    asm!("rdmsr", in("ecx") EFER, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    let value = (((hi as u64) << 32) | lo as u64) | (1 << 11);
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    asm!("wrmsr", in("ecx") EFER, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags));
}

unsafe fn configure_pat() {
    const PAT_MSR: u32 = 0x277;
    const WRITE_COMBINING: u64 = 0x01;

    asm!("wbinvd", options(nomem, nostack));

    let lo: u32;
    let hi: u32;
    asm!("rdmsr", in("ecx") PAT_MSR, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    let mut value = ((hi as u64) << 32) | lo as u64;

    value &= !(0xFFu64 << 8);
    value |= WRITE_COMBINING << 8;

    let lo = value as u32;
    let hi = (value >> 32) as u32;
    asm!("wrmsr", in("ecx") PAT_MSR, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags));

    asm!("wbinvd", options(nomem, nostack));
}

fn merge_ranges(r: &mut [(u64, u64)]) -> usize {
    for i in 1..r.len() {
        let mut j = i;
        while j > 0 && r[j - 1].0 > r[j].0 {
            r.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut out = 0usize;
    for i in 0..r.len() {
        if out > 0 && r[i].0 <= r[out - 1].1 {
            if r[i].1 > r[out - 1].1 {
                r[out - 1].1 = r[i].1;
            }
        } else {
            r[out] = r[i];
            out += 1;
        }
    }
    out
}

unsafe fn map_hhdm_range(pml4: *mut Table, hhdm: u64, start: u64, end: u64) {
    let mut p = start;
    while p < end {
        if p % HUGE_2M == 0 && end - p >= HUGE_2M {
            map2m(pml4, hhdm + p, p, KERNEL_RW);
            p += HUGE_2M;
        } else {
            map4k(pml4, hhdm + p, p, KERNEL_RW);
            p += PAGE_SIZE;
        }
    }
}

unsafe fn map_kernel_sections(pml4: *mut Table, kernel_phys: u64, kernel_virt: u64) {
    let to_phys = |v: u64| kernel_phys + (v - kernel_virt);

    let map_section = |start: u64, end: u64, flags: u64| {
        let start = align_down(start, PAGE_SIZE);
        let end = align_up(end, PAGE_SIZE);
        let mut v = start;
        while v < end {
            map4k(pml4, v, to_phys(v), flags);
            v += PAGE_SIZE;
        }
    };

    map_section(
        addr_of!(__kernel_text_start) as u64,
        addr_of!(__kernel_text_end) as u64,
        KERNEL_RX,
    );
    map_section(
        addr_of!(__kernel_rodata_start) as u64,
        addr_of!(__kernel_rodata_end) as u64,
        KERNEL_RO,
    );
    map_section(
        addr_of!(__kernel_data_start) as u64,
        addr_of!(__kernel_data_end) as u64,
        KERNEL_RW,
    );
}

pub fn init() {
    unsafe { enable_nxe() };
    unsafe { configure_pat() };

    let Some(ka) = KERNEL_ADDRESS_REQUEST.get_response() else {
        log_fail!("VMM", "Init", "Limine gave us no kernel address info");
        return;
    };
    let kernel_phys = ka.physical_base();
    let kernel_virt = ka.virtual_base();

    let hhdm = pmm::hhdm_offset();
    let phys_top = pmm::phys_top();
    if phys_top == 0 {
        log_fail!(
            "VMM",
            "Init",
            "PMM has no memory range to map (run pmm::init first)"
        );
        return;
    }

    let Some(pml4_phys) = pmm::alloc_frame_zeroed() else {
        log_fail!("VMM", "Init", "Out of memory allocating the PML4");
        return;
    };
    let pml4 = table_ptr(pml4_phys);

    let mut mapped_bytes = 0u64;
    unsafe {

        let (mut ranges, n) = pmm::hhdm_ranges();
        let n = merge_ranges(&mut ranges[..n]);
        for &(start, end) in &ranges[..n] {
            map_hhdm_range(pml4, hhdm, start, end);
            mapped_bytes += end - start;
        }

        if let Some(fb) = crate::framebuffer::FRAMEBUFFER.lock().as_ref() {
            if let Some(fb_phys) = (fb.addr as u64).checked_sub(hhdm) {
                let start = align_down(fb_phys, PAGE_SIZE);
                let end = align_up(fb_phys + fb.pitch * fb.height, PAGE_SIZE);
                let mut p = start;
                while p < end {
                    map4k(pml4, hhdm + p, p, KERNEL_WC);
                    p += PAGE_SIZE;
                }
            }
        }

        map_kernel_sections(pml4, kernel_phys, kernel_virt);
    }

    let old_cr3 = unsafe { read_cr3() };
    unsafe { write_cr3(pml4_phys) };

    *VMM.lock() = Some(Vmm { pml4_phys });
    KERNEL_CR3.store(pml4_phys, Ordering::Release);

    log_ok!(
        "VMM",
        "Init",
        "Own page tables active ({} physical mapped, cr3 {:#x} -> {:#x}, PAT slot 1 = write-combining)",
        Size(mapped_bytes),
        old_cr3,
        pml4_phys
    );
    log_debug!(
        "VMM",
        "Init",
        "Kernel image {:#018x} (phys {:#x}) mapped RX/RO/RW by section, NXE enabled",
        kernel_virt,
        kernel_phys
    );
}

pub struct AddressSpace {
    pml4_phys: u64,
}

unsafe impl Send for AddressSpace {}

impl AddressSpace {

    pub fn phys(&self) -> u64 {
        self.pml4_phys
    }

    pub unsafe fn activate(&self) {
        write_cr3(self.pml4_phys);
    }

    pub fn map(&self, virt: u64, phys: u64, flags: u64) {
        assert!(
            virt < USER_SPACE_END,
            "user mapping outside user address space"
        );
        assert_eq!(
            virt & (PAGE_SIZE - 1),
            0,
            "user virtual address is not page-aligned"
        );
        assert_eq!(
            phys & (PAGE_SIZE - 1),
            0,
            "user physical address is not page-aligned"
        );
        assert!(
            matches!(flags, USER_RX | USER_RO | USER_RW),
            "unsupported user page permissions"
        );
        assert!(
            self.translate(virt).is_none(),
            "user mapping would replace an owned page"
        );
        unsafe {
            map4k(table_ptr(self.pml4_phys), virt, phys, flags);

            invlpg(virt);
        }
    }

    pub fn unmap(&self, virt: u64) -> Option<u64> {
        assert!(
            virt < USER_SPACE_END,
            "user unmap outside user address space"
        );
        let leaf = unsafe { leaf_for(table_ptr(self.pml4_phys), virt)? };
        let phys = unsafe {
            let phys = *leaf & ADDR_MASK;
            *leaf = 0;
            phys
        };
        unsafe { invlpg(virt) };
        Some(phys)
    }

    pub fn protect(&self, virt: u64, flags: u64) -> bool {
        if virt >= USER_SPACE_END || !matches!(flags, USER_RX | USER_RO | USER_RW) {
            return false;
        }
        let Some(leaf) = (unsafe { leaf_for(table_ptr(self.pml4_phys), virt) }) else {
            return false;
        };
        unsafe {
            *leaf = (*leaf & ADDR_MASK) | flags | PRESENT;

            invlpg(virt);
        }
        true
    }

    pub fn translate(&self, virt: u64) -> Option<u64> {
        unsafe { walk(self.pml4_phys, virt) }
    }
}

impl Drop for AddressSpace {

    fn drop(&mut self) {
        unsafe { free_user_half(self.pml4_phys) };
    }
}

unsafe fn free_user_half(pml4_phys: u64) {
    let pml4 = table_ptr(pml4_phys);
    for i in 0..KERNEL_PML4_START {
        let e = (*pml4).0[i];
        if e & PRESENT != 0 {
            free_pdpt(e & ADDR_MASK);
        }
    }
    pmm::free_frame(pml4_phys);
}

unsafe fn free_pdpt(phys: u64) {
    let t = table_ptr(phys);
    for i in 0..ENTRIES {
        let e = (*t).0[i];
        if e & PRESENT == 0 {
            continue;
        }
        if e & HUGE != 0 {
            pmm::free_frame(e & HUGE_1G_MASK);
        } else {
            free_pd(e & ADDR_MASK);
        }
    }
    pmm::free_frame(phys);
}

unsafe fn free_pd(phys: u64) {
    let t = table_ptr(phys);
    for i in 0..ENTRIES {
        let e = (*t).0[i];
        if e & PRESENT == 0 {
            continue;
        }
        if e & HUGE != 0 {
            pmm::free_frame(e & HUGE_2M_MASK);
        } else {
            free_pt(e & ADDR_MASK);
        }
    }
    pmm::free_frame(phys);
}

unsafe fn free_pt(phys: u64) {
    let t = table_ptr(phys);
    for i in 0..ENTRIES {
        let e = (*t).0[i];
        if e & PRESENT != 0 {
            pmm::free_frame(e & ADDR_MASK);
        }
    }
    pmm::free_frame(phys);
}

unsafe fn walk_user(pml4_phys: u64, virt: u64) -> Option<u64> {
    let pml4 = table_ptr(pml4_phys);
    let e = (*pml4).0[index(virt, 0)];
    if e & (PRESENT | USER) != (PRESENT | USER) {
        return None;
    }
    let pdpt = table_ptr(e & ADDR_MASK);
    let e = (*pdpt).0[index(virt, 1)];
    if e & (PRESENT | USER) != (PRESENT | USER) || e & HUGE != 0 {
        return None;
    }
    let pd = table_ptr(e & ADDR_MASK);
    let e = (*pd).0[index(virt, 2)];
    if e & (PRESENT | USER) != (PRESENT | USER) || e & HUGE != 0 {
        return None;
    }
    let pt = table_ptr(e & ADDR_MASK);
    let leaf = (*pt).0[index(virt, 3)];
    if leaf & (PRESENT | USER) != (PRESENT | USER) {
        return None;
    }
    Some((leaf & ADDR_MASK) + (virt & (PAGE_SIZE - 1)))
}

pub fn copy_from_user(dst: &mut [u8], src: u64) -> bool {
    if !validate_user_range(src, dst.len(), false) {
        return false;
    }

    let pml4 = current_cr3();
    let mut done = 0usize;
    while done < dst.len() {
        let va = src + done as u64;
        let page_off = (va & (PAGE_SIZE - 1)) as usize;
        let n = (dst.len() - done).min(PAGE_SIZE as usize - page_off);
        let Some(phys) = (unsafe { walk_user(pml4, va) }) else {
            return false;
        };
        unsafe {
            core::ptr::copy_nonoverlapping(
                pmm::phys_to_virt(phys) as *const u8,
                dst.as_mut_ptr().add(done),
                n,
            );
        }
        done += n;
    }
    true
}

pub fn copy_to_user(dst: u64, src: &[u8]) -> bool {
    if !validate_user_range(dst, src.len(), true) {
        return false;
    }

    let pml4 = current_cr3();
    let mut done = 0usize;
    while done < src.len() {
        let va = dst + done as u64;
        let page_off = (va & (PAGE_SIZE - 1)) as usize;
        let n = (src.len() - done).min(PAGE_SIZE as usize - page_off);
        let Some(phys) = (unsafe { walk_user_write(pml4, va) }) else {
            return false;
        };
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.as_ptr().add(done),
                pmm::phys_to_virt(phys) as *mut u8,
                n,
            );
        }
        done += n;
    }
    true
}

pub fn validate_user_range(addr: u64, len: usize, write: bool) -> bool {
    if len == 0 {
        return true;
    }
    let Some(end) = addr.checked_add(len as u64) else {
        return false;
    };
    if addr >= USER_SPACE_END || end > USER_SPACE_END {
        return false;
    }

    let pml4 = current_cr3();
    let mut va = addr;
    while va < end {
        let mapped = unsafe {
            if write {
                walk_user_write(pml4, va)
            } else {
                walk_user(pml4, va)
            }
        };
        if mapped.is_none() {
            return false;
        }
        let next_page = (va & !(PAGE_SIZE - 1)).saturating_add(PAGE_SIZE);
        if next_page <= va {
            return false;
        }
        va = next_page.min(end);
    }
    true
}

unsafe fn walk_user_write(pml4_phys: u64, virt: u64) -> Option<u64> {
    let pml4 = table_ptr(pml4_phys);
    let e = (*pml4).0[index(virt, 0)];
    if e & (PRESENT | USER) != (PRESENT | USER) {
        return None;
    }
    let pdpt = table_ptr(e & ADDR_MASK);
    let e = (*pdpt).0[index(virt, 1)];
    if e & (PRESENT | USER) != (PRESENT | USER) || e & HUGE != 0 {
        return None;
    }
    let pd = table_ptr(e & ADDR_MASK);
    let e = (*pd).0[index(virt, 2)];
    if e & (PRESENT | USER) != (PRESENT | USER) || e & HUGE != 0 {
        return None;
    }
    let pt = table_ptr(e & ADDR_MASK);
    let leaf = (*pt).0[index(virt, 3)];
    if leaf & (PRESENT | USER | WRITABLE) != (PRESENT | USER | WRITABLE) {
        return None;
    }
    Some((leaf & ADDR_MASK) + (virt & (PAGE_SIZE - 1)))
}

pub fn create_address_space() -> Option<AddressSpace> {
    let guard = VMM.lock();
    let kernel = guard.as_ref()?;
    let kernel_pml4 = table_ptr(kernel.pml4_phys);

    let pml4_phys = pmm::alloc_frame_zeroed()?;
    let pml4 = table_ptr(pml4_phys);
    unsafe {
        for i in KERNEL_PML4_START..ENTRIES {
            (*pml4).0[i] = (*kernel_pml4).0[i];
        }
    }

    Some(AddressSpace { pml4_phys })
}

pub fn self_test() {
    if !is_active() {
        log_fail!("VMM", "SelfTest", "VMM not initialized");
        return;
    }

    let Some(phys) = pmm::alloc_frame() else {
        log_fail!("VMM", "SelfTest", "No free frame to test with");
        return;
    };

    map_page(SELF_TEST_VIRT, phys, KERNEL_RW);

    let ok = unsafe {
        let ptr = SELF_TEST_VIRT as *mut u64;
        ptr.write_volatile(0xCAFE_F00D_1234_5678);
        let read_back = ptr.read_volatile();
        read_back == 0xCAFE_F00D_1234_5678 && translate(SELF_TEST_VIRT) == Some(phys)
    };

    let protect_ok = protect_page(SELF_TEST_VIRT, KERNEL_RO)
        && translate(SELF_TEST_VIRT) == Some(phys)
        && protect_page(SELF_TEST_VIRT, KERNEL_RW);
    let unmapped = unmap_page(SELF_TEST_VIRT);
    if let Some(frame) = unmapped {
        pmm::free_frame(frame);
    }

    if translate(SELF_TEST_VIRT).is_some() || unmapped != Some(phys) {
        log_fail!(
            "VMM",
            "SelfTest",
            "Page survived unmap at {:#x}",
            SELF_TEST_VIRT
        );
        return;
    }

    if ok && protect_ok {
        log_ok!(
            "VMM",
            "SelfTest",
            "Mapped, wrote, translated and unmapped a test page ({:#x} -> {:#x})",
            SELF_TEST_VIRT,
            phys
        );
    } else {
        log_fail!(
            "VMM",
            "SelfTest",
            "Mapping/translation mismatch for {:#x}",
            SELF_TEST_VIRT
        );
    }
}

pub fn address_space_self_test() {
    if !is_active() {
        log_fail!("VMM", "AddressSpaceSelfTest", "VMM not initialized");
        return;
    }

    let frames_before = pmm::free_frame_count();

    let Some(aspace) = create_address_space() else {
        log_fail!("VMM", "AddressSpaceSelfTest", "create_address_space failed");
        return;
    };

    unsafe {
        let pml4 = table_ptr(aspace.pml4_phys);
        for i in 0..KERNEL_PML4_START {
            if (*pml4).0[i] != 0 {
                log_fail!(
                    "VMM",
                    "AddressSpaceSelfTest",
                    "User half not empty at PML4[{}]",
                    i
                );
                return;
            }
        }
    }

    let kernel_matches = {
        let guard = VMM.lock();
        let kernel_pml4 = table_ptr(guard.as_ref().unwrap().pml4_phys);
        let new_pml4 = table_ptr(aspace.pml4_phys);
        unsafe { (KERNEL_PML4_START..ENTRIES).all(|i| (*kernel_pml4).0[i] == (*new_pml4).0[i]) }
    };
    if !kernel_matches {
        log_fail!(
            "VMM",
            "AddressSpaceSelfTest",
            "Kernel half diverges from the kernel's own PML4"
        );
        return;
    }

    let probe = addr_of!(__kernel_text_start) as u64;
    if aspace.translate(probe) != translate(probe) {
        log_fail!(
            "VMM",
            "AddressSpaceSelfTest",
            "New address space disagrees with the kernel on kernel text at {:#x}",
            probe
        );
        return;
    }

    let Some(user_phys) = pmm::alloc_frame() else {
        log_fail!("VMM", "AddressSpaceSelfTest", "No free frame to test with");
        return;
    };
    aspace.map(SELF_TEST_USER_VIRT, user_phys, USER_RW);

    let old_cr3 = unsafe { read_cr3() };
    unsafe { aspace.activate() };

    let ok = unsafe {
        let ptr = SELF_TEST_USER_VIRT as *mut u64;
        ptr.write_volatile(0xA11C_0A11_C0DE_5A5A);
        ptr.read_volatile() == 0xA11C_0A11_C0DE_5A5A
    };

    let protect_ro_ok = aspace.protect(SELF_TEST_USER_VIRT, USER_RX)
        && unsafe { walk_user(aspace.pml4_phys, SELF_TEST_USER_VIRT).is_some() }
        && unsafe { walk_user_write(aspace.pml4_phys, SELF_TEST_USER_VIRT).is_none() };
    let protect_rw_ok = aspace.protect(SELF_TEST_USER_VIRT, USER_RW)
        && unsafe { walk_user_write(aspace.pml4_phys, SELF_TEST_USER_VIRT).is_some() };

    unsafe { write_cr3(old_cr3) };
    log_debug!(
        "VMM",
        "AddressSpaceSelfTest",
        "Restored kernel CR3 {:#x} after running with {:#x} active",
        old_cr3,
        aspace.pml4_phys
    );

    if !ok {
        log_fail!(
            "VMM",
            "AddressSpaceSelfTest",
            "Private-half mapping did not read back correctly while active"
        );
        return;
    }

    if !protect_ro_ok || !protect_rw_ok {
        log_fail!(
            "VMM",
            "AddressSpaceSelfTest",
            "Protect did not update user page permissions as requested"
        );
        return;
    }

    let unmapped = aspace.unmap(SELF_TEST_USER_VIRT);
    let unmap_ok = unmapped == Some(user_phys) && aspace.translate(SELF_TEST_USER_VIRT).is_none();
    if let Some(phys) = unmapped {
        pmm::free_frame(phys);
    }
    if !unmap_ok {
        log_fail!(
            "VMM",
            "AddressSpaceSelfTest",
            "Unmap did not return the removed frame and clear the mapping"
        );
        return;
    }

    drop(aspace);

    let frames_after = pmm::free_frame_count();
    if frames_after != frames_before {
        log_fail!(
            "VMM",
            "AddressSpaceSelfTest",
            "Frames leaked: {} free before, {} after",
            frames_before,
            frames_after
        );
        return;
    }

    log_ok!(
        "VMM",
        "AddressSpaceSelfTest",
        "New address space shares the kernel half (verified by walk and by live CR3 switch), private half torn down cleanly ({} frames free)",
        frames_after
    );
}

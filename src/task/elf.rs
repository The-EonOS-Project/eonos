#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

use crate::pmm::{self, PAGE_SIZE};
use crate::vmm::{self, AddressSpace};

pub const LOAD_MIN: u64 = 0x1000;

pub const LOAD_MAX: u64 = 0x0000_4000_0000_0000;

const MAX_SEGMENTS: usize = 16;
const MAX_PHDRS: u16 = 64;

const MAX_PAGES: u64 = 16 * 1024;

const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;

const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;

const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;

const PF_X: u32 = 1;
const PF_W: u32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElfError {

    TooShort,

    BadMagic,

    NotElf64Le,

    NotExecutable,

    WrongArch,

    BadHeader,

    Dynamic,

    Truncated,

    BadSegment,

    TooManySegments,

    Overlap,

    WritableExecutable,

    TooLarge,

    NoLoadable,

    BadEntry,

    NoMemory,
}

impl ElfError {
    pub fn as_str(self) -> &'static str {
        match self {
            ElfError::TooShort => "file too short for an ELF header",
            ElfError::BadMagic => "not an ELF file (bad magic)",
            ElfError::NotElf64Le => "not a 64-bit little-endian ELF",
            ElfError::NotExecutable => "not a static executable (ET_EXEC)",
            ElfError::WrongArch => "not an x86-64 executable",
            ElfError::BadHeader => "bad program header table",
            ElfError::Dynamic => "dynamically linked executables are not supported",
            ElfError::Truncated => "header or segment extends past the end of the file",
            ElfError::BadSegment => "segment size or address is invalid",
            ElfError::TooManySegments => "too many loadable segments",
            ElfError::Overlap => "loadable segments overlap",
            ElfError::WritableExecutable => "segment is both writable and executable",
            ElfError::TooLarge => "image is too large",
            ElfError::NoLoadable => "no loadable segments",
            ElfError::BadEntry => "entry point is not inside an executable segment",
            ElfError::NoMemory => "out of memory",
        }
    }
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

struct Segment {
    vaddr: u64,
    offset: u64,
    filesz: u64,
    memsz: u64,
    flags: u32,
}

struct Page {
    phys: u64,
    writable: bool,
    executable: bool,
}

const fn align_down(v: u64) -> u64 {
    v & !(PAGE_SIZE - 1)
}

const fn align_up(v: u64) -> u64 {
    (v + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

fn rd16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn rd64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes([
        b[o],
        b[o + 1],
        b[o + 2],
        b[o + 3],
        b[o + 4],
        b[o + 5],
        b[o + 6],
        b[o + 7],
    ])
}

fn parse(image: &[u8]) -> Result<(u64, Vec<Segment>), ElfError> {
    if image.len() < EHDR_SIZE {
        return Err(ElfError::TooShort);
    }
    if &image[0..4] != b"\x7fELF" {
        return Err(ElfError::BadMagic);
    }

    if image[4] != 2 || image[5] != 1 {
        return Err(ElfError::NotElf64Le);
    }
    if rd16(image, 16) != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }
    if rd16(image, 18) != EM_X86_64 {
        return Err(ElfError::WrongArch);
    }

    let entry = rd64(image, 24);
    let phoff = rd64(image, 32);
    let phentsize = rd16(image, 54) as usize;
    let phnum = rd16(image, 56);

    if phentsize != PHDR_SIZE || phnum == 0 || phnum > MAX_PHDRS {
        return Err(ElfError::BadHeader);
    }
    let table_end = phoff
        .checked_add(phnum as u64 * PHDR_SIZE as u64)
        .ok_or(ElfError::Truncated)?;
    if table_end > image.len() as u64 {
        return Err(ElfError::Truncated);
    }

    let mut segments: Vec<Segment> = Vec::new();
    let mut total_pages = 0u64;

    for i in 0..phnum as usize {
        let o = phoff as usize + i * PHDR_SIZE;
        let p_type = rd32(image, o);
        let p_flags = rd32(image, o + 4);
        let p_offset = rd64(image, o + 8);
        let p_vaddr = rd64(image, o + 16);
        let p_filesz = rd64(image, o + 32);
        let p_memsz = rd64(image, o + 40);

        if p_type == PT_INTERP {
            return Err(ElfError::Dynamic);
        }
        if p_type != PT_LOAD || p_memsz == 0 {
            continue;
        }

        if p_filesz > p_memsz {
            return Err(ElfError::BadSegment);
        }
        let end = p_vaddr.checked_add(p_memsz).ok_or(ElfError::BadSegment)?;
        if p_vaddr < LOAD_MIN || end > LOAD_MAX {
            return Err(ElfError::BadSegment);
        }
        let file_end = p_offset.checked_add(p_filesz).ok_or(ElfError::Truncated)?;
        if file_end > image.len() as u64 {
            return Err(ElfError::Truncated);
        }
        if p_flags & PF_W != 0 && p_flags & PF_X != 0 {
            return Err(ElfError::WritableExecutable);
        }
        if segments.len() == MAX_SEGMENTS {
            return Err(ElfError::TooManySegments);
        }

        total_pages += (align_up(end) - align_down(p_vaddr)) / PAGE_SIZE;
        if total_pages > MAX_PAGES {
            return Err(ElfError::TooLarge);
        }

        segments.push(Segment {
            vaddr: p_vaddr,
            offset: p_offset,
            filesz: p_filesz,
            memsz: p_memsz,
            flags: p_flags,
        });
    }

    if segments.is_empty() {
        return Err(ElfError::NoLoadable);
    }

    for i in 0..segments.len() {
        for j in (i + 1)..segments.len() {
            let a = &segments[i];
            let b = &segments[j];
            if a.vaddr < b.vaddr + b.memsz && b.vaddr < a.vaddr + a.memsz {
                return Err(ElfError::Overlap);
            }
        }
    }

    let entry_ok = segments
        .iter()
        .any(|s| s.flags & PF_X != 0 && entry >= s.vaddr && entry < s.vaddr + s.memsz);
    if !entry_ok {
        return Err(ElfError::BadEntry);
    }

    Ok((entry, segments))
}

fn populate(
    image: &[u8],
    segments: &[Segment],
    pages: &mut BTreeMap<u64, Page>,
) -> Result<(), ElfError> {
    for seg in segments {
        let writable = seg.flags & PF_W != 0;
        let executable = seg.flags & PF_X != 0;

        let mut va = align_down(seg.vaddr);
        let last = align_up(seg.vaddr + seg.memsz);
        while va < last {
            if let Some(page) = pages.get_mut(&va) {

                page.writable |= writable;
                page.executable |= executable;
            } else {
                let phys = pmm::alloc_frame_zeroed().ok_or(ElfError::NoMemory)?;
                pages.insert(
                    va,
                    Page {
                        phys,
                        writable,
                        executable,
                    },
                );
            }
            va += PAGE_SIZE;
        }

        let mut copied = 0u64;
        while copied < seg.filesz {
            let dst_va = seg.vaddr + copied;
            let page_va = align_down(dst_va);
            let in_page = dst_va - page_va;
            let n = (seg.filesz - copied).min(PAGE_SIZE - in_page);
            let Some(page) = pages.get(&page_va) else {
                return Err(ElfError::BadSegment); 
            };
            unsafe {
                core::ptr::copy_nonoverlapping(
                    image.as_ptr().add((seg.offset + copied) as usize),
                    pmm::phys_to_virt(page.phys).add(in_page as usize),
                    n as usize,
                );
            }
            copied += n;
        }
    }

    if pages.values().any(|p| p.writable && p.executable) {
        return Err(ElfError::WritableExecutable);
    }
    Ok(())
}

fn free_pages(pages: &BTreeMap<u64, Page>) {
    for page in pages.values() {
        pmm::free_frame(page.phys);
    }
}

pub fn load(image: &[u8], aspace: &AddressSpace) -> Result<u64, ElfError> {
    let (entry, segments) = parse(image)?;

    let mut pages: BTreeMap<u64, Page> = BTreeMap::new();
    if let Err(e) = populate(image, &segments, &mut pages) {
        free_pages(&pages);
        return Err(e);
    }

    for (&va, page) in pages.iter() {
        let flags = match (page.writable, page.executable) {
            (false, false) => vmm::USER_RO,
            (true, false) => vmm::USER_RW,
            (false, true) => vmm::USER_RX,

            (true, true) => vmm::USER_RO,
        };
        aspace.map(va, page.phys, flags);
    }

    Ok(entry)
}

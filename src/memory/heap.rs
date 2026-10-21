








#![allow(dead_code)]

use core::alloc::{GlobalAlloc, Layout};
use core::fmt;
use core::mem::{align_of, size_of};
use core::ptr;

use crate::pmm;
use crate::sync::IrqMutex;
use crate::vmm;
use crate::{log_debug, log_fail, log_ok};





const HEAP_START: usize = 0xffff_ffff_9000_0000;




const HEAP_MAX_SIZE: usize = 64 * 1024 * 1024; 




const HEAP_GROW_STEP: usize = 64 * 1024; 

const PAGE_SIZE: usize = pmm::PAGE_SIZE as usize;

const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

struct Size(usize);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.0 >= (1 << 20) {
            write!(f, "{} MiB", self.0 >> 20)
        } else if self.0 >= (1 << 10) {
            write!(f, "{} KiB", self.0 >> 10)
        } else {
            write!(f, "{} B", self.0)
        }
    }
}




struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

impl FreeBlock {
    fn addr(&self) -> usize {
        self as *const _ as usize
    }

    fn end(&self) -> usize {
        self.addr() + self.size
    }
}

struct FreeList {
    
    
    head: *mut FreeBlock,
    
    
    mapped_end: usize,
    
    allocated: usize,
    
    total: usize,
}



unsafe impl Send for FreeList {}

impl FreeList {
    const fn new() -> Self {
        FreeList {
            head: ptr::null_mut(),
            mapped_end: 0,
            allocated: 0,
            total: 0,
        }
    }

    
    
    
    
    unsafe fn insert(&mut self, addr: usize, size: usize) {
        debug_assert_eq!(addr % align_of::<FreeBlock>(), 0);
        debug_assert!(size >= size_of::<FreeBlock>());

        let mut prev: *mut FreeBlock = ptr::null_mut();
        let mut cur = self.head;
        while !cur.is_null() && (*cur).addr() < addr {
            prev = cur;
            cur = (*cur).next;
        }

        
        let (size, next) = if !cur.is_null() && addr + size == (*cur).addr() {
            (size + (*cur).size, (*cur).next)
        } else {
            (size, cur)
        };

        
        
        if !prev.is_null() && (*prev).end() == addr {
            (*prev).size += size;
            (*prev).next = next;
            return;
        }

        let block = addr as *mut FreeBlock;
        block.write(FreeBlock { size, next });

        if prev.is_null() {
            self.head = block;
        } else {
            (*prev).next = block;
        }
    }

    
    
    
    unsafe fn find_and_remove(&mut self, size: usize, align: usize) -> Option<usize> {
        let mut prev: *mut FreeBlock = ptr::null_mut();
        let mut cur = self.head;

        while !cur.is_null() {
            let block_addr = (*cur).addr();
            let block_size = (*cur).size;
            let alloc_start = align_up(block_addr, align);
            let padding = alloc_start - block_addr;

            if padding + size <= block_size {
                let next = (*cur).next;
                if prev.is_null() {
                    self.head = next;
                } else {
                    (*prev).next = next;
                }

                if padding >= size_of::<FreeBlock>() {
                    self.insert(block_addr, padding);
                }
                let after_size = block_size - padding - size;
                if after_size >= size_of::<FreeBlock>() {
                    self.insert(alloc_start + size, after_size);
                }
                
                
                
                

                return Some(alloc_start);
            }

            prev = cur;
            cur = (*cur).next;
        }

        None
    }

    
    
    
    unsafe fn grow(&mut self, min_extra: usize) -> bool {
        if self.mapped_end == 0 {
            self.mapped_end = HEAP_START;
        }

        let remaining_va = HEAP_START + HEAP_MAX_SIZE - self.mapped_end;
        if remaining_va == 0 {
            log_fail!(
                "Heap",
                "Grow",
                "Heap has hit its {} ceiling",
                Size(HEAP_MAX_SIZE)
            );
            return false;
        }

        let mut grow_by = HEAP_GROW_STEP.max(align_up(min_extra, PAGE_SIZE));
        grow_by = grow_by.min(remaining_va);
        grow_by = align_up(grow_by, PAGE_SIZE).min(align_down(remaining_va, PAGE_SIZE));
        if grow_by == 0 {
            return false;
        }

        let start = self.mapped_end;
        let mut mapped = 0usize;
        while mapped < grow_by {
            let Some(phys) = pmm::alloc_frame() else {
                log_fail!("Heap", "Grow", "PMM out of frames while growing the heap");
                break;
            };
            vmm::map_page((start + mapped) as u64, phys, vmm::KERNEL_RW);
            mapped += PAGE_SIZE;
        }

        if mapped == 0 {
            return false;
        }

        self.mapped_end = start + mapped;
        self.total += mapped;
        self.insert(start, mapped);

        log_debug!(
            "Heap",
            "Grow",
            "+{} mapped at {:#x} (heap now {})",
            Size(mapped),
            start,
            Size(self.total)
        );

        true
    }
}

const fn align_down(v: usize, a: usize) -> usize {
    v & !(a - 1)
}




fn adjust(layout: Layout) -> (usize, usize) {
    let align = layout.align().max(align_of::<FreeBlock>());
    let size = layout.size().max(size_of::<FreeBlock>());
    let size = align_up(size, align_of::<FreeBlock>());
    (size, align)
}

pub struct KernelHeap {
    inner: IrqMutex<FreeList>,
}

impl KernelHeap {
    const fn new() -> Self {
        KernelHeap {
            inner: IrqMutex::new(FreeList::new()),
        }
    }
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = adjust(layout);
        let mut list = self.inner.lock();

        if let Some(addr) = list.find_and_remove(size, align) {
            list.allocated += size;
            return addr as *mut u8;
        }

        
        
        
        if !list.grow(size + align) {
            return ptr::null_mut();
        }

        match list.find_and_remove(size, align) {
            Some(addr) => {
                list.allocated += size;
                addr as *mut u8
            }
            None => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _align) = adjust(layout);
        let mut list = self.inner.lock();
        list.allocated = list.allocated.saturating_sub(size);
        list.insert(ptr as usize, size);
    }
}

#[global_allocator]
static ALLOCATOR: KernelHeap = KernelHeap::new();


pub fn allocated_bytes() -> usize {
    ALLOCATOR.inner.lock().allocated
}


pub fn heap_size() -> usize {
    ALLOCATOR.inner.lock().total
}

pub fn init() {
    let mut list = ALLOCATOR.inner.lock();
    if !unsafe { list.grow(HEAP_GROW_STEP) } {
        drop(list);
        log_fail!("Heap", "Init", "Could not map the initial heap region");
        return;
    }
    let total = list.total;
    drop(list);

    log_ok!(
        "Heap",
        "Init",
        "Free-list allocator online, {} at {:#x} (grows lazily up to {})",
        Size(total),
        HEAP_START,
        Size(HEAP_MAX_SIZE)
    );
}

pub fn self_test() {
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    let before = allocated_bytes();

    
    let boxed = Box::new(0x1234_5678_9abc_def0u64);
    if *boxed != 0x1234_5678_9abc_def0 {
        log_fail!("Heap", "SelfTest", "Box value mismatch");
        return;
    }
    drop(boxed);

    
    
    let mut v: Vec<u32> = Vec::new();
    for i in 0..2048u32 {
        v.push(i);
    }
    if v.len() != 2048 || v[0] != 0 || v[2047] != 2047 {
        log_fail!("Heap", "SelfTest", "Vec contents corrupted after growth");
        return;
    }
    let sum: u64 = v.iter().map(|&x| x as u64).sum();
    if sum != (0..2048u64).sum::<u64>() {
        log_fail!("Heap", "SelfTest", "Vec checksum mismatch");
        return;
    }
    drop(v);

    
    
    
    let heap_before_stress = heap_size();
    {
        let mut chunks: Vec<Box<[u8; 512]>> = Vec::new();
        for _ in 0..300 {
            chunks.push(Box::new([0xAA; 512]));
        }
        for c in &chunks {
            if c[0] != 0xAA || c[511] != 0xAA {
                log_fail!("Heap", "SelfTest", "Corrupted allocation under stress");
                return;
            }
        }
    } 

    let grew = heap_size() > heap_before_stress;

    let after = allocated_bytes();
    if after != before {
        log_fail!(
            "Heap",
            "SelfTest",
            "Leak detected: {} bytes allocated before, {} after (everything should have been freed)",
            before,
            after
        );
        return;
    }

    log_ok!(
        "Heap",
        "SelfTest",
        "Box/Vec alloc-free verified, coalesced back to {} allocated ({}grew during stress test)",
        after,
        if grew { "" } else { "did not " }
    );
}

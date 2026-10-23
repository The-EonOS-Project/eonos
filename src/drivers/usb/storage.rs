use alloc::boxed::Box;
use alloc::format;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::block::{BlockDevice, BlockError};
use crate::fat32::SECTOR_SIZE;

static NEXT_DISK_NUMBER: AtomicUsize = AtomicUsize::new(0);

pub struct UsbDisk {
    pub controller_index: usize,
    pub storage_id: u64,
    pub sectors: u64,
}

impl UsbDisk {
    pub fn leak(controller_index: usize, storage_id: u64, sectors: u64) -> (&'static str, &'static dyn BlockDevice) {
        let number = NEXT_DISK_NUMBER.fetch_add(1, Ordering::Relaxed);
        let name = Box::leak(format!("usb{}", number).into_boxed_str());
        let device: &'static UsbDisk = Box::leak(Box::new(Self { controller_index, storage_id, sectors }));
        (name, device)
    }
}

impl BlockDevice for UsbDisk {
    fn sector_count(&self) -> u64 { self.sectors }

    fn read_sector(&self, lba: u64, out: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        if lba >= self.sectors { return Err(BlockError::OutOfRange); }
        super::xhci::read_storage_sector(self.controller_index, self.storage_id, lba, out)
    }

    fn write_sector(&self, lba: u64, data: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        if lba >= self.sectors { return Err(BlockError::OutOfRange); }
        super::xhci::write_storage_sector(self.controller_index, self.storage_id, lba, data)
    }

    fn flush(&self) -> Result<(), BlockError> {
        super::xhci::flush_storage(self.controller_index, self.storage_id)
    }
}

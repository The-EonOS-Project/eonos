//! Small safe helpers for common OS interactions.
//!
//! Third-party apps are restricted by the kernel regardless of which helper
//! they call. This module deliberately offers read-only file access and basic
//! status/time services; it does not expose device, network, power, or write
//! operations.

use alloc::string::String;
use alloc::vec::Vec;
use eonos_rt::{Error, Result};

const MAX_READ_FILE: usize = 1024 * 1024;

/// Return milliseconds since boot.
pub fn uptime_ms() -> u64 {
    eonos_rt::uptime_ms() as u64
}

/// Yield the process for at least the requested number of milliseconds.
pub fn sleep_ms(milliseconds: u64) -> Result<()> {
    eonos_rt::sleep_ms(milliseconds.min(60_000) as usize)
}

/// Print UTF-8 text to the application's standard output.
pub fn print(value: &str) -> Result<()> {
    eonos_rt::write_all(1, value.as_bytes())
}

/// Read a file up to one MiB. Writable open flags are not available here.
pub fn read_file(path: &str) -> Result<Vec<u8>> {
    let fd = eonos_rt::open(path, eonos_rt::O_RDONLY)?;
    let result = (|| {
        let mut contents = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let count = eonos_rt::read(fd, &mut chunk)?;
            if count == 0 { return Ok(contents); }
            if contents.len().saturating_add(count) > MAX_READ_FILE { return Err(Error(7)); }
            contents.extend_from_slice(&chunk[..count]);
        }
    })();
    let _ = eonos_rt::close(fd);
    result
}

/// Read a directory's newline-separated entries (up to the kernel's buffer limit).
pub fn list_directory(path: &str) -> Result<String> {
    let mut buffer = [0u8; 4096];
    let count = eonos_rt::list_directory(path, &mut buffer)?;
    String::from_utf8(buffer[..count].to_vec()).map_err(|_| Error(22))
}

/// Snapshot basic physical-memory and kernel-heap counters.
pub fn memory_info() -> Result<eonos_rt::MemoryInfo> {
    eonos_rt::memory_info()
}

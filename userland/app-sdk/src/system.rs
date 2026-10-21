





use alloc::string::String;
use alloc::vec::Vec;
use eonos_rt::{Error, Result};

const MAX_READ_FILE: usize = 1024 * 1024;


pub fn uptime_ms() -> u64 {
    eonos_rt::uptime_ms() as u64
}


pub fn sleep_ms(milliseconds: u64) -> Result<()> {
    eonos_rt::sleep_ms(milliseconds.min(60_000) as usize)
}


pub fn print(value: &str) -> Result<()> {
    eonos_rt::write_all(1, value.as_bytes())
}


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




pub fn write_file(path: &str, contents: &[u8]) -> Result<()> {
    write_file_with_flags(path, contents, eonos_rt::O_WRONLY | eonos_rt::O_CREAT | eonos_rt::O_TRUNC)
}


pub fn append_file(path: &str, contents: &[u8]) -> Result<()> {
    write_file_with_flags(path, contents, eonos_rt::O_WRONLY | eonos_rt::O_CREAT | eonos_rt::O_APPEND)
}

fn write_file_with_flags(path: &str, contents: &[u8], flags: usize) -> Result<()> {
    let fd = eonos_rt::open(path, flags)?;
    let write_result = eonos_rt::write_all(fd, contents);
    let close_result = eonos_rt::close(fd);
    write_result?;
    close_result
}


pub fn list_directory(path: &str) -> Result<String> {
    let mut buffer = [0u8; 4096];
    let count = eonos_rt::list_directory(path, &mut buffer)?;
    String::from_utf8(buffer[..count].to_vec()).map_err(|_| Error(22))
}


pub fn memory_info() -> Result<eonos_rt::MemoryInfo> {
    eonos_rt::memory_info()
}

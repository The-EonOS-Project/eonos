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

pub fn install_app_package(path: &str) -> Result<String> {
    if !valid_package_path(path) { return Err(Error(22)); }
    run_package_command(&alloc::format!("install {}", path))
}

pub fn uninstall_app(id: &str) -> Result<String> {
    if id.is_empty() || id.len() > 32
        || !id.as_bytes()[0].is_ascii_lowercase()
        || !id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-')
    { return Err(Error(22)); }
    run_package_command(&alloc::format!("uninstall {}", id))
}

fn valid_package_path(path: &str) -> bool {
    (path.starts_with("/packages/") || path.starts_with("/disk/"))
        && path.ends_with(".eonapp")
        && !path.split('/').any(|part| part == ".." || part.is_empty())
        && path.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'.'))
}

fn run_package_command(command: &str) -> Result<String> {
    use alloc::string::String;
    use alloc::vec::Vec;
    let (read_fd, write_fd) = eonos_rt::pipe()?;
    let environment = ["PATH=/disk/apps/bin:/apps/bin:/bin", "HOME=/", "USER=superuser", "HOSTNAME=eonos"];
    let process = match eonos_user::Process::spawn_sandboxed_with_stdio(
        "/bin/rust-shell", &["-c", command], &environment, None, Some(write_fd),
    ) {
        Ok(process) => process,
        Err(_) => {
            let _ = eonos_rt::close(read_fd);
            let _ = eonos_rt::close(write_fd);
            return Err(Error(5));
        }
    };
    let _ = eonos_rt::close(write_fd);
    let mut output = Vec::new();
    let mut buffer = [0u8; 512];
    let status = loop {
        let count = eonos_rt::pipe_read_available(read_fd, &mut buffer)?;
        if count != 0 && output.len() < 4096 {
            let keep = count.min(4096 - output.len());
            output.extend_from_slice(&buffer[..keep]);
        }
        match process.try_wait().map_err(|_| Error(5))? {
            Some(status) if count == 0 => break status,
            Some(status) => {
                while let Ok(remaining) = eonos_rt::pipe_read_available(read_fd, &mut buffer) {
                    if remaining == 0 { break; }
                    let keep = remaining.min(4096usize.saturating_sub(output.len()));
                    output.extend_from_slice(&buffer[..keep]);
                }
                break status;
            }
            None => eonos_rt::sleep_ms(10)?,
        }
    };
    let _ = eonos_rt::close(read_fd);
    let message = String::from_utf8_lossy(&output).into_owned();
    let command_failed = message.lines().any(|line| {
        line.starts_with("error:") || line.starts_with("install:") || line.starts_with("uninstall:")
    });
    if status.code == 0 && !command_failed { Ok(message) } else { Err(Error(5)) }
}

pub fn memory_info() -> Result<eonos_rt::MemoryInfo> {
    eonos_rt::memory_info()
}

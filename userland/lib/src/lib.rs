#![no_std]

extern crate alloc;

use alloc::ffi::{CString, NulError};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

pub const MAX_LINE_LENGTH: usize = 4096;

#[derive(Debug)]
pub enum Error {

    System(eonos_rt::Error),

    InteriorNul,

    InvalidUtf8,

    LineTooLong,

    InvalidInput,

    InvalidArgument,

    Usage(&'static str),
}

impl From<eonos_rt::Error> for Error {
    fn from(error: eonos_rt::Error) -> Self {
        Self::System(error)
    }
}

impl From<NulError> for Error {
    fn from(_: NulError) -> Self {
        Self::InteriorNul
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System(error) => f.write_str(errno_message(error.0)),
            Self::InteriorNul => f.write_str("string contains a NUL byte"),
            Self::InvalidUtf8 => f.write_str("file contents are not valid UTF-8"),
            Self::LineTooLong => f.write_str("input line is too long"),
            Self::InvalidInput => f.write_str("unclosed quote or trailing escape"),
            Self::InvalidArgument => f.write_str("invalid argument"),
            Self::Usage(usage) => write!(f, "usage: {}", usage),
        }
    }
}

fn errno_message(errno: i32) -> &'static str {
    match errno {
        2 => "no such file or directory",
        5 => "I/O error",
        9 => "bad file descriptor",
        10 => "no child process",
        11 => "resource temporarily unavailable",
        12 => "out of memory",
        13 => "permission denied",
        14 => "bad user memory address",
        17 => "file already exists",
        20 => "not a directory",
        21 => "is a directory",
        22 => "invalid argument",
        28 => "no space left on device",
        30 => "read-only filesystem",
        34 => "result too large",
        36 => "path too long",
        38 => "operation not supported",
        95 => "operation not supported",
        39 => "directory not empty",
        19 => "network device unavailable",
        99 => "IPv4 address not configured; run net dhcp",
        100 => "network link is down",
        105 => "network transmit ring is busy",
        110 => "network operation timed out",
        113 => "host unreachable (ARP did not resolve the next hop)",
        _ => "system call failed",
    }
}

pub fn read_line(fd: usize) -> Result<Option<String>, Error> {
    let mut bytes = [0u8; MAX_LINE_LENGTH];
    let count = eonos_rt::read(fd, &mut bytes)?;
    if count == 0 {
        return Ok(None);
    }
    if count == bytes.len() && bytes[count - 1] != b'\n' {
        return Err(Error::LineTooLong);
    }

    let mut end = count;
    if end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    String::from_utf8(bytes[..end].to_vec())
        .map(Some)
        .map_err(|_| Error::InvalidUtf8)
}

pub mod flags {
    pub const READ_ONLY: usize = eonos_rt::O_RDONLY;
    pub const WRITE_ONLY: usize = eonos_rt::O_WRONLY;
    pub const READ_WRITE: usize = eonos_rt::O_RDWR;
    pub const CREATE: usize = eonos_rt::O_CREAT;
    pub const TRUNCATE: usize = eonos_rt::O_TRUNC;
    pub const APPEND: usize = eonos_rt::O_APPEND;
}

pub struct File {
    fd: usize,
    closed: bool,
}

impl File {

    pub fn open(path: &str, open_flags: usize) -> Result<Self, Error> {
        Ok(Self {
            fd: eonos_rt::open(path, open_flags)?,
            closed: false,
        })
    }

    pub const fn fd(&self) -> usize {
        self.fd
    }

    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(eonos_rt::read(self.fd, buffer)?)
    }

    pub fn write(&mut self, buffer: &[u8]) -> Result<usize, Error> {
        Ok(eonos_rt::write(self.fd, buffer)?)
    }

    pub fn write_all(&mut self, mut buffer: &[u8]) -> Result<(), Error> {
        while !buffer.is_empty() {
            let count = self.write(buffer)?;
            if count == 0 {
                return Err(Error::System(eonos_rt::Error(5)));
            }
            buffer = &buffer[count..];
        }
        Ok(())
    }

    pub fn read_to_end(&mut self) -> Result<Vec<u8>, Error> {
        let mut contents = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let count = self.read(&mut chunk)?;
            if count == 0 {
                return Ok(contents);
            }
            contents.extend_from_slice(&chunk[..count]);
        }
    }

    pub fn read_to_string(&mut self) -> Result<String, Error> {
        String::from_utf8(self.read_to_end()?).map_err(|_| Error::InvalidUtf8)
    }

    pub fn close(mut self) -> Result<(), Error> {
        eonos_rt::close(self.fd)?;
        self.closed = true;
        Ok(())
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if !self.closed {
            let _ = eonos_rt::close(self.fd);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Process {
    pid: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitStatus {

    pub pid: usize,

    pub code: i32,
}

impl Process {

    pub fn spawn(path: &str, arguments: &[&str], environment: &[&str]) -> Result<Self, Error> {
        Self::spawn_with_stdio(path, arguments, environment, None, None)
    }

    pub fn spawn_with_stdio(
        path: &str,
        arguments: &[&str],
        environment: &[&str],
        stdin_fd: Option<usize>,
        stdout_fd: Option<usize>,
    ) -> Result<Self, Error> {
        let path_c = CString::new(path)?;
        let mut owned_args = Vec::with_capacity(arguments.len() + 1);
        owned_args.push(path_c.clone());
        for argument in arguments {
            owned_args.push(CString::new(*argument)?);
        }
        let owned_env: Vec<CString> = environment
            .iter()
            .map(|entry| CString::new(*entry))
            .collect::<core::result::Result<_, NulError>>()?;
        let argv: Vec<_> = owned_args.iter().map(|value| value.as_c_str()).collect();
        let env: Vec<_> = owned_env.iter().map(|value| value.as_c_str()).collect();
        let pid = eonos_rt::exec_with_stdio(path, &argv, &env, stdin_fd, stdout_fd)?;
        Ok(Self { pid })
    }

    pub fn spawn_sandboxed_with_stdio(
        path: &str,
        arguments: &[&str],
        environment: &[&str],
        stdin_fd: Option<usize>,
        stdout_fd: Option<usize>,
    ) -> Result<Self, Error> {
        let path_c = CString::new(path)?;
        let mut owned_args = Vec::with_capacity(arguments.len() + 1);
        owned_args.push(path_c.clone());
        for argument in arguments {
            owned_args.push(CString::new(*argument)?);
        }
        let owned_env: Vec<CString> = environment
            .iter()
            .map(|entry| CString::new(*entry))
            .collect::<core::result::Result<_, NulError>>()?;
        let argv: Vec<_> = owned_args.iter().map(|value| value.as_c_str()).collect();
        let env: Vec<_> = owned_env.iter().map(|value| value.as_c_str()).collect();
        let pid = eonos_rt::exec_sandboxed_with_stdio(path, &argv, &env, stdin_fd, stdout_fd)?;
        Ok(Self { pid })
    }

    pub const fn pid(&self) -> usize {
        self.pid
    }

    pub fn wait(self) -> Result<ExitStatus, Error> {
        let mut code = 0;
        let pid = eonos_rt::wait4(self.pid as isize, &mut code, 0)?;
        Ok(ExitStatus { pid, code })
    }

    pub fn try_wait(&self) -> Result<Option<ExitStatus>, Error> {
        let mut code = 0;
        let pid = eonos_rt::wait4(self.pid as isize, &mut code, 1)?;
        Ok((pid != 0).then_some(ExitStatus { pid, code }))
    }

    pub fn signal(&self, signal: usize) -> Result<(), Error> {
        Ok(eonos_rt::kill(self.pid, signal)?)
    }
}

pub fn print(text: &str) -> Result<(), Error> {
    eonos_rt::write_all(1, text.as_bytes())?;
    Ok(())
}

pub fn chdir(path: &str) -> Result<(), Error> {
    Ok(eonos_rt::chdir(path)?)
}

pub fn current_dir() -> Result<String, Error> {
    let mut bytes = [0u8; 256];
    let count = eonos_rt::getcwd(&mut bytes)?;
    String::from_utf8(bytes[..count].to_vec()).map_err(|_| Error::InvalidUtf8)
}

pub fn eprint(text: &str) -> Result<(), Error> {
    eonos_rt::write_all(2, text.as_bytes())?;
    Ok(())
}

pub fn list_files() -> Result<String, Error> {
    read_listing(eonos_rt::list_files)
}

pub fn list_directory(path: &str) -> Result<String, Error> {
    let mut bytes = [0u8; 4096];
    let count = eonos_rt::list_directory(path, &mut bytes)?;
    String::from_utf8(bytes[..count].to_vec()).map_err(|_| Error::InvalidUtf8)
}

pub fn list_processes() -> Result<String, Error> {
    read_listing(eonos_rt::list_processes)
}

pub fn list_block_devices() -> Result<String, Error> {
    read_listing(eonos_rt::list_block_devices)
}

pub fn list_mounts() -> Result<String, Error> {
    read_listing(eonos_rt::list_mounts)
}

fn read_listing(list: fn(&mut [u8]) -> eonos_rt::Result<usize>) -> Result<String, Error> {
    let mut bytes = [0u8; 4096];
    let count = list(&mut bytes)?;
    String::from_utf8(bytes[..count].to_vec()).map_err(|_| Error::InvalidUtf8)
}

pub fn mkdir(path: &str) -> Result<(), Error> {
    Ok(eonos_rt::mkdir(path)?)
}

pub fn remove(path: &str) -> Result<(), Error> {
    Ok(eonos_rt::remove(path)?)
}

pub fn mount(device: &str, mountpoint: &str) -> Result<(), Error> {
    let device = CString::new(device)?;
    let mountpoint = CString::new(mountpoint)?;
    Ok(eonos_rt::mount(device.as_c_str(), mountpoint.as_c_str())?)
}

pub fn is_mountpoint(path: &str) -> Result<bool, Error> {
    let path = CString::new(path)?;
    Ok(eonos_rt::is_mountpoint(path.as_c_str())?)
}

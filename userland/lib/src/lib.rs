#![no_std]

//! Small safe convenience layer for EonOS Rust user programs.

extern crate alloc;

use alloc::ffi::{CString, NulError};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Maximum line length accepted by [`read_line`].
pub const MAX_LINE_LENGTH: usize = 4096;

/// Error returned by this library.
#[derive(Debug)]
pub enum Error {
    /// An EonOS syscall failed; the value is its positive errno.
    System(eonos_rt::Error),
    /// A string passed to process execution contained an interior NUL byte.
    InteriorNul,
    /// A byte buffer could not be represented as UTF-8.
    InvalidUtf8,
    /// The input line exceeded [`MAX_LINE_LENGTH`] bytes.
    LineTooLong,
    /// The shell command had invalid quoting or escaping.
    InvalidInput,
    /// A command argument had an invalid value.
    InvalidArgument,
    /// The shell command is missing an argument.
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

/// Read one UTF-8 line from a descriptor, removing its trailing newline.
///
/// EonOS terminal input is canonical, so a read from descriptor zero returns
/// one complete line. Lines longer than [`MAX_LINE_LENGTH`] are rejected.
/// Returns `Ok(None)` when the descriptor reaches end of file.
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

/// File open flags supported by EonOS.
pub mod flags {
    pub const READ_ONLY: usize = eonos_rt::O_RDONLY;
    pub const WRITE_ONLY: usize = eonos_rt::O_WRONLY;
    pub const READ_WRITE: usize = eonos_rt::O_RDWR;
    pub const CREATE: usize = eonos_rt::O_CREAT;
    pub const TRUNCATE: usize = eonos_rt::O_TRUNC;
    pub const APPEND: usize = eonos_rt::O_APPEND;
}

/// An open file descriptor. Dropping it closes the descriptor.
pub struct File {
    fd: usize,
    closed: bool,
}

impl File {
    /// Open a file with EonOS open flags.
    pub fn open(path: &str, open_flags: usize) -> Result<Self, Error> {
        Ok(Self {
            fd: eonos_rt::open(path, open_flags)?,
            closed: false,
        })
    }

    /// Return the underlying process file descriptor.
    pub const fn fd(&self) -> usize {
        self.fd
    }

    /// Read up to the length of `buffer` bytes.
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(eonos_rt::read(self.fd, buffer)?)
    }

    /// Write up to the length of `buffer` bytes.
    pub fn write(&mut self, buffer: &[u8]) -> Result<usize, Error> {
        Ok(eonos_rt::write(self.fd, buffer)?)
    }

    /// Write all bytes, handling short writes.
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

    /// Read the remaining file contents into a byte vector.
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

    /// Read the remaining file contents and validate them as UTF-8.
    pub fn read_to_string(&mut self) -> Result<String, Error> {
        String::from_utf8(self.read_to_end()?).map_err(|_| Error::InvalidUtf8)
    }

    /// Close the descriptor and report a close error, if any.
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

/// A child process that can be waited on.
#[derive(Clone, Copy, Debug)]
pub struct Process {
    pid: usize,
}

/// Exit information returned after a child process finishes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitStatus {
    /// Process ID of the child that exited.
    pub pid: usize,
    /// Exit code passed to the process exit syscall.
    pub code: i32,
}

impl Process {
    /// Start `path` with the supplied arguments and environment.
    ///
    /// `argv[0]` is set to `path`; `arguments` become the remaining entries.
    pub fn spawn(path: &str, arguments: &[&str], environment: &[&str]) -> Result<Self, Error> {
        Self::spawn_with_stdio(path, arguments, environment, None, None)
    }

    /// Start a child and optionally connect its stdin and stdout to open
    /// descriptors owned by the calling process.
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

    /// Start a child with syscall restrictions suitable for a third-party app.
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

    /// Return the process ID.
    pub const fn pid(&self) -> usize {
        self.pid
    }

    /// Wait for this child and collect its exit code.
    pub fn wait(self) -> Result<ExitStatus, Error> {
        let mut code = 0;
        let pid = eonos_rt::wait4(self.pid as isize, &mut code, 0)?;
        Ok(ExitStatus { pid, code })
    }

    /// Check for completion without blocking. A completed child's status is reaped.
    pub fn try_wait(&self) -> Result<Option<ExitStatus>, Error> {
        let mut code = 0;
        let pid = eonos_rt::wait4(self.pid as isize, &mut code, 1)?;
        Ok((pid != 0).then_some(ExitStatus { pid, code }))
    }

    /// Send a signal to this child process.
    pub fn signal(&self, signal: usize) -> Result<(), Error> {
        Ok(eonos_rt::kill(self.pid, signal)?)
    }
}

/// Write a complete string to standard output.
pub fn print(text: &str) -> Result<(), Error> {
    eonos_rt::write_all(1, text.as_bytes())?;
    Ok(())
}

/// Change the calling process's working directory.
pub fn chdir(path: &str) -> Result<(), Error> {
    Ok(eonos_rt::chdir(path)?)
}

/// Return the calling process's current working directory.
pub fn current_dir() -> Result<String, Error> {
    let mut bytes = [0u8; 256];
    let count = eonos_rt::getcwd(&mut bytes)?;
    String::from_utf8(bytes[..count].to_vec()).map_err(|_| Error::InvalidUtf8)
}

/// Write a complete string to standard error.
pub fn eprint(text: &str) -> Result<(), Error> {
    eonos_rt::write_all(2, text.as_bytes())?;
    Ok(())
}

/// Return the newline-separated VFS path listing.
pub fn list_files() -> Result<String, Error> {
    read_listing(eonos_rt::list_files)
}

/// Return the newline-separated entries in a directory.
pub fn list_directory(path: &str) -> Result<String, Error> {
    let mut bytes = [0u8; 4096];
    let count = eonos_rt::list_directory(path, &mut bytes)?;
    String::from_utf8(bytes[..count].to_vec()).map_err(|_| Error::InvalidUtf8)
}

/// Return the process table.
pub fn list_processes() -> Result<String, Error> {
    read_listing(eonos_rt::list_processes)
}

/// Return the block device table.
pub fn list_block_devices() -> Result<String, Error> {
    read_listing(eonos_rt::list_block_devices)
}

/// Return active mountpoints, filesystem types, and their sources.
pub fn list_mounts() -> Result<String, Error> {
    read_listing(eonos_rt::list_mounts)
}

fn read_listing(list: fn(&mut [u8]) -> eonos_rt::Result<usize>) -> Result<String, Error> {
    let mut bytes = [0u8; 4096];
    let count = list(&mut bytes)?;
    String::from_utf8(bytes[..count].to_vec()).map_err(|_| Error::InvalidUtf8)
}

/// Create a directory at `path`.
pub fn mkdir(path: &str) -> Result<(), Error> {
    Ok(eonos_rt::mkdir(path)?)
}

/// Remove a file or empty directory at `path`.
pub fn remove(path: &str) -> Result<(), Error> {
    Ok(eonos_rt::remove(path)?)
}

/// Mount `device` at `mountpoint`.
pub fn mount(device: &str, mountpoint: &str) -> Result<(), Error> {
    let device = CString::new(device)?;
    let mountpoint = CString::new(mountpoint)?;
    Ok(eonos_rt::mount(device.as_c_str(), mountpoint.as_c_str())?)
}

/// Return whether a writable filesystem is mounted exactly at `path`.
pub fn is_mountpoint(path: &str) -> Result<bool, Error> {
    let path = CString::new(path)?;
    Ok(eonos_rt::is_mountpoint(path.as_c_str())?)
}

#![no_std]

//! Safe, small application SDK for EonOS userland programs.
//!
//! All drawing and event traffic for Era windows goes through bounded pipes
//! owned by Era. Applications never receive a framebuffer pointer or kernel
//! address.

extern crate alloc;

pub mod era;
pub mod system;

/// Arguments passed to a program's entry function.
pub use eonos_rt::Args;
pub use eonos_rt::entry;
pub use eonos_rt::{eprint, eprintln, print, println};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AppType {
    Shell = 1,
    Era = 2,
}

/// Declare the package kind in the ELF and install the EonOS program entry.
/// The app packager checks this embedded marker against Cargo metadata.
#[macro_export]
macro_rules! app {
    (SHELL, $main:path) => {
        #[used]
        #[link_section = ".eonos_app"]
        static EONOS_APP_TYPE_MARKER: [u8; 16] = *b"EONOSAPP1:SHELL\0";
        $crate::entry!($main);
    };
    (ERA, $main:path) => {
        #[used]
        #[link_section = ".eonos_app"]
        static EONOS_APP_TYPE_MARKER: [u8; 14] = *b"EONOSAPP1:ERA\0";
        $crate::entry!($main);
    };
}

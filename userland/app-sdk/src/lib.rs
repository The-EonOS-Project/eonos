#![no_std]

extern crate alloc;

pub mod era;
pub mod system;
pub mod web;

pub use eonos_rt::Args;
pub use eonos_rt::entry;
pub use eonos_rt::{eprint, eprintln, print, println};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AppType {
    Shell = 1,
    Era = 2,
}

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

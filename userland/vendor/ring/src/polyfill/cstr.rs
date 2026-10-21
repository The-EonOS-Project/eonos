#![cfg(all(
    target_vendor = "apple",
    any(
        target_os = "ios",
        target_os = "macos",
        target_os = "tvos",
        target_os = "visionos",
        target_os = "watchos"
    )
))]

#![cfg(all(
    all(target_arch = "aarch64", target_endian = "little"),
    target_vendor = "apple"
))]

use core::mem::{align_of, size_of};

use libc::c_char;

pub struct Ref(&'static [u8]);

impl Ref {
    #[inline(always)]
    pub fn as_ptr(&self) -> *const c_char {
        const _SAME_ALIGNMENT: () = assert!(align_of::<u8>() == align_of::<c_char>());
        const _SAME_SIZE: () = assert!(size_of::<u8>() == size_of::<c_char>());

        self.0.as_ptr().cast()
    }

    const unsafe fn from_bytes_with_nul_unchecked(value: &'static [u8]) -> Self {
        Self(value)
    }
}

pub const fn unwrap_const_from_bytes_with_nul(value: &'static [u8]) -> Ref {

    match const_from_bytes_with_nul(value) {
        Some(r) => r,
        None => panic!("const_from_bytes_with_nul failed"),
    }
}

#[inline(always)]
const fn const_from_bytes_with_nul(value: &'static [u8]) -> Option<Ref> {
    const fn const_contains(mut value: &[u8], needle: &u8) -> bool {
        while let [head, tail @ ..] = value {
            if *head == *needle {
                return true;
            }
            value = tail;
        }
        false
    }

    match value {
        [before_nul @ .., 0] if !const_contains(before_nul, &0) => {

            Some(unsafe { Ref::from_bytes_with_nul_unchecked(value) })
        }
        _ => None,
    }
}

mod tests {
    use super::const_from_bytes_with_nul;

    const _EMPTY_UNTERMINATED: () = assert!(const_from_bytes_with_nul(b"").is_none());
    const _EMPTY_DOUBLE_TERMINATED: () = assert!(const_from_bytes_with_nul(b"\0\0").is_none());
    const _DOUBLE_NUL: () = assert!(const_from_bytes_with_nul(b"\0\0").is_none());
    const _LEADINGL_NUL: () = assert!(const_from_bytes_with_nul(b"\0a\0").is_none());
    const _INTERNAL_NUL_UNTERMINATED: () = assert!(const_from_bytes_with_nul(b"\0a").is_none());

    const _EMPTY_TERMINATED: () = assert!(const_from_bytes_with_nul(b"\0").is_some());
    const _NONEMPTY: () = assert!(const_from_bytes_with_nul(b"asdf\0").is_some());
    const _1_CHAR: () = assert!(const_from_bytes_with_nul(b"a\0").is_some());
}

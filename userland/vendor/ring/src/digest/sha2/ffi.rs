use super::CHAINING_WORDS;
use crate::polyfill::slice::AsChunks;
use core::num::{NonZeroUsize, Wrapping};

macro_rules! sha2_ffi {
    ( $U:ty, $BLOCK_LEN:expr, unsafe { $Cpu:ty => $f:ident },
      $state:expr, $data:expr, $cpu:expr $(,)? ) => {{
        prefixed_extern! {
            fn $f(
                state: *mut [core::num::Wrapping<$U>; crate::digest::sha2::CHAINING_WORDS],
                data: *const [u8; $BLOCK_LEN],
                num: crate::c::NonZero_size_t);
        }

        unsafe {
            crate::digest::sha2::ffi::sha2_ffi::<$U, $Cpu, { $BLOCK_LEN }>($state, $data, $cpu, $f)
        }
    }};
}

macro_rules! sha2_32_ffi {
    ( unsafe { $Cpu:ty => $f:ident }, $state:expr, $data:expr, $cpu:expr $(,)? ) => {
        sha2_ffi!(u32, crate::digest::sha2::SHA256_BLOCK_LEN.into(),
                  unsafe { $Cpu => $f }, $state, $data, $cpu)
    }
}

macro_rules! sha2_64_ffi {
    ( unsafe { $Cpu:ty => $f:ident }, $state:expr, $data:expr, $cpu:expr $(,)? ) => {
        sha2_ffi!(u64, SHA512_BLOCK_LEN.into(), unsafe { $Cpu => $f }, $state, $data, $cpu)
    }
}

pub(super) unsafe fn sha2_ffi<U, Cpu, const BLOCK_LEN: usize>(
    state: &mut [Wrapping<U>; CHAINING_WORDS],
    data: AsChunks<u8, BLOCK_LEN>,
    cpu: Cpu,
    f: unsafe extern "C" fn(
        *mut [Wrapping<U>; CHAINING_WORDS],
        *const [u8; BLOCK_LEN],
        crate::c::NonZero_size_t,
    ),
) {
    if let Some(blocks) = NonZeroUsize::new(data.len()) {
        let data = data.as_ptr();
        let _: Cpu = cpu;

        unsafe { f(state, data, blocks) }
    }
}

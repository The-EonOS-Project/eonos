use super::{Block, KeyBytes, Overlapping, BLOCK_LEN};
use crate::{bits::BitLength, c, error};
use core::{
    ffi::{c_int, c_uint},
    num::{NonZeroU32, NonZeroUsize},
};

#[repr(transparent)]
pub(in super::super) struct Counter(pub(super) [u8; BLOCK_LEN]);

#[repr(C)]
#[derive(Clone)]
pub(in super::super) struct AES_KEY {
    pub rd_key: [u32; 4 * (MAX_ROUNDS + 1)],
    pub rounds: c_uint,
}

const MAX_ROUNDS: usize = 14;

impl AES_KEY {
    #[inline]
    pub(super) unsafe fn new(
        f: unsafe extern "C" fn(*const u8, BitLength<c_int>, *mut AES_KEY) -> c_int,
        bytes: KeyBytes<'_>,
    ) -> Result<Self, error::Unspecified> {
        let mut key = Self {
            rd_key: [0; 4 * (MAX_ROUNDS + 1)],
            rounds: 0,
        };

        let (bytes, key_bits) = match bytes {
            KeyBytes::AES_128(bytes) => (&bytes[..], BitLength::from_bits(128)),
            KeyBytes::AES_256(bytes) => (&bytes[..], BitLength::from_bits(256)),
        };

        if 0 == unsafe { f(bytes.as_ptr(), key_bits, &mut key) } {
            debug_assert_ne!(key.rounds, 0); 
            Ok(key)
        } else {
            Err(error::Unspecified)
        }
    }
}

#[cfg(all(target_arch = "arm", target_endian = "little"))]
impl AES_KEY {
    pub(super) unsafe fn derive(
        f: for<'a> unsafe extern "C" fn(*mut AES_KEY, &'a AES_KEY),
        src: &Self,
    ) -> Self {
        let mut r = AES_KEY {
            rd_key: [0u32; 4 * (MAX_ROUNDS + 1)],
            rounds: 0,
        };
        unsafe { f(&mut r, src) };
        r
    }

    pub(super) fn rounds(&self) -> u32 {
        self.rounds
    }
}

macro_rules! set_encrypt_key {
    ( $name:ident, $key_bytes:expr $(,)? ) => {{
        use crate::bits::BitLength;
        use core::ffi::c_int;
        prefixed_extern! {
            fn $name(user_key: *const u8, bits: BitLength<c_int>, key: *mut AES_KEY) -> c_int;
        }
        $crate::aead::aes::ffi::AES_KEY::new($name, $key_bytes)
    }};
}

macro_rules! encrypt_block {
    ($name:ident, $block:expr, $key:expr) => {{
        use crate::aead::aes::{ffi::AES_KEY, Block};
        prefixed_extern! {
            fn $name(a: &Block, r: *mut Block, key: &AES_KEY);
        }
        $key.encrypt_block($name, $block)
    }};
}

impl AES_KEY {
    #[inline]
    pub(super) unsafe fn encrypt_block(
        &self,
        f: unsafe extern "C" fn(&Block, *mut Block, &AES_KEY),
        a: Block,
    ) -> Block {
        let mut result = core::mem::MaybeUninit::uninit();
        unsafe {
            f(&a, result.as_mut_ptr(), self);
            result.assume_init()
        }
    }
}

macro_rules! ctr32_encrypt_blocks {
    ($name:ident, $in_out:expr, $key:expr, $ctr:expr $(,)? ) => {{
        use crate::{
            aead::aes::{ffi::AES_KEY, Counter, BLOCK_LEN},
            c,
        };
        prefixed_extern! {
            fn $name(
                input: *const [u8; BLOCK_LEN],
                output: *mut [u8; BLOCK_LEN],
                blocks: c::NonZero_size_t,
                key: &AES_KEY,
                ivec: &Counter,
            );
        }
        $key.ctr32_encrypt_blocks($name, $in_out, $ctr)
    }};
}

impl AES_KEY {

    #[inline]
    pub(super) unsafe fn ctr32_encrypt_blocks(
        &self,
        f: unsafe extern "C" fn(
            input: *const [u8; BLOCK_LEN],
            output: *mut [u8; BLOCK_LEN],
            blocks: c::NonZero_size_t,
            key: &AES_KEY,
            ivec: &Counter,
        ),
        in_out: Overlapping<'_>,
        ctr: &mut Counter,
    ) {
        in_out.with_input_output_len(|input, output, len| {
            debug_assert_eq!(len % BLOCK_LEN, 0);

            let blocks = match NonZeroUsize::new(len / BLOCK_LEN) {
                Some(blocks) => blocks,
                None => {
                    return;
                }
            };

            let input: *const [u8; BLOCK_LEN] = input.cast();
            let output: *mut [u8; BLOCK_LEN] = output.cast();
            let blocks_u32: NonZeroU32 = blocks.try_into().unwrap();

            unsafe {
                f(input, output, blocks, self, ctr);
            }

            ctr.increment_by_less_safe(blocks_u32);
        });
    }
}

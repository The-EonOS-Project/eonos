#![cfg(all(target_arch = "aarch64", target_endian = "little"))]

use super::{aes, gcm, Counter, BLOCK_LEN};
use crate::{aead::aes::Overlapping, bits::BitLength, polyfill::slice::AsChunksMut};
use core::num::NonZeroU64;

pub(super) fn seal_whole(
    aes_key: &aes::hw::Key,
    auth: &mut gcm::Context<gcm::clmul::Key>,
    ctr: &mut Counter,
    mut in_out: AsChunksMut<u8, BLOCK_LEN>,
) {
    let whole_block_bits = auth.in_out_whole_block_bits();
    let whole_block_bits_u64: BitLength<u64> = whole_block_bits.into();
    if let Ok(whole_block_bits) = whole_block_bits_u64.try_into() {
        let (htable, xi) = auth.inner();

        prefixed_extern! {
            fn aes_gcm_enc_kernel(
                input: *const [u8; BLOCK_LEN],
                in_bits: BitLength<NonZeroU64>,
                output: *mut [u8; BLOCK_LEN],
                Xi: &mut gcm::Xi,
                ivec: &mut Counter,
                key: &aes::AES_KEY,
                Htable: &gcm::HTable);
        }

        unsafe {
            aes_gcm_enc_kernel(
                in_out.as_ptr(),
                whole_block_bits,
                in_out.as_mut_ptr(),
                xi,
                ctr,
                aes_key.inner_less_safe(),
                htable,
            )
        }
    }
}

pub(super) fn open_whole(
    aes_key: &aes::hw::Key,
    auth: &mut gcm::Context<gcm::clmul::Key>,
    in_out: Overlapping,
    ctr: &mut Counter,
) {

    assert_eq!(in_out.len() % BLOCK_LEN, 0);

    in_out.with_input_output_len(|input, output, _len| {
        let whole_block_bits = auth.in_out_whole_block_bits();
        let whole_block_bits_u64: BitLength<u64> = whole_block_bits.into();
        if let Ok(whole_block_bits) = whole_block_bits_u64.try_into() {
            let (htable, xi) = auth.inner();
            prefixed_extern! {
                fn aes_gcm_dec_kernel(
                    input: *const u8,
                    in_bits: BitLength<NonZeroU64>,
                    output: *mut u8,
                    Xi: &mut gcm::Xi,
                    ivec: &mut Counter,
                    key: &aes::AES_KEY,
                    Htable: &gcm::HTable);
            }

            unsafe {
                aes_gcm_dec_kernel(
                    input,
                    whole_block_bits,
                    output,
                    xi,
                    ctr,
                    aes_key.inner_less_safe(),
                    htable,
                )
            }
        }
    })
}

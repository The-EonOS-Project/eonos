













#![cfg(target_arch = "x86_64")]

use super::{aes, gcm, Counter, BLOCK_LEN};
use crate::{aead::aes::Overlapping, c, polyfill::slice::AsChunksMut};
use core::num::NonZeroU32;

pub(super) fn seal_whole_vaes_clmul_avx2(
    aes_key: &aes::hw::Key,
    auth: &mut gcm::Context<gcm::vclmulavx2::Key>,
    ctr: &mut Counter,
    mut in_out: AsChunksMut<u8, BLOCK_LEN>,
) {
    prefixed_extern! {
        fn aes_gcm_enc_update_vaes_avx2(
            input: *const u8,
            output: *mut u8,
            len: c::size_t,
            key: &aes::AES_KEY,
            ivec: &Counter,
            Htable: &gcm::HTable,
            Xi: &mut gcm::Xi);
    }

    let in_out = in_out.as_flattened_mut();

    
    
    let blocks = u32::try_from(in_out.len() / BLOCK_LEN).unwrap();

    if let Some(blocks) = NonZeroU32::new(blocks) {
        let aes_key = aes_key.inner_less_safe();
        let (htable, xi) = auth.inner();
        let input = in_out.as_ptr();
        let output = in_out.as_mut_ptr();
        let len = in_out.len();
        unsafe { aes_gcm_enc_update_vaes_avx2(input, output, len, aes_key, ctr, htable, xi) };
        ctr.increment_by_less_safe(blocks);
    }
}

pub(super) fn open_whole_vaes_clmul_avx2(
    aes_key: &aes::hw::Key,
    auth: &mut gcm::Context<gcm::vclmulavx2::Key>,
    in_out: Overlapping,
    ctr: &mut Counter,
) {
    prefixed_extern! {
        fn aes_gcm_dec_update_vaes_avx2(
            input: *const u8,
            output: *mut u8,
            len: c::size_t,
            key: &aes::AES_KEY,
            ivec: &mut Counter,
            Htable: &gcm::HTable,
            Xi: &mut gcm::Xi);
    }

    
    assert_eq!(in_out.len() % BLOCK_LEN, 0);
    
    
    let blocks = u32::try_from(in_out.len() / BLOCK_LEN).unwrap();

    if let Some(blocks) = NonZeroU32::new(blocks) {
        let aes_key = aes_key.inner_less_safe();
        let (htable, xi) = auth.inner();
        in_out.with_input_output_len(|input, output, len| unsafe {
            aes_gcm_dec_update_vaes_avx2(input, output, len, aes_key, ctr, htable, xi)
        });
        ctr.increment_by_less_safe(blocks);
    }
}

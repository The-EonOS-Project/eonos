#![cfg(target_arch = "x86_64")]

use super::{
    super::overlapping::IndexError,
    aes::{self, Counter, EncryptCtr32, Overlapping, OverlappingPartialBlock},
    gcm, Aad, Tag,
};
use crate::{
    c,
    error::{self, InputTooLongError},
    polyfill::slice,
};
use core::ops::RangeFrom;

#[inline(never)]
pub(super) fn seal(
    aes_key: &aes::hw::Key,
    gcm_key: &gcm::clmulavxmovbe::Key,
    mut ctr: Counter,
    tag_iv: aes::Iv,
    aad: Aad<&[u8]>,
    in_out: &mut [u8],
) -> Result<Tag, error::Unspecified> {
    prefixed_extern! {

        fn aesni_gcm_encrypt(
            input: *const u8,
            output: *mut u8,
            len: c::size_t,
            key: &aes::AES_KEY,
            ivec: &mut Counter,
            Htable: &gcm::HTable,
            Xi: &mut gcm::Xi) -> c::size_t;
    }

    let mut auth = gcm::Context::new(gcm_key, aad, in_out.len())?;
    let (htable, xi) = auth.inner();

    let processed = unsafe {
        aesni_gcm_encrypt(
            in_out.as_ptr(),
            in_out.as_mut_ptr(),
            in_out.len(),
            aes_key.inner_less_safe(),
            &mut ctr,
            htable,
            xi,
        )
    };

    let ramaining = match in_out.get_mut(processed..) {
        Some(remaining) => remaining,
        None => {

            unreachable!()
        }
    };
    let (mut whole, remainder) = slice::as_chunks_mut(ramaining);
    aes_key.ctr32_encrypt_within(whole.as_flattened_mut().into(), &mut ctr);
    auth.update_blocks(whole.as_ref());
    let remainder = OverlappingPartialBlock::new(remainder.into())
        .unwrap_or_else(|InputTooLongError { .. }| unreachable!());

    super::seal_finish(aes_key, auth, remainder, ctr, tag_iv)
}

#[inline(never)]
pub(super) fn open(
    aes_key: &aes::hw::Key,
    gcm_key: &gcm::clmulavxmovbe::Key,
    mut ctr: Counter,
    tag_iv: aes::Iv,
    aad: Aad<&[u8]>,
    in_out_slice: &mut [u8],
    src: RangeFrom<usize>,
) -> Result<Tag, error::Unspecified> {
    prefixed_extern! {

        fn aesni_gcm_decrypt(
            input: *const u8,
            output: *mut u8,
            len: c::size_t,
            key: &aes::AES_KEY,
            ivec: &mut Counter,
            Htable: &gcm::HTable,
            Xi: &mut gcm::Xi) -> c::size_t;
    }

    let in_out = Overlapping::new(in_out_slice, src.clone()).map_err(error::erase::<IndexError>)?;
    let mut auth = gcm::Context::new(gcm_key, aad, in_out.len())?;
    let processed = in_out.with_input_output_len(|input, output, len| {
        let (htable, xi) = auth.inner();
        unsafe {
            aesni_gcm_decrypt(
                input,
                output,
                len,
                aes_key.inner_less_safe(),
                &mut ctr,
                htable,
                xi,
            )
        }
    });
    let in_out_slice = in_out_slice.get_mut(processed..).unwrap_or_else(|| {

        unreachable!()
    });

    let in_out =
        Overlapping::new(in_out_slice, src.clone()).unwrap_or_else(|IndexError { .. }| {

            unreachable!()
        });
    let (whole, _) = slice::as_chunks(in_out.input());
    auth.update_blocks(whole);

    let whole_len = whole.as_flattened().len();

    let whole = Overlapping::new(&mut in_out_slice[..(src.start + whole_len)], src.clone())
        .map_err(error::erase::<IndexError>)?;
    aes_key.ctr32_encrypt_within(whole, &mut ctr);

    let in_out_slice = match in_out_slice.get_mut(whole_len..) {
        Some(partial) => partial,
        None => unreachable!(),
    };
    let in_out =
        Overlapping::new(in_out_slice, src).unwrap_or_else(|IndexError { .. }| unreachable!());
    let in_out = OverlappingPartialBlock::new(in_out)
        .unwrap_or_else(|InputTooLongError { .. }| unreachable!());

    super::open_finish(aes_key, auth, in_out, ctr, tag_iv)
}

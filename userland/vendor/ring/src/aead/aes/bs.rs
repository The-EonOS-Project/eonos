#![cfg(all(target_arch = "arm", target_endian = "little"))]

use super::{Counter, Overlapping, AES_KEY};

pub(super) unsafe fn ctr32_encrypt_blocks_with_vpaes_key(
    in_out: Overlapping<'_>,
    vpaes_key: &AES_KEY,
    ctr: &mut Counter,
) {
    prefixed_extern! {

        fn vpaes_encrypt_key_to_bsaes(bsaes_key: *mut AES_KEY, vpaes_key: &AES_KEY);
    }

    let bsaes_key = unsafe { AES_KEY::derive(vpaes_encrypt_key_to_bsaes, vpaes_key) };

    debug_assert_eq!(bsaes_key.rounds(), vpaes_key.rounds() + 1);

    unsafe {
        ctr32_encrypt_blocks!(bsaes_ctr32_encrypt_blocks, in_out, &bsaes_key, ctr);
    }
}

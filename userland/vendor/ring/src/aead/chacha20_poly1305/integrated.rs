use super::{
    super::{NONCE_LEN, TAG_LEN},
    chacha::Overlapping,
    check_input_lengths, Aad, InputTooLongError, Key, Nonce, Tag, KEY_LEN,
};
use cfg_if::cfg_if;

macro_rules! declare_open {
    ( $name:ident ) => {
        prefixed_extern! {
            fn $name(
                out_plaintext: *mut u8,
                ciphertext: *const u8,
                plaintext_len: usize,
                ad: *const u8,
                ad_len: usize,
                data: &mut InOut<open_data_in>,
            );
        }
    };
}

macro_rules! declare_seal {
    ( $name:ident ) => {
        prefixed_extern! {
            fn $name(
                out_ciphertext: *mut u8,
                plaintext: *const u8,
                plaintext_len: usize,
                ad: *const u8,
                ad_len: usize,
                data: &mut InOut<seal_data_in>,
            );
        }
    };
}

cfg_if! {
    if #[cfg(all(target_arch = "aarch64", target_endian = "little"))] {
        use crate::cpu::arm::Neon;
        type RequiredCpuFeatures = Neon;
        type OptionalCpuFeatures = ();
    } else {
        use crate::cpu::intel::{Avx2, Bmi2, Sse41};
        type RequiredCpuFeatures = Sse41;
        type OptionalCpuFeatures = (Avx2, Bmi2);
    }
}

pub(super) fn seal(
    Key(key): &Key,
    nonce: Nonce,
    aad: Aad<&[u8]>,
    in_out: &mut [u8],
    required_cpu_features: RequiredCpuFeatures,
    optional_cpu_features: Option<OptionalCpuFeatures>,
) -> Result<Tag, InputTooLongError> {
    check_input_lengths(aad, in_out)?;

    #[repr(align(16), C)]
    #[derive(Clone, Copy)]
    struct seal_data_in {
        key: [u32; KEY_LEN / 4],
        counter: u32,
        nonce: [u8; NONCE_LEN],
        extra_ciphertext: *const u8,
        extra_ciphertext_len: usize,
    }

    let mut data = InOut {
        input: seal_data_in {
            key: *key.words_less_safe(),
            counter: 0,
            nonce: *nonce.as_ref(),
            extra_ciphertext: core::ptr::null(),
            extra_ciphertext_len: 0,
        },
    };

    let output = in_out.as_mut_ptr();
    let input = in_out.as_ptr();
    let len = in_out.len();
    let ad = aad.as_ref().as_ptr();
    let ad_len = aad.as_ref().len();

    #[allow(clippy::needless_late_init)]
    let tag;

    cfg_if! {
        if #[cfg(all(target_arch = "aarch64", target_endian = "little"))] {
            declare_seal! { chacha20_poly1305_seal }
            let _: Neon = required_cpu_features;
            let _: Option<()> = optional_cpu_features;
            tag = unsafe {
                chacha20_poly1305_seal(output, input, len, ad, ad_len, &mut data);
                &data.out.tag
            };
        } else {
            let _: Sse41 = required_cpu_features;
            if matches!(optional_cpu_features, Some((Avx2 { .. }, Bmi2 { .. }))) {
                declare_seal! { chacha20_poly1305_seal_avx2 }
                tag = unsafe {
                    chacha20_poly1305_seal_avx2(output, input, len, ad, ad_len, &mut data);
                    &data.out.tag
                };
            } else {
                declare_seal! { chacha20_poly1305_seal_sse41 }
                tag = unsafe {
                    chacha20_poly1305_seal_sse41(output, input, len, ad, ad_len, &mut data);
                    &data.out.tag
                };
            }
        }
    }

    Ok(Tag(*tag))
}

pub(super) fn open(
    Key(key): &Key,
    nonce: Nonce,
    aad: Aad<&[u8]>,
    in_out: Overlapping<'_>,
    required_cpu_features: RequiredCpuFeatures,
    optional_cpu_features: Option<OptionalCpuFeatures>,
) -> Result<Tag, InputTooLongError> {
    check_input_lengths(aad, in_out.input())?;

    #[derive(Copy, Clone)]
    #[repr(align(16), C)]
    struct open_data_in {
        key: [u32; KEY_LEN / 4],
        counter: u32,
        nonce: [u8; NONCE_LEN],
    }

    let mut data = InOut {
        input: open_data_in {
            key: *key.words_less_safe(),
            counter: 0,
            nonce: *nonce.as_ref(),
        },
    };

    in_out.with_input_output_len(|input, output, len| {
        let ad = aad.as_ref().as_ptr();
        let ad_len = aad.as_ref().len();

        #[allow(clippy::needless_late_init)]
        let tag;

        cfg_if! {
            if #[cfg(all(target_arch = "aarch64", target_endian = "little"))] {
                declare_open! { chacha20_poly1305_open }
                let _: Neon = required_cpu_features;
                let _: Option<()> = optional_cpu_features;
                tag = unsafe {
                    chacha20_poly1305_open(output, input, len, ad, ad_len, &mut data);
                    &data.out.tag
                };
            } else {
                let _: Sse41 = required_cpu_features;
                if matches!(optional_cpu_features, Some((Avx2 { .. }, Bmi2 { .. }))) {
                    declare_open! { chacha20_poly1305_open_avx2 }
                    tag = unsafe {
                        chacha20_poly1305_open_avx2(output, input, len, ad, ad_len, &mut data);
                        &data.out.tag
                    };
                } else {
                    declare_open! { chacha20_poly1305_open_sse41 }
                    tag = unsafe {
                        chacha20_poly1305_open_sse41(output, input, len, ad, ad_len, &mut data);
                        &data.out.tag
                    };
                }
            }
        }

        Ok(Tag(*tag))
    })
}

#[repr(C)]
pub(super) union InOut<T>
where
    T: Copy,
{
    pub(super) input: T,
    pub(super) out: Out,
}

#[derive(Clone, Copy)]
#[repr(align(16), C)]
pub(super) struct Out {
    pub(super) tag: [u8; TAG_LEN],
}

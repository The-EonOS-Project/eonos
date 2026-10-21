















use crate::{c, error};
use core::{ffi::c_int, num::NonZeroUsize};

mod boolmask;
mod leaky;
mod word;

pub(crate) use self::{boolmask::BoolMask, leaky::LeakyWord, word::Word};


pub fn verify_slices_are_equal(a: &[u8], b: &[u8]) -> Result<(), error::Unspecified> {
    let len = a.len(); 
    if b.len() != len {
        return Err(error::Unspecified);
    }
    match NonZeroUsize::new(len) {
        Some(len) => {
            let a = a.as_ptr();
            let b = b.as_ptr();
            
            
            let result = unsafe { CRYPTO_memcmp(a, b, len) };
            match result {
                0 => Ok(()),
                _ => Err(error::Unspecified),
            }
        }
        None => Ok(()), 
    }
}

prefixed_extern! {
    fn CRYPTO_memcmp(a: *const u8, b: *const u8, len: c::NonZero_size_t) -> c_int;
}

pub(crate) fn xor_16(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let a = u128::from_ne_bytes(a);
    let b = u128::from_ne_bytes(b);
    let r = a ^ b;
    r.to_ne_bytes()
}

#[inline(always)]
pub(crate) fn xor_assign<'a>(a: impl IntoIterator<Item = &'a mut u8>, b: u8) {
    a.into_iter().for_each(|a| *a ^= b);
}



#[inline(always)]
pub(crate) fn xor_assign_at_start<'a>(
    a: impl IntoIterator<Item = &'a mut u8>,
    b: impl IntoIterator<Item = &'a u8>,
) {
    a.into_iter().zip(b).for_each(|(a, b)| *a ^= *b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bssl, rand};

    fn leak_in_test(a: BoolMask) -> bool {
        a.leak()
    }

    #[test]
    fn test_constant_time() -> Result<(), error::Unspecified> {
        prefixed_extern! {
            fn bssl_constant_time_test_main() -> bssl::Result;
        }
        Result::from(unsafe { bssl_constant_time_test_main() })
    }

    #[test]
    fn constant_time_conditional_memcpy() -> Result<(), error::Unspecified> {
        let rng = rand::SystemRandom::new();
        for _ in 0..100 {
            let mut out = rand::generate::<[u8; 256]>(&rng)?.expose();
            let input = rand::generate::<[u8; 256]>(&rng)?.expose();

            
            let b = (rand::generate::<[u8; 1]>(&rng)?.expose()[0] & 0x0f) == 0;

            let ref_in = input;
            let ref_out = if b { input } else { out };

            prefixed_extern! {
                fn bssl_constant_time_test_conditional_memcpy(dst: &mut [u8; 256], src: &[u8; 256], b: BoolMask);
            }
            unsafe {
                bssl_constant_time_test_conditional_memcpy(
                    &mut out,
                    &input,
                    if b { BoolMask::TRUE } else { BoolMask::FALSE },
                )
            }
            assert_eq!(ref_in, input);
            assert_eq!(ref_out, out);
        }

        Ok(())
    }

    #[test]
    fn constant_time_conditional_memxor() -> Result<(), error::Unspecified> {
        let rng = rand::SystemRandom::new();
        for _ in 0..256 {
            let mut out = rand::generate::<[u8; 256]>(&rng)?.expose();
            let input = rand::generate::<[u8; 256]>(&rng)?.expose();

            
            let b = (rand::generate::<[u8; 1]>(&rng)?.expose()[0] & 0x0f) != 0;

            let ref_in = input;
            let mut ref_out = out;
            if b {
                xor_assign_at_start(&mut ref_out, &ref_in)
            };

            prefixed_extern! {
                fn bssl_constant_time_test_conditional_memxor(dst: &mut [u8; 256], src: &[u8; 256], b: BoolMask);
            }
            unsafe {
                bssl_constant_time_test_conditional_memxor(
                    &mut out,
                    &input,
                    if b { BoolMask::TRUE } else { BoolMask::FALSE },
                );
            }

            assert_eq!(ref_in, input);
            assert_eq!(ref_out, out);
        }

        Ok(())
    }

    #[test]
    fn test_bool_mask_bitwise_and_is_logical_and() {
        assert!(leak_in_test(BoolMask::TRUE & BoolMask::TRUE));
        assert!(!leak_in_test(BoolMask::TRUE & BoolMask::FALSE));
        assert!(!leak_in_test(BoolMask::FALSE & BoolMask::TRUE));
        assert!(!leak_in_test(BoolMask::FALSE & BoolMask::FALSE));
    }
}

use crate::{
    arithmetic::inout::{AliasingSlices2, AliasingSlices3},
    bb, c,
    error::{self, LenMismatchError},
    polyfill::{sliceutil, usize_from_u32, ArrayFlatMap},
};
use core::{iter, num::NonZeroUsize};

#[cfg(any(test, feature = "alloc"))]
use crate::bits;

#[cfg(feature = "alloc")]
use core::num::Wrapping;

pub type Limb = bb::Word;
pub type LeakyLimb = bb::LeakyWord;
pub const LIMB_BITS: usize = usize_from_u32(Limb::BITS);
pub const LIMB_BYTES: usize = (LIMB_BITS + 7) / 8;

pub type LimbMask = bb::BoolMask;

#[inline]
pub fn limbs_equal_limbs_consttime(a: &[Limb], b: &[Limb]) -> Result<LimbMask, LenMismatchError> {
    if a.len() != b.len() {
        return Err(LenMismatchError::new(a.len()));
    }
    let all = a.iter().zip(b).fold(0, |running, (a, b)| running | (a ^ b));
    Ok(limb_is_zero(all))
}

#[inline]
fn limbs_less_than_limbs(a: &[Limb], b: &[Limb]) -> Result<LimbMask, LenMismatchError> {
    prefixed_extern! {
        fn LIMBS_less_than(a: *const Limb, b: *const Limb, num_limbs: c::NonZero_size_t)
            -> LimbMask;
    }

    let len = NonZeroUsize::new(b.len()).ok_or_else(|| LenMismatchError::new(a.len()))?;
    if a.len() != len.get() {
        return Err(LenMismatchError::new(a.len()));
    }
    Ok(unsafe { LIMBS_less_than(a.as_ptr(), b.as_ptr(), len) })
}

#[inline]
pub(crate) fn verify_limbs_less_than_limbs_leak_bit(
    a: &[Limb],
    b: &[Limb],
) -> Result<(), error::Unspecified> {
    let r = limbs_less_than_limbs(a, b).map_err(error::erase::<LenMismatchError>)?;
    if r.leak() {
        Ok(())
    } else {
        Err(error::Unspecified)
    }
}

#[inline]
pub fn limbs_less_than_limbs_vartime(a: &[Limb], b: &[Limb]) -> Result<bool, LenMismatchError> {
    let r = limbs_less_than_limbs(a, b)?;
    Ok(r.leak())
}

#[inline]
fn limb_is_zero(limb: Limb) -> LimbMask {
    prefixed_extern! {
        fn LIMB_is_zero(limb: Limb) -> LimbMask;
    }
    unsafe { LIMB_is_zero(limb) }
}

#[inline]
pub fn limbs_are_zero(limbs: &[Limb]) -> LimbMask {
    limb_is_zero(limbs.iter().fold(0, |a, b| a | b))
}

#[cfg(any(test, feature = "alloc"))]
#[inline]
pub fn limbs_reject_even_leak_bit(limbs: &[Limb]) -> Result<(), error::Unspecified> {
    let bottom = *limbs.first().ok_or(error::Unspecified)?;
    if limb_is_zero(bottom & 1).leak() {
        return Err(error::Unspecified);
    }
    Ok(())
}

#[cfg(any(test, feature = "alloc"))]
#[inline]
pub fn verify_limbs_equal_1_leak_bit(a: &[Limb]) -> Result<(), error::Unspecified> {
    if let [bottom, ref rest @ ..] = *a {
        let equal = limb_is_zero(bottom ^ 1) & limbs_are_zero(rest);
        if equal.leak() {
            return Ok(());
        }
    }
    Err(error::Unspecified)
}

#[cfg(any(test, feature = "alloc"))]
pub fn limbs_minimal_bits(a: &[Limb]) -> bits::BitLength {
    for num_limbs in (1..=a.len()).rev() {
        let high_limb = a[num_limbs - 1];

        for high_limb_num_bits in (1..=LIMB_BITS).rev() {
            let shifted = unsafe { LIMB_shr(high_limb, high_limb_num_bits - 1) };
            if shifted != 0 {
                return bits::BitLength::from_bits(
                    ((num_limbs - 1) * LIMB_BITS) + high_limb_num_bits,
                );
            }
        }
    }

    bits::BitLength::from_bits(0)
}

#[inline]
pub fn limbs_reduce_once(r: &mut [Limb], m: &[Limb]) -> Result<(), LenMismatchError> {
    prefixed_extern! {
        fn LIMBS_reduce_once(r: *mut Limb, m: *const Limb, num_limbs: c::NonZero_size_t);
    }
    let num_limbs = NonZeroUsize::new(r.len()).ok_or_else(|| LenMismatchError::new(m.len()))?;
    let r = r.as_mut_ptr(); 
    let m = m.as_ptr(); 
    unsafe { LIMBS_reduce_once(r, m, num_limbs) };
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
pub enum AllowZero {
    No,
    Yes,
}

pub fn parse_big_endian_in_range_and_pad_consttime(
    input: untrusted::Input,
    allow_zero: AllowZero,
    max_exclusive: &[Limb],
    result: &mut [Limb],
) -> Result<(), error::Unspecified> {
    parse_big_endian_and_pad_consttime(input, result)?;
    verify_limbs_less_than_limbs_leak_bit(result, max_exclusive)?;
    if allow_zero != AllowZero::Yes {
        if limbs_are_zero(result).leak() {
            return Err(error::Unspecified);
        }
    }
    Ok(())
}

pub fn parse_big_endian_and_pad_consttime(
    input: untrusted::Input,
    result: &mut [Limb],
) -> Result<(), error::Unspecified> {
    if input.is_empty() {
        return Err(error::Unspecified);
    }
    let input_limbs = input.as_slice_less_safe().rchunks(LIMB_BYTES).map(|chunk| {
        let mut padded = [0; LIMB_BYTES];
        sliceutil::overwrite_at_start(&mut padded[(LIMB_BYTES - chunk.len())..], chunk);
        Limb::from_be_bytes(padded)
    });
    if input_limbs.len() > result.len() {
        return Err(error::Unspecified);
    }

    result
        .iter_mut()
        .zip(input_limbs.chain(iter::repeat(0)))
        .for_each(|(r, i)| *r = i);

    Ok(())
}

pub fn big_endian_from_limbs(limbs: &[Limb], out: &mut [u8]) {
    let be_bytes = unstripped_be_bytes(limbs);
    assert_eq!(out.len(), be_bytes.len());
    out.iter_mut().zip(be_bytes).for_each(|(o, i)| {
        *o = i;
    });
}

pub fn unstripped_be_bytes(limbs: &[Limb]) -> impl ExactSizeIterator<Item = u8> + Clone + '_ {

    ArrayFlatMap::new(limbs.iter().rev().copied(), Limb::to_be_bytes).unwrap()
}

pub type Window = bb::Word;

pub type LeakyWindow = bb::LeakyWord;

#[cfg(feature = "alloc")]
pub fn fold_5_bit_windows<R, I: FnOnce(Window) -> R, F: Fn(R, Window) -> R>(
    limbs: &[Limb],
    init: I,
    fold: F,
) -> R {
    #[derive(Clone, Copy)]
    #[repr(transparent)]
    struct BitIndex(Wrapping<c::size_t>);

    const WINDOW_BITS: Wrapping<c::size_t> = Wrapping(5);

    prefixed_extern! {
        fn LIMBS_window5_split_window(
            lower_limb: Limb,
            higher_limb: Limb,
            index_within_word: BitIndex,
        ) -> Window;
        fn LIMBS_window5_unsplit_window(limb: Limb, index_within_word: BitIndex) -> Window;
    }

    let num_limbs = limbs.len();
    let mut window_low_bit = {
        let num_whole_windows = (num_limbs * LIMB_BITS) / 5;
        let mut leading_bits = (num_limbs * LIMB_BITS) - (num_whole_windows * 5);
        if leading_bits == 0 {
            leading_bits = WINDOW_BITS.0;
        }
        BitIndex(Wrapping(LIMB_BITS - leading_bits))
    };

    let initial_value = {
        let leading_partial_window =
            unsafe { LIMBS_window5_split_window(*limbs.first().unwrap(), 0, window_low_bit) };
        window_low_bit.0 -= WINDOW_BITS;
        init(leading_partial_window)
    };

    let mut low_limb = Limb::from(0 as LeakyWindow);
    limbs.iter().fold(initial_value, |mut acc, current_limb| {
        let higher_limb = low_limb;
        low_limb = *current_limb;

        if window_low_bit.0 > Wrapping(LIMB_BITS) - WINDOW_BITS {
            let window =
                unsafe { LIMBS_window5_split_window(low_limb, higher_limb, window_low_bit) };
            window_low_bit.0 -= WINDOW_BITS;
            acc = fold(acc, window);
        };
        while window_low_bit.0 < Wrapping(LIMB_BITS) {
            let window = unsafe { LIMBS_window5_unsplit_window(low_limb, window_low_bit) };

            window_low_bit.0 -= WINDOW_BITS;
            acc = fold(acc, window);
        }
        window_low_bit.0 += Wrapping(LIMB_BITS); 

        acc
    })
}

#[inline]
pub(crate) fn limbs_add_assign_mod(
    a: &mut [Limb],
    b: &[Limb],
    m: &[Limb],
) -> Result<(), LenMismatchError> {
    prefixed_extern! {

        fn LIMBS_add_mod(
            r: *mut Limb,
            a: *const Limb,
            b: *const Limb,
            m: *const Limb,
            num_limbs: c::NonZero_size_t,
        );
    }
    let num_limbs = NonZeroUsize::new(m.len()).ok_or_else(|| LenMismatchError::new(m.len()))?;
    (a, b).with_non_dangling_non_null_pointers_rab(num_limbs, |r, a, b| {
        let m = m.as_ptr(); 
        unsafe { LIMBS_add_mod(r, a, b, m, num_limbs) }
    })
}

pub(crate) fn limbs_double_mod(r: &mut [Limb], m: &[Limb]) -> Result<(), LenMismatchError> {
    prefixed_extern! {

        fn LIMBS_shl_mod(
            r: *mut Limb,
            a: *const Limb,
            m: *const Limb,
            num_limbs: c::NonZero_size_t);
    }
    let num_limbs = NonZeroUsize::new(m.len()).ok_or_else(|| LenMismatchError::new(m.len()))?;
    r.with_non_dangling_non_null_pointers_ra(num_limbs, |r, a| {
        let m = m.as_ptr(); 
        unsafe {
            LIMBS_shl_mod(r, a, m, num_limbs);
        }
    })
}

pub(crate) fn limbs_negative_odd(r: &mut [Limb], a: &[Limb]) {
    debug_assert_eq!(r.len(), a.len());

    r.iter_mut().zip(a.iter()).for_each(|(r, &a)| {
        *r = !a;
    });

    r[0] |= 1;
}

#[cfg(any(test, feature = "alloc"))]
prefixed_extern! {
    fn LIMB_shr(a: Limb, shift: c::size_t) -> Limb;
}

#[allow(clippy::useless_conversion)]
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use cfg_if::cfg_if;

    const MAX: LeakyLimb = LeakyLimb::MAX;

    fn leak_in_test(a: LimbMask) -> bool {
        a.leak()
    }

    #[test]
    fn test_limbs_are_even() {
        static EVENS: &[&[LeakyLimb]] = &[
            &[],
            &[0],
            &[2],
            &[0, 0],
            &[2, 0],
            &[0, 1],
            &[0, 2],
            &[0, 3],
            &[0, 0, 0, 0, MAX],
        ];
        for even in EVENS {
            let even = &Vec::from_iter(even.iter().copied().map(Limb::from));
            assert!(matches!(
                limbs_reject_even_leak_bit(even),
                Err(error::Unspecified)
            ));
        }
        static ODDS: &[&[LeakyLimb]] = &[
            &[1],
            &[3],
            &[1, 0],
            &[3, 0],
            &[1, 1],
            &[1, 2],
            &[1, 3],
            &[1, 0, 0, 0, MAX],
        ];
        for odd in ODDS {
            let odd = &Vec::from_iter(odd.iter().copied().map(Limb::from));
            assert!(matches!(limbs_reject_even_leak_bit(odd), Ok(())));
        }
    }

    static ZEROES: &[&[LeakyLimb]] = &[
        &[],
        &[0],
        &[0, 0],
        &[0, 0, 0],
        &[0, 0, 0, 0],
        &[0, 0, 0, 0, 0],
        &[0, 0, 0, 0, 0, 0, 0],
        &[0, 0, 0, 0, 0, 0, 0, 0],
        &[0, 0, 0, 0, 0, 0, 0, 0, 0],
    ];

    static NONZEROES: &[&[LeakyLimb]] = &[
        &[1],
        &[0, 1],
        &[1, 1],
        &[1, 0, 0, 0],
        &[0, 1, 0, 0],
        &[0, 0, 1, 0],
        &[0, 0, 0, 1],
    ];

    #[test]
    fn test_limbs_are_zero() {
        for zero in ZEROES {
            let zero = &Vec::from_iter(zero.iter().copied().map(Limb::from));
            assert!(leak_in_test(limbs_are_zero(zero)));
        }
        for nonzero in NONZEROES {
            let nonzero = &Vec::from_iter(nonzero.iter().copied().map(Limb::from));
            assert!(!leak_in_test(limbs_are_zero(nonzero)));
        }
    }

    #[test]
    fn test_limbs_equal_limb() {

        static EQUAL: &[&[LeakyLimb]] = &[&[1], &[1, 0], &[1, 0, 0], &[1, 0, 0, 0, 0, 0, 0]];
        for a in EQUAL {
            let a = &Vec::from_iter(a.iter().copied().map(Limb::from));
            assert!(matches!(verify_limbs_equal_1_leak_bit(a), Ok(())));
        }

        static UNEQUAL: &[&[LeakyLimb]] = &[
            &[0],
            &[2],
            &[3],
            &[MAX],
            &[0, 1],
            &[1, 1],
            &[0, 0, 0, 0, 0, 0, 0, 1],
            &[0, 0, 0, 0, 1, 0, 0, 0],
            &[0, 0, 0, 0, 1, 0, 0, 1],
            &[MAX, 1],
        ];
        for a in UNEQUAL {
            let a = &Vec::from_iter(a.iter().copied().map(Limb::from));
            assert!(matches!(
                verify_limbs_equal_1_leak_bit(a),
                Err(error::Unspecified)
            ));
        }
    }

    #[test]
    fn test_parse_big_endian_and_pad_consttime() {
        const LIMBS: usize = 4;

        {

            let inp = untrusted::Input::from(&[]);
            let mut result = [0; LIMBS].map(From::<LeakyLimb>::from);
            assert!(parse_big_endian_and_pad_consttime(inp, &mut result).is_err());
        }

        {
            let inp = [1, 2, 3, 4, 5, 6, 7, 8, 9];
            let inp = untrusted::Input::from(&inp);
            let mut result = [0; 8 / LIMB_BYTES].map(From::<LeakyLimb>::from);
            assert!(parse_big_endian_and_pad_consttime(inp, &mut result[..]).is_err());
        }

        {
            let inp = [0xfe];
            let inp = untrusted::Input::from(&inp);
            let mut result = [0; LIMBS].map(From::<LeakyLimb>::from);
            assert_eq!(
                Ok(()),
                parse_big_endian_and_pad_consttime(inp, &mut result[..])
            );
            assert_eq!(&[0xfe, 0, 0, 0], &result);
        }

        {
            let inp = [0xbe, 0xef, 0xf0, 0x0d];
            let inp = untrusted::Input::from(&inp);
            let mut result = [0; LIMBS].map(From::<LeakyLimb>::from);
            assert_eq!(Ok(()), parse_big_endian_and_pad_consttime(inp, &mut result));
            assert_eq!(&[0xbeeff00d, 0, 0, 0], &result);
        }

        cfg_if! {
            if #[cfg(target_pointer_width = "64")] {
                static TEST_CASES: &[(&[u8], &[Limb])] = &[
                    (&[1], &[1, 0]),
                    (&[1, 2], &[0x102, 0]),
                    (&[1, 2, 3], &[0x10203, 0]),
                    (&[1, 2, 3, 4], &[0x102_0304, 0]),
                    (&[1, 2, 3, 4, 5], &[0x1_0203_0405, 0]),
                    (&[1, 2, 3, 4, 5, 6], &[0x102_0304_0506, 0]),
                    (&[1, 2, 3, 4, 5, 6, 7], &[0x1_0203_0405_0607, 0]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8], &[0x102_0304_0506_0708, 0]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8, 9], &[0x0203_0405_0607_0809, 0x1]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa], &[0x0304_0506_0708_090a, 0x102]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa, 0xb], &[0x0405_0607_0809_0a0b, 0x1_0203]),
                ];
                for (be_bytes, limbs) in TEST_CASES {
                    let mut buf = [0; 2];
                    parse_big_endian_and_pad_consttime(untrusted::Input::from(be_bytes), &mut buf)
                        .unwrap();
                    assert_eq!(limbs, &buf, "({be_bytes:x?}, {limbs:x?}");
                }
            } else if #[cfg(target_pointer_width = "32")] {
                static TEST_CASES: &[(&[u8], &[Limb])] = &[
                    (&[1], &[1, 0, 0]),
                    (&[1, 2], &[0x102, 0, 0]),
                    (&[1, 2, 3], &[0x10203, 0, 0]),
                    (&[1, 2, 3, 4], &[0x102_0304, 0, 0]),
                    (&[1, 2, 3, 4, 5], &[0x0203_0405, 0x1, 0]),
                    (&[1, 2, 3, 4, 5, 6], &[0x0304_0506, 0x102, 0]),
                    (&[1, 2, 3, 4, 5, 6, 7], &[0x0405_0607, 0x1_0203, 0]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8], &[0x0506_0708, 0x102_0304, 0]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8, 9], &[0x0607_0809, 0x0203_0405, 0x1]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa], &[0x0708_090a, 0x0304_0506, 0x102]),
                    (&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa, 0xb], &[0x0809_0a0b, 0x0405_0607, 0x1_0203]),
                ];
                for (be_bytes, limbs) in TEST_CASES {
                    let mut buf = [0; 3];
                    parse_big_endian_and_pad_consttime(untrusted::Input::from(be_bytes), &mut buf)
                        .unwrap();
                    assert_eq!(limbs, &buf, "({be_bytes:x?}, {limbs:x?}");
                }
            } else {
                panic!("Unsupported target_pointer_width");
            }

        }
    }

    #[test]
    fn test_big_endian_from_limbs_same_length() {
        #[cfg(target_pointer_width = "32")]
        let limbs = [
            0xbccddeef, 0x89900aab, 0x45566778, 0x01122334, 0xddeeff00, 0x99aabbcc, 0x55667788,
            0x11223344,
        ];

        #[cfg(target_pointer_width = "64")]
        let limbs = [
            0x8990_0aab_bccd_deef,
            0x0112_2334_4556_6778,
            0x99aa_bbcc_ddee_ff00,
            0x1122_3344_5566_7788,
        ];

        let limbs = limbs.map(From::<LeakyLimb>::from);

        let expected = [
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff, 0x00, 0x01, 0x12, 0x23, 0x34, 0x45, 0x56, 0x67, 0x78, 0x89, 0x90, 0x0a, 0xab,
            0xbc, 0xcd, 0xde, 0xef,
        ];

        let mut out = [0xabu8; 32];
        big_endian_from_limbs(&limbs[..], &mut out);
        assert_eq!(&out[..], &expected[..]);
    }

    #[should_panic]
    #[test]
    fn test_big_endian_from_limbs_fewer_limbs() {
        #[cfg(target_pointer_width = "32")]

        let limbs = [
            0xbccddeef, 0x89900aab, 0x45566778, 0x01122334, 0xddeeff00, 0x99aabbcc,
        ];

        #[cfg(target_pointer_width = "64")]
        let limbs = [
            0x8990_0aab_bccd_deef,
            0x0112_2334_4556_6778,
            0x99aa_bbcc_ddee_ff00,
        ];

        let limbs = limbs.map(From::<LeakyLimb>::from);

        let mut out = [0xabu8; 32];

        big_endian_from_limbs(&limbs[..], &mut out);
    }

    #[test]
    fn test_limbs_minimal_bits() {
        const ALL_ONES: LeakyLimb = LeakyLimb::MAX;
        static CASES: &[(&[LeakyLimb], usize)] = &[
            (&[], 0),
            (&[0], 0),
            (&[ALL_ONES], LIMB_BITS),
            (&[ALL_ONES, 0], LIMB_BITS),
            (&[ALL_ONES, 1], LIMB_BITS + 1),
            (&[0, 0], 0),
            (&[1, 0], 1),
            (&[0, 1], LIMB_BITS + 1),
            (&[0, ALL_ONES], 2 * LIMB_BITS),
            (&[ALL_ONES, ALL_ONES], 2 * LIMB_BITS),
            (&[ALL_ONES, ALL_ONES >> 1], 2 * LIMB_BITS - 1),
            (&[ALL_ONES, 0b100_0000], LIMB_BITS + 7),
            (&[ALL_ONES, 0b101_0000], LIMB_BITS + 7),
            (&[ALL_ONES, ALL_ONES >> 1], LIMB_BITS + (LIMB_BITS) - 1),
        ];
        for (limbs, bits) in CASES {
            let limbs = &Vec::from_iter(limbs.iter().copied().map(Limb::from));
            assert_eq!(limbs_minimal_bits(limbs).as_bits(), *bits);
        }
    }
}

use super::{inout::AliasingSlices3, n0::N0, LimbSliceError, MAX_LIMBS, MIN_LIMBS};
use crate::{c, limb::Limb, polyfill::usize_from_u32};
use core::{mem::size_of, num::NonZeroUsize};

const _MAX_LIMBS_ADDRESSES_MEMORY_SAFETY_ISSUES: () = {

    const BN_MONTGOMERY_MAX_WORDS: usize = (8 * 1092) / size_of::<Limb>();
    assert!(MAX_LIMBS <= BN_MONTGOMERY_MAX_WORDS);

    assert!(MAX_LIMBS <= usize_from_u32(u32::MAX));
};

macro_rules! bn_mul_mont_ffi {
    ( $in_out:expr, $n:expr, $n0:expr, $cpu:expr,
      unsafe { ($MIN_LEN:expr, $MOD_LEN:expr, $Cpu:ty) => $f:ident }) => {{
        use crate::{c, limb::Limb};
        prefixed_extern! {

            fn $f(
                r: *mut Limb,
                a: *const Limb,
                b: *const Limb,
                n: *const Limb,
                n0: &N0,
                len: c::NonZero_size_t,
            );
        }
        unsafe {
            crate::arithmetic::ffi::bn_mul_mont_ffi::<$Cpu, { $MIN_LEN }, { $MOD_LEN }>(
                $in_out, $n, $n0, $cpu, $f,
            )
        }
    }};
}

#[inline]
pub(super) unsafe fn bn_mul_mont_ffi<Cpu, const LEN_MIN: usize, const LEN_MOD: usize>(
    in_out: impl AliasingSlices3<Limb>,
    n: &[Limb],
    n0: &N0,
    cpu: Cpu,
    f: unsafe extern "C" fn(
        r: *mut Limb,
        a: *const Limb,
        b: *const Limb,
        n: *const Limb,
        n0: &N0,
        len: c::NonZero_size_t,
    ),
) -> Result<(), LimbSliceError> {
    assert_eq!(n.len() % LEN_MOD, 0); 

    const _MIN_LIMBS_AT_LEAST_4: () = assert!(MIN_LIMBS >= 4);

    assert!(LEN_MIN >= MIN_LIMBS);
    if n.len() < LEN_MIN {
        return Err(LimbSliceError::too_short(n.len()));
    }
    let len = NonZeroUsize::new(n.len()).unwrap_or_else(|| {

        unreachable!()
    });

    if len.get() > MAX_LIMBS {
        return Err(LimbSliceError::too_long(n.len()));
    }
    in_out
        .with_non_dangling_non_null_pointers_rab(len, |r, a, b| {
            let n = n.as_ptr();
            let _: Cpu = cpu;
            unsafe { f(r, a, b, n, n0, len) };
        })
        .map_err(LimbSliceError::len_mismatch)
}

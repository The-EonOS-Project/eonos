pub(crate) use self::{constant::limbs_from_hex, limb_slice_error::LimbSliceError};
use crate::{error::LenMismatchError, limb::LIMB_BITS};

#[macro_use]
mod ffi;

mod constant;

#[cfg(feature = "alloc")]
pub mod bigint;

pub(crate) mod inout;
mod limbs;
mod limbs512;
pub mod montgomery;

mod n0;

pub const MIN_LIMBS: usize = 4;

pub const MAX_LIMBS: usize = 8192 / LIMB_BITS;

cold_exhaustive_error! {
    enum limb_slice_error::LimbSliceError {
        len_mismatch => LenMismatch(LenMismatchError),
        too_short => TooShort(usize),
        too_long => TooLong(usize),
    }
}

#![cfg_attr(not(test), allow(dead_code))]

use super::Overlapping;
use crate::error::LenMismatchError;
use core::array::TryFromSliceError;

pub struct Array<'o, T, const N: usize> {

    in_out: Overlapping<'o, T>,
}

impl<'o, T, const N: usize> Array<'o, T, N> {
    pub(super) fn new(in_out: Overlapping<'o, T>) -> Result<Self, LenMismatchError> {
        if N == 0 || in_out.len() != N {
            return Err(LenMismatchError::new(N));
        }
        Ok(Self { in_out })
    }

    pub fn into_unwritten_output(self) -> &'o mut [T; N]
    where
        &'o mut [T]: TryInto<&'o mut [T; N], Error = TryFromSliceError>,
    {
        self.in_out
            .into_unwritten_output()
            .try_into()
            .unwrap_or_else(|TryFromSliceError { .. }| {
                unreachable!() 
            })
    }
}

impl<T, const N: usize> Array<'_, T, N> {
    pub fn input<'s>(&'s self) -> &'s [T; N]
    where
        &'s [T]: TryInto<&'s [T; N], Error = TryFromSliceError>,
    {
        self.in_out
            .input()
            .try_into()
            .unwrap_or_else(|TryFromSliceError { .. }| {
                unreachable!() 
            })
    }
}

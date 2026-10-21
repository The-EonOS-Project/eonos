use crate::{error::InputTooLongError, polyfill};

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct BitLength<T = usize>(T);

pub(crate) trait FromByteLen<T>: Sized {

    fn from_byte_len(bytes: T) -> Result<Self, InputTooLongError<T>>;
}

impl FromByteLen<usize> for BitLength<usize> {
    #[inline]
    fn from_byte_len(bytes: usize) -> Result<Self, InputTooLongError> {
        match bytes.checked_mul(8) {
            Some(bits) => Ok(Self(bits)),
            None => Err(InputTooLongError::new(bytes)),
        }
    }
}

impl FromByteLen<u64> for BitLength<u64> {
    #[inline]
    fn from_byte_len(bytes: u64) -> Result<Self, InputTooLongError<u64>> {
        match bytes.checked_mul(8) {
            Some(bits) => Ok(Self(bits)),
            None => Err(InputTooLongError::new(bytes)),
        }
    }
}

impl FromByteLen<usize> for BitLength<u64> {
    #[inline]
    fn from_byte_len(bytes: usize) -> Result<Self, InputTooLongError<usize>> {
        match polyfill::u64_from_usize(bytes).checked_mul(8) {
            Some(bits) => Ok(Self(bits)),
            None => Err(InputTooLongError::new(bytes)),
        }
    }
}

impl<T> BitLength<T> {

    #[inline]
    pub const fn from_bits(bits: T) -> Self {
        Self(bits)
    }
}

impl<T: Copy> BitLength<T> {

    #[inline]
    pub fn as_bits(self) -> T {
        self.0
    }
}

impl BitLength<usize> {
    #[cfg(feature = "alloc")]
    #[inline]
    pub(crate) fn half_rounded_up(&self) -> Self {
        let round_up = self.0 & 1;
        Self((self.0 / 2) + round_up)
    }

    #[inline]
    pub const fn as_usize_bytes_rounded_up(&self) -> usize {

        let round_up = ((self.0 >> 2) | (self.0 >> 1) | self.0) & 1;

        (self.0 / 8) + round_up
    }

    #[cfg(feature = "alloc")]
    #[inline]
    pub(crate) fn try_sub_1(self) -> Result<Self, crate::error::Unspecified> {
        let sum = self.0.checked_sub(1).ok_or(crate::error::Unspecified)?;
        Ok(Self(sum))
    }
}

impl BitLength<u64> {
    pub fn to_be_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl From<BitLength<usize>> for BitLength<u64> {
    fn from(BitLength(value): BitLength<usize>) -> Self {
        BitLength(polyfill::u64_from_usize(value))
    }
}

impl TryFrom<BitLength<u64>> for BitLength<core::num::NonZeroU64> {
    type Error = <core::num::NonZeroU64 as TryFrom<u64>>::Error;

    fn try_from(BitLength(value): BitLength<u64>) -> Result<Self, Self::Error> {
        value.try_into().map(BitLength)
    }
}

const _TEST_AS_USIZE_BYTES_ROUNDED_UP_EVEN: () =
    assert!(BitLength::from_bits(8192).as_usize_bytes_rounded_up() == 8192 / 8);
const _TEST_AS_USIZE_BYTES_ROUNDED_UP_ONE_BIT_HIGH: () =
    assert!(BitLength::from_bits(8192 + 1).as_usize_bytes_rounded_up() == (8192 / 8) + 1);
const _TEST_AS_USIZE_BYTES_ROUNDED_UP_SEVEN_BITS_HIGH: () =
    assert!(BitLength::from_bits(8192 + 7).as_usize_bytes_rounded_up() == (8192 / 8) + 1);

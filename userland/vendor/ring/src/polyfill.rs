#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
#[inline(always)]
pub const fn u64_from_usize(x: usize) -> u64 {
    x as u64
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
pub const fn usize_from_u32(x: u32) -> usize {
    x as usize
}

#[cfg(all(
    target_arch = "aarch64",
    target_endian = "little",
    target_pointer_width = "64"
))]
#[allow(clippy::cast_possible_truncation)]
pub fn usize_from_u64(x: u64) -> usize {
    x as usize
}

#[allow(clippy::cast_possible_truncation)]
#[inline(always)]
pub const fn usize_from_u64_saturated(x: u64) -> usize {
    const USIZE_MAX: u64 = u64_from_usize(usize::MAX);
    if x < USIZE_MAX {
        x as usize
    } else {
        usize::MAX
    }
}

#[macro_use]
mod cold_error;

mod array_flat_map;
mod array_split_map;

pub mod cstr;

pub mod sliceutil;

#[cfg(feature = "alloc")]
mod leading_zeros_skipped;

#[cfg(any(
    all(target_arch = "aarch64", target_endian = "little"),
    all(target_arch = "arm", target_endian = "little"),
    target_arch = "x86",
    target_arch = "x86_64"
))]
pub mod once_cell {
    pub mod race;
}

mod notsend;
pub mod ptr;

pub mod slice;

#[cfg(test)]
mod test;

mod unwrap_const;

pub use self::{
    array_flat_map::ArrayFlatMap, array_split_map::ArraySplitMap, notsend::NotSend,
    unwrap_const::unwrap_const,
};

#[cfg(feature = "alloc")]
pub use leading_zeros_skipped::LeadingZerosStripped;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_usize_from_u64_saturated() {
        const USIZE_MAX: u64 = u64_from_usize(usize::MAX);
        assert_eq!(usize_from_u64_saturated(u64::MIN), usize::MIN);
        assert_eq!(usize_from_u64_saturated(USIZE_MAX), usize::MAX);
        assert_eq!(usize_from_u64_saturated(USIZE_MAX - 1), usize::MAX - 1);

        #[cfg(not(target_pointer_width = "64"))]
        {
            assert_eq!(usize_from_u64_saturated(USIZE_MAX + 1), usize::MAX);
        }
    }
}

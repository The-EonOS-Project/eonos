













use crate::error;
use core::ffi::c_int;




#[must_use]
#[repr(transparent)]
pub struct Result(c_int);

impl From<Result> for core::result::Result<(), error::Unspecified> {
    fn from(ret: Result) -> Self {
        match ret.0 {
            1 => Ok(()),
            c => {
                debug_assert_eq!(c, 0, "`bssl::Result` value must be 0 or 1");
                Err(error::Unspecified)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    mod result {
        use crate::bssl;
        use core::{
            ffi::c_int,
            mem::{align_of, size_of},
        };

        #[test]
        fn size_and_alignment() {
            type Underlying = c_int;
            assert_eq!(size_of::<bssl::Result>(), size_of::<Underlying>());
            assert_eq!(align_of::<bssl::Result>(), align_of::<Underlying>());
        }

        #[test]
        fn semantics() {
            assert!(Result::from(bssl::Result(0)).is_err());
            assert!(Result::from(bssl::Result(1)).is_ok());
        }
    }
}

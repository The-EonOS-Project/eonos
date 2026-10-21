













#[cfg(feature = "std")]
extern crate std;

























































#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unspecified;


impl core::fmt::Display for Unspecified {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.write_str("ring::error::Unspecified")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Unspecified {}

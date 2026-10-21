













use crate::{bb, error};

#[deprecated(
    note = "To be removed. Internal function not intended for external use with no promises regarding side channels."
)]
pub fn verify_slices_are_equal(a: &[u8], b: &[u8]) -> Result<(), error::Unspecified> {
    bb::verify_slices_are_equal(a, b)
}

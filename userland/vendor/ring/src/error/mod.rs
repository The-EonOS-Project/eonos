















pub use self::{key_rejected::KeyRejected, unspecified::Unspecified};

pub(crate) use self::{
    input_too_long::InputTooLongError, len_mismatch_error::LenMismatchError,
    too_much_output_requested::TooMuchOutputRequestedError,
};

mod input_too_long;
mod into_unspecified;
mod key_rejected;
mod unspecified;

#[cold]
#[inline(never)]
pub(crate) fn erase<T>(_: T) -> Unspecified {
    Unspecified
}

cold_exhaustive_error! {
    struct too_much_output_requested::TooMuchOutputRequestedError
        with pub(crate) constructor {
        
        
        
        
        
        
        
        
        
        
        
        imprecise_output_length: usize
    }
}

cold_exhaustive_error! {
    struct len_mismatch_error::LenMismatchError
        with pub(crate) constructor {
        len: usize
    }
}

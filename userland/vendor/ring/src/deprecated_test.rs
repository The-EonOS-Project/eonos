













#![doc(hidden)]


#[macro_export]
macro_rules! test_file {
    ($file_name:expr) => {
        $crate::test::File {
            file_name: $file_name,
            contents: include_str!($file_name),
        }
    };
}

pub use crate::testutil::{
    compile_time_assert_clone, compile_time_assert_copy, compile_time_assert_eq,
    compile_time_assert_send, compile_time_assert_sync, from_hex, run, File, TestCase,
};

#[cfg(feature = "std")]
pub use crate::testutil::compile_time_assert_std_error_error;

#[deprecated(note = "internal API that will be removed")]
#[doc(hidden)]
pub mod rand {
    #[deprecated(note = "internal API that will be removed")]
    pub type FixedByteRandom = crate::testutil::rand::FixedByteRandom;
    #[deprecated(note = "internal API that will be removed")]
    pub type FixedSliceRandom<'a> = crate::testutil::rand::FixedSliceRandom<'a>;
    #[deprecated(note = "internal API that will be removed")]
    pub type FixedSliceSequenceRandom<'a> = crate::testutil::rand::FixedSliceSequenceRandom<'a>;
}

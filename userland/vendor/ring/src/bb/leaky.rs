#[cfg(target_pointer_width = "64")]
type CompilerWord = u64;

#[cfg(target_pointer_width = "32")]
type CompilerWord = u32;

pub(crate) type LeakyWord = CompilerWord;

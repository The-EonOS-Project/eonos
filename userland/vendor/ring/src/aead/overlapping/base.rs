













pub use self::index_error::IndexError;
use super::Array;
use crate::error::LenMismatchError;
use core::{mem, ops::RangeFrom};

pub struct Overlapping<'o, T> {
    
    in_out: &'o mut [T],
    src: RangeFrom<usize>,
}

impl<'o, T> From<&'o mut [T]> for Overlapping<'o, T> {
    fn from(in_out: &'o mut [T]) -> Self {
        Self { in_out, src: 0.. }
    }
}

impl<'o, T> Overlapping<'o, T> {
    pub fn new(in_out: &'o mut [T], src: RangeFrom<usize>) -> Result<Self, IndexError> {
        match in_out.get(src.clone()) {
            Some(_) => Ok(Self { in_out, src }),
            None => Err(IndexError::new(src.start)),
        }
    }

    #[cfg(any(
        all(target_arch = "arm", target_endian = "little"),
        target_arch = "x86"
    ))]
    pub fn copy_within(self) -> &'o mut [T]
    where
        T: Copy,
    {
        if self.src.start == 0 {
            self.in_out
        } else {
            let len = self.len();
            self.in_out.copy_within(self.src, 0);
            &mut self.in_out[..len]
        }
    }

    #[cfg(any(
        all(target_arch = "arm", target_endian = "little"),
        target_arch = "x86"
    ))]
    pub fn into_slice_src_mut(self) -> (&'o mut [T], RangeFrom<usize>) {
        (self.in_out, self.src)
    }

    pub fn into_unwritten_output(self) -> &'o mut [T] {
        let len = self.len();
        self.in_out.get_mut(..len).unwrap_or_else(|| {
            
            unreachable!()
        })
    }
}

impl<T> Overlapping<'_, T> {
    pub fn len(&self) -> usize {
        self.input().len()
    }

    pub fn input(&self) -> &[T] {
        self.in_out.get(self.src.clone()).unwrap_or_else(|| {
            
            unreachable!()
        })
    }

    pub fn with_input_output_len<R>(self, f: impl FnOnce(*const T, *mut T, usize) -> R) -> R {
        let len = self.len();
        let output = self.in_out.as_mut_ptr();
        
        let output_const: *const T = output;
        
        
        
        
        
        
        let input = if self.src.start == 0 {
            output_const
        } else {
            unsafe { output_const.add(self.src.start) }
        };
        f(input, output, len)
    }

    
    
    
    
    
    
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn split_first_chunk<const N: usize>(
        mut self,
        f: impl for<'a> FnOnce(Array<'a, T, N>),
    ) -> Result<Self, IndexError> {
        let src = self.src.clone();
        let end = self
            .src
            .start
            .checked_add(N)
            .ok_or_else(|| IndexError::new(N))?;
        let first = self
            .in_out
            .get_mut(..end)
            .ok_or_else(|| IndexError::new(N))?;
        let first = Overlapping::new(first, src).unwrap_or_else(|IndexError { .. }| {
            
            unreachable!()
        });
        let first = Array::new(first).unwrap_or_else(|LenMismatchError { .. }| {
            
            unreachable!()
        });
        
        
        Ok({
            f(first);
            let tail = mem::take(&mut self.in_out).get_mut(N..).unwrap_or_else(|| {
                
                unreachable!()
            });
            Self::new(tail, self.src).unwrap_or_else(|IndexError { .. }| {
                
                unreachable!()
            })
        })
    }
}

cold_exhaustive_error! {
    struct index_error::IndexError { index: usize }
}















use super::Overlapping;
use crate::error::InputTooLongError;

pub struct PartialBlock<'i, T, const BLOCK_LEN: usize> {
    
    in_out: Overlapping<'i, T>,
}

impl<'i, T, const BLOCK_LEN: usize> PartialBlock<'i, T, BLOCK_LEN> {
    pub fn new(in_out: Overlapping<'i, T>) -> Result<Self, InputTooLongError> {
        let len = in_out.len();
        if len >= BLOCK_LEN {
            return Err(InputTooLongError::new(len));
        }
        Ok(Self { in_out })
    }

    pub fn overwrite_at_start(self, padded: [T; BLOCK_LEN])
    where
        T: Copy,
    {
        let len = self.len();
        let output = self.in_out.into_unwritten_output();
        assert!(output.len() <= padded.len());
        output.copy_from_slice(&padded[..len]);
    }
}

impl<T, const BLOCK_LEN: usize> PartialBlock<'_, T, BLOCK_LEN> {
    #[inline(always)]
    pub fn input(&self) -> &[T] {
        let r = self.in_out.input();
        
        
        if r.len() >= BLOCK_LEN {
            unreachable!()
        }
        r
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.input().len()
    }
}

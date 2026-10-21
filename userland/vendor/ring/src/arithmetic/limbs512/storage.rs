













use crate::{
    error::LenMismatchError,
    limb::{Limb, LIMB_BITS},
    polyfill::slice::{self, AsChunksMut},
};
use core::mem::{align_of, size_of};








#[repr(C, align(64))]
pub struct AlignedStorage<const N: usize>([Limb; N]);

const _LIMB_SIZE_DIVIDES_ALIGNMENT: () =
    assert!(align_of::<AlignedStorage<1>>() % size_of::<Limb>() == 0);

pub const LIMBS_PER_CHUNK: usize = 512 / LIMB_BITS;

impl<const N: usize> AlignedStorage<N> {
    pub fn zeroed() -> Self {
        assert_eq!(N % LIMBS_PER_CHUNK, 0); 
        Self([0; N])
    }

    
    pub fn aligned_chunks_mut(
        &mut self,
        num_entries: usize,
        chunks_per_entry: usize,
    ) -> Result<AsChunksMut<Limb, LIMBS_PER_CHUNK>, LenMismatchError> {
        let total_limbs = num_entries * chunks_per_entry * LIMBS_PER_CHUNK;
        let len = self.0.len();
        let flattened = self
            .0
            .get_mut(..total_limbs)
            .ok_or_else(|| LenMismatchError::new(len))?;
        match slice::as_chunks_mut(flattened) {
            (chunks, []) => Ok(chunks),
            (_, r) => Err(LenMismatchError::new(r.len())),
        }
    }
}

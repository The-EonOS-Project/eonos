






















use super::{Aad, Algorithm, BoundKey, LessSafeKey, NonceSequence, Tag, UnboundKey};
use crate::error;






pub struct SealingKey<N: NonceSequence> {
    key: LessSafeKey,
    nonce_sequence: N,
}

impl<N: NonceSequence> BoundKey<N> for SealingKey<N> {
    fn new(key: UnboundKey, nonce_sequence: N) -> Self {
        Self {
            key: key.into_inner(),
            nonce_sequence,
        }
    }

    #[inline]
    fn algorithm(&self) -> &'static Algorithm {
        self.key.algorithm()
    }
}

impl<N: NonceSequence> core::fmt::Debug for SealingKey<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        self.key.fmt_debug("SealingKey", f)
    }
}

impl<N: NonceSequence> SealingKey<N> {
    
    
    
    
    
    
    
    
    
    #[inline]
    pub fn seal_in_place_append_tag<A, InOut>(
        &mut self,
        aad: Aad<A>,
        in_out: &mut InOut,
    ) -> Result<(), error::Unspecified>
    where
        A: AsRef<[u8]>,
        InOut: AsMut<[u8]> + for<'in_out> Extend<&'in_out u8>,
    {
        self.key
            .seal_in_place_append_tag(self.nonce_sequence.advance()?, aad, in_out)
    }

    
    
    
    
    
    
    
    
    
    
    
    #[inline]
    pub fn seal_in_place_separate_tag<A>(
        &mut self,
        aad: Aad<A>,
        in_out: &mut [u8],
    ) -> Result<Tag, error::Unspecified>
    where
        A: AsRef<[u8]>,
    {
        self.key
            .seal_in_place_separate_tag(self.nonce_sequence.advance()?, aad, in_out)
    }
}

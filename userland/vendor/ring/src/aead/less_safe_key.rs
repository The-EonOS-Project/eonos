













use super::{Aad, Algorithm, KeyInner, Nonce, Tag, UnboundKey, TAG_LEN};
use crate::{cpu, error};
use core::ops::RangeFrom;





#[derive(Clone)]
pub struct LessSafeKey {
    inner: KeyInner,
    algorithm: &'static Algorithm,
}

impl LessSafeKey {
    
    #[inline]
    pub fn new(key: UnboundKey) -> Self {
        key.into_inner()
    }

    pub(super) fn new_(
        algorithm: &'static Algorithm,
        key_bytes: &[u8],
        cpu_features: cpu::Features,
    ) -> Result<Self, error::Unspecified> {
        Ok(Self {
            inner: algorithm.new_key(key_bytes, cpu_features)?,
            algorithm,
        })
    }

    
    
    #[inline]
    pub fn open_in_place_separate_tag<'in_out, A>(
        &self,
        nonce: Nonce,
        aad: Aad<A>,
        tag: Tag,
        in_out: &'in_out mut [u8],
        ciphertext: RangeFrom<usize>,
    ) -> Result<&'in_out mut [u8], error::Unspecified>
    where
        A: AsRef<[u8]>,
    {
        let aad = Aad::from(aad.as_ref());
        self.algorithm.open_within(
            &self.inner,
            nonce,
            aad,
            tag,
            in_out,
            ciphertext,
            cpu::features(),
        )
    }

    
    
    
    
    #[inline]
    pub fn open_in_place<'in_out, A>(
        &self,
        nonce: Nonce,
        aad: Aad<A>,
        in_out: &'in_out mut [u8],
    ) -> Result<&'in_out mut [u8], error::Unspecified>
    where
        A: AsRef<[u8]>,
    {
        self.open_within(nonce, aad, in_out, 0..)
    }

    
    
    
    
    #[inline]
    pub fn open_within<'in_out, A>(
        &self,
        nonce: Nonce,
        aad: Aad<A>,
        in_out: &'in_out mut [u8],
        ciphertext_and_tag: RangeFrom<usize>,
    ) -> Result<&'in_out mut [u8], error::Unspecified>
    where
        A: AsRef<[u8]>,
    {
        let tag_offset = in_out
            .len()
            .checked_sub(TAG_LEN)
            .ok_or(error::Unspecified)?;

        
        let (in_out, received_tag) = in_out.split_at_mut(tag_offset);
        let received_tag = (*received_tag).try_into()?;
        let ciphertext = ciphertext_and_tag;

        self.open_in_place_separate_tag(nonce, aad, received_tag, in_out, ciphertext)
    }

    
    
    
    
    #[inline]
    pub fn seal_in_place_append_tag<A, InOut>(
        &self,
        nonce: Nonce,
        aad: Aad<A>,
        in_out: &mut InOut,
    ) -> Result<(), error::Unspecified>
    where
        A: AsRef<[u8]>,
        InOut: AsMut<[u8]> + for<'in_out> Extend<&'in_out u8>,
    {
        self.seal_in_place_separate_tag(nonce, aad, in_out.as_mut())
            .map(|tag| in_out.extend(tag.as_ref()))
    }

    
    
    
    
    #[inline]
    pub fn seal_in_place_separate_tag<A>(
        &self,
        nonce: Nonce,
        aad: Aad<A>,
        in_out: &mut [u8],
    ) -> Result<Tag, error::Unspecified>
    where
        A: AsRef<[u8]>,
    {
        self.algorithm.seal(
            &self.inner,
            nonce,
            Aad::from(aad.as_ref()),
            in_out,
            cpu::features(),
        )
    }

    
    #[inline]
    pub fn algorithm(&self) -> &'static Algorithm {
        self.algorithm
    }

    pub(super) fn fmt_debug(
        &self,
        type_name: &'static str,
        f: &mut core::fmt::Formatter,
    ) -> Result<(), core::fmt::Error> {
        f.debug_struct(type_name)
            .field("algorithm", &self.algorithm())
            .finish()
    }
}

impl core::fmt::Debug for LessSafeKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        self.fmt_debug("LessSafeKey", f)
    }
}

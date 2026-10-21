













use super::Modulus;
use crate::{
    error,
    limb::{self, Limb},
};
use alloc::{boxed::Box, vec};
use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};


pub(super) struct BoxedLimbs<M> {
    limbs: Box<[Limb]>,

    
    m: PhantomData<M>,
}

impl<M> Deref for BoxedLimbs<M> {
    type Target = [Limb];
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.limbs
    }
}

impl<M> DerefMut for BoxedLimbs<M> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.limbs
    }
}



impl<M> Clone for BoxedLimbs<M> {
    fn clone(&self) -> Self {
        Self {
            limbs: self.limbs.clone(),
            m: self.m,
        }
    }
}

impl<M> BoxedLimbs<M> {
    pub(super) fn from_be_bytes_padded_less_than(
        input: untrusted::Input,
        m: &Modulus<M>,
    ) -> Result<Self, error::Unspecified> {
        let mut r = Self::zero(m.limbs().len());
        limb::parse_big_endian_and_pad_consttime(input, &mut r)?;
        limb::verify_limbs_less_than_limbs_leak_bit(&r, m.limbs())?;
        Ok(r)
    }

    pub(super) fn zero(len: usize) -> Self {
        Self {
            limbs: vec![0; len].into_boxed_slice(),
            m: PhantomData,
        }
    }

    pub(super) fn into_limbs(self) -> Box<[Limb]> {
        self.limbs
    }
}

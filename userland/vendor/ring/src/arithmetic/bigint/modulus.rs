use super::{
    super::montgomery::Unencoded, unwrap_impossible_len_mismatch_error, BoxedLimbs, Elem,
    OwnedModulusValue, PublicModulus, Storage, N0,
};
use crate::{
    bits::BitLength,
    cpu, error,
    limb::{self, Limb, LIMB_BITS},
    polyfill::LeadingZerosStripped,
};
use core::marker::PhantomData;

pub struct OwnedModulus<M> {
    inner: OwnedModulusValue<M>,

    n0: N0,
}

impl<M: PublicModulus> Clone for OwnedModulus<M> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            n0: self.n0,
        }
    }
}

impl<M> OwnedModulus<M> {
    pub(crate) fn from(n: OwnedModulusValue<M>) -> Self {

        #[allow(clippy::useless_conversion)]
        let n0 = {
            prefixed_extern! {
                fn bn_neg_inv_mod_r_u64(n: u64) -> u64;
            }

            let mut n_mod_r: u64 = u64::from(n.limbs()[0]);

            if N0::LIMBS_USED == 2 {

                debug_assert_eq!(LIMB_BITS, 32);
                n_mod_r |= u64::from(n.limbs()[1]) << 32;
            }
            N0::precalculated(unsafe { bn_neg_inv_mod_r_u64(n_mod_r) })
        };

        Self { inner: n, n0 }
    }

    pub fn to_elem<L>(&self, l: &Modulus<L>) -> Result<Elem<L, Unencoded>, error::Unspecified> {
        self.inner.verify_less_than(l)?;
        let mut limbs = BoxedLimbs::zero(l.limbs().len());
        limbs[..self.inner.limbs().len()].copy_from_slice(self.inner.limbs());
        Ok(Elem {
            limbs,
            encoding: PhantomData,
        })
    }

    pub(crate) fn modulus(&self, cpu_features: cpu::Features) -> Modulus<M> {
        Modulus {
            limbs: self.inner.limbs(),
            n0: self.n0,
            len_bits: self.len_bits(),
            m: PhantomData,
            cpu_features,
        }
    }

    pub fn len_bits(&self) -> BitLength {
        self.inner.len_bits()
    }
}

impl<M: PublicModulus> OwnedModulus<M> {
    pub fn be_bytes(&self) -> LeadingZerosStripped<impl ExactSizeIterator<Item = u8> + Clone + '_> {
        LeadingZerosStripped::new(limb::unstripped_be_bytes(self.inner.limbs()))
    }
}

pub struct Modulus<'a, M> {
    limbs: &'a [Limb],
    n0: N0,
    len_bits: BitLength,
    m: PhantomData<M>,
    cpu_features: cpu::Features,
}

impl<M> Modulus<'_, M> {
    pub(super) fn oneR(&self, out: &mut [Limb]) {
        assert_eq!(self.limbs.len(), out.len());

        let r = self.limbs.len() * LIMB_BITS;

        limb::limbs_negative_odd(out, self.limbs);

        let lg_m = self.len_bits().as_bits();
        let leading_zero_bits_in_m = r - lg_m;

        if leading_zero_bits_in_m != 0 {
            debug_assert!(leading_zero_bits_in_m < LIMB_BITS);

            *out.last_mut().unwrap() &= (!0) >> leading_zero_bits_in_m;

            for _ in 0..leading_zero_bits_in_m {
                limb::limbs_double_mod(out, self.limbs)
                    .unwrap_or_else(unwrap_impossible_len_mismatch_error);
            }
        }

    }

    pub fn alloc_zero(&self) -> Storage<M> {
        Storage {
            limbs: BoxedLimbs::zero(self.limbs.len()),
        }
    }

    #[inline]
    pub(super) fn limbs(&self) -> &[Limb] {
        self.limbs
    }

    #[inline]
    pub(super) fn n0(&self) -> &N0 {
        &self.n0
    }

    pub fn len_bits(&self) -> BitLength {
        self.len_bits
    }

    #[inline]
    pub(crate) fn cpu_features(&self) -> cpu::Features {
        self.cpu_features
    }
}

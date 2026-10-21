use super::{limb, BoxedLimbs, Limb, Modulus};
use crate::error;
use alloc::boxed::Box;

pub struct PrivateExponent {

    limbs: Box<[Limb]>,
}

impl PrivateExponent {

    pub fn from_be_bytes_padded<M>(
        input: untrusted::Input,
        p: &Modulus<M>,
    ) -> Result<Self, error::Unspecified> {
        let mut dP = BoxedLimbs::from_be_bytes_padded_less_than(input, p)?;

        limb::limbs_reject_even_leak_bit(&dP)?;
        dP.reverse();

        Ok(Self {
            limbs: dP.into_limbs(),
        })
    }

    #[cfg(test)]
    pub fn from_be_bytes_for_test_only<M>(
        input: untrusted::Input,
        p: &Modulus<M>,
    ) -> Result<Self, error::Unspecified> {
        use crate::limb::LIMB_BYTES;

        if let r @ Ok(_) = Self::from_be_bytes_padded(input, p) {
            return r;
        }

        let num_limbs = (input.len() + LIMB_BYTES - 1) / LIMB_BYTES;
        let mut limbs = BoxedLimbs::<M>::zero(num_limbs);
        limb::parse_big_endian_and_pad_consttime(input, &mut limbs)
            .map_err(|error::Unspecified| error::KeyRejected::unexpected_error())?;
        limbs.reverse();
        Ok(Self {
            limbs: limbs.into_limbs(),
        })
    }

    #[inline]
    pub(super) fn limbs(&self) -> &[Limb] {
        &self.limbs
    }
}

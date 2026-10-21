













use super::{PublicExponent, PublicModulus, N, PUBLIC_KEY_PUBLIC_MODULUS_MAX_LEN};
use crate::{
    arithmetic::bigint,
    bits, cpu, error,
    io::{self, der, der_writer},
    limb::LIMB_BYTES,
};
use alloc::boxed::Box;
use core::num::NonZeroU64;


#[derive(Clone)]
pub struct PublicKey {
    inner: Inner,
    serialized: Box<[u8]>,
}

derive_debug_self_as_ref_hex_bytes!(PublicKey);

impl PublicKey {
    pub(super) fn from_modulus_and_exponent(
        n: untrusted::Input,
        e: untrusted::Input,
        n_min_bits: bits::BitLength,
        n_max_bits: bits::BitLength,
        e_min_value: PublicExponent,
        cpu_features: cpu::Features,
    ) -> Result<Self, error::KeyRejected> {
        let inner = Inner::from_modulus_and_exponent(
            n,
            e,
            n_min_bits,
            n_max_bits,
            e_min_value,
            cpu_features,
        )?;

        let n_bytes = n;
        let e_bytes = e;

        
        
        
        
        let n_bytes = io::Positive::from_be_bytes(n_bytes)
            .map_err(|_: error::Unspecified| error::KeyRejected::unexpected_error())?;
        let e_bytes = io::Positive::from_be_bytes(e_bytes)
            .map_err(|_: error::Unspecified| error::KeyRejected::unexpected_error())?;
        let serialized = der_writer::write_all(der::Tag::Sequence, &|output| {
            der_writer::write_positive_integer(output, &n_bytes)?;
            der_writer::write_positive_integer(output, &e_bytes)
        })
        .map_err(|_: io::TooLongError| error::KeyRejected::unexpected_error())?;

        Ok(Self { inner, serialized })
    }

    
    
    
    
    pub fn modulus_len(&self) -> usize {
        self.inner.n().len_bits().as_usize_bytes_rounded_up()
    }

    pub(super) fn inner(&self) -> &Inner {
        &self.inner
    }
}



#[derive(Clone)]
pub(crate) struct Inner {
    n: PublicModulus,
    e: PublicExponent,
}

impl Inner {
    pub(super) fn from_modulus_and_exponent(
        n: untrusted::Input,
        e: untrusted::Input,
        n_min_bits: bits::BitLength,
        n_max_bits: bits::BitLength,
        e_min_value: PublicExponent,
        cpu_features: cpu::Features,
    ) -> Result<Self, error::KeyRejected> {
        
        
        
        
        
        
        
        

        let n = PublicModulus::from_be_bytes(n, n_min_bits..=n_max_bits, cpu_features)?;

        let e = PublicExponent::from_be_bytes(e, e_min_value)?;

        
        
        

        
        

        Ok(Self { n, e })
    }

    
    #[inline]
    pub(super) fn n(&self) -> &PublicModulus {
        &self.n
    }

    
    #[inline]
    pub(super) fn e(&self) -> PublicExponent {
        self.e
    }

    
    
    
    
    
    
    
    pub(super) fn exponentiate<'out>(
        &self,
        base: untrusted::Input,
        out_buffer: &'out mut [u8; PUBLIC_KEY_PUBLIC_MODULUS_MAX_LEN],
        cpu_features: cpu::Features,
    ) -> Result<&'out [u8], error::Unspecified> {
        let n = &self.n.value(cpu_features);

        
        
        if base.len() != self.n.len_bits().as_usize_bytes_rounded_up() {
            return Err(error::Unspecified);
        }

        

        
        let s = bigint::Elem::from_be_bytes_padded(base, n)?;
        if s.is_zero() {
            return Err(error::Unspecified);
        }

        
        let m = n.alloc_zero();
        let m = self.exponentiate_elem(m, &s, cpu_features);

        
        Ok(fill_be_bytes_n(m, self.n.len_bits(), out_buffer))
    }

    
    
    
    pub(super) fn exponentiate_elem(
        &self,
        out: bigint::Storage<N>,
        base: &bigint::Elem<N>,
        cpu_features: cpu::Features,
    ) -> bigint::Elem<N> {
        
        let exponent_without_low_bit = NonZeroU64::try_from(self.e.value().get() & !1).unwrap();
        
        debug_assert_ne!(exponent_without_low_bit, self.e.value());

        let n = &self.n.value(cpu_features);

        let tmp = n.alloc_zero();
        let base_r = bigint::elem_mul_into(tmp, self.n.oneRR(), base, n);

        
        
        
        
        let acc = bigint::elem_exp_vartime(out, base_r, exponent_without_low_bit, n);

        
        bigint::elem_mul(base, acc, n)
    }
}


impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.serialized
    }
}






fn fill_be_bytes_n(
    elem: bigint::Elem<N>,
    n_bits: bits::BitLength,
    out: &mut [u8; PUBLIC_KEY_PUBLIC_MODULUS_MAX_LEN],
) -> &[u8] {
    let n_bytes = n_bits.as_usize_bytes_rounded_up();
    let n_bytes_padded = ((n_bytes + (LIMB_BYTES - 1)) / LIMB_BYTES) * LIMB_BYTES;
    let out = &mut out[..n_bytes_padded];
    elem.fill_be_bytes(out);
    let (padding, out) = out.split_at(n_bytes_padded - n_bytes);
    assert!(padding.iter().all(|&b| b == 0));
    out
}

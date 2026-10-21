use super::{
    padding::{self, RsaEncoding},
    KeyPairComponents, PublicExponent, PublicKey, PublicKeyComponents, N,
};

use crate::{
    arithmetic::{
        bigint,
        montgomery::{R, RR, RRR},
        LimbSliceError,
    },
    bits::BitLength,
    cpu, digest,
    error::{self, KeyRejected},
    io::der,
    pkcs8, rand, signature,
};

pub struct KeyPair {
    p: PrivateCrtPrime<P>,
    q: PrivateCrtPrime<Q>,
    qInv: bigint::Elem<P, R>,
    public: PublicKey,
}

derive_debug_via_field!(KeyPair, stringify!(RsaKeyPair), public);

impl KeyPair {

    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, KeyRejected> {
        const RSA_ENCRYPTION: &[u8] = include_bytes!("../data/alg-rsa-encryption.der");
        let (der, _) = pkcs8::unwrap_key_(
            untrusted::Input::from(RSA_ENCRYPTION),
            pkcs8::Version::V1Only,
            untrusted::Input::from(pkcs8),
        )?;
        Self::from_der(der.as_slice_less_safe())
    }

    pub fn from_der(input: &[u8]) -> Result<Self, KeyRejected> {
        untrusted::Input::from(input).read_all(KeyRejected::invalid_encoding(), |input| {
            der::nested(
                input,
                der::Tag::Sequence,
                KeyRejected::invalid_encoding(),
                Self::from_der_reader,
            )
        })
    }

    fn from_der_reader(input: &mut untrusted::Reader) -> Result<Self, KeyRejected> {
        let version = der::small_nonnegative_integer(input)
            .map_err(|error::Unspecified| KeyRejected::invalid_encoding())?;
        if version != 0 {
            return Err(KeyRejected::version_not_supported());
        }

        fn nonnegative_integer<'a>(
            input: &mut untrusted::Reader<'a>,
        ) -> Result<&'a [u8], KeyRejected> {
            der::nonnegative_integer(input)
                .map(|input| input.as_slice_less_safe())
                .map_err(|error::Unspecified| KeyRejected::invalid_encoding())
        }

        let n = nonnegative_integer(input)?;
        let e = nonnegative_integer(input)?;
        let d = nonnegative_integer(input)?;
        let p = nonnegative_integer(input)?;
        let q = nonnegative_integer(input)?;
        let dP = nonnegative_integer(input)?;
        let dQ = nonnegative_integer(input)?;
        let qInv = nonnegative_integer(input)?;

        let components = KeyPairComponents {
            public_key: PublicKeyComponents { n, e },
            d,
            p,
            q,
            dP,
            dQ,
            qInv,
        };

        Self::from_components(&components)
    }

    pub fn from_components<Public, Private>(
        components: &KeyPairComponents<Public, Private>,
    ) -> Result<Self, KeyRejected>
    where
        Public: AsRef<[u8]>,
        Private: AsRef<[u8]>,
    {
        let components = KeyPairComponents {
            public_key: PublicKeyComponents {
                n: components.public_key.n.as_ref(),
                e: components.public_key.e.as_ref(),
            },
            d: components.d.as_ref(),
            p: components.p.as_ref(),
            q: components.q.as_ref(),
            dP: components.dP.as_ref(),
            dQ: components.dQ.as_ref(),
            qInv: components.qInv.as_ref(),
        };
        Self::from_components_(&components, cpu::features())
    }

    fn from_components_(
        &KeyPairComponents {
            public_key,
            d,
            p,
            q,
            dP,
            dQ,
            qInv,
        }: &KeyPairComponents<&[u8]>,
        cpu_features: cpu::Features,
    ) -> Result<Self, KeyRejected> {
        let d = untrusted::Input::from(d);
        let p = untrusted::Input::from(p);
        let q = untrusted::Input::from(q);
        let dP = untrusted::Input::from(dP);
        let dQ = untrusted::Input::from(dQ);
        let qInv = untrusted::Input::from(qInv);

        let n = untrusted::Input::from(public_key.n);
        let e = untrusted::Input::from(public_key.e);
        let public_key = PublicKey::from_modulus_and_exponent(
            n,
            e,
            BitLength::from_bits(2048),
            super::PRIVATE_KEY_PUBLIC_MODULUS_MAX_BITS,
            PublicExponent::_65537,
            cpu_features,
        )?;

        let n_one = public_key.inner().n().oneRR();
        let n = &public_key.inner().n().value(cpu_features);

        let n_bits = public_key.inner().n().len_bits();

        let p = PrivatePrime::new(p, n_bits, cpu_features)?;
        let q = PrivatePrime::new(q, n_bits, cpu_features)?;

        let q_mod_n = q
            .modulus
            .to_elem(n)
            .map_err(|error::Unspecified| KeyRejected::inconsistent_components())?;
        let p_mod_n = p
            .modulus
            .to_elem(n)
            .map_err(|error::Unspecified| KeyRejected::inconsistent_components())?;
        let p_mod_n = bigint::elem_mul(n_one, p_mod_n, n);
        let pq_mod_n = bigint::elem_mul(&q_mod_n, p_mod_n, n);
        if !pq_mod_n.is_zero() {
            return Err(KeyRejected::inconsistent_components());
        }

        let d = bigint::OwnedModulusValue::<D>::from_be_bytes(d)
            .map_err(|_| KeyRejected::invalid_component())?;
        if !(n_bits.half_rounded_up() < d.len_bits()) {
            return Err(KeyRejected::inconsistent_components());
        }

        d.verify_less_than(n)
            .map_err(|error::Unspecified| KeyRejected::inconsistent_components())?;

        let pm = &p.modulus.modulus(cpu_features);

        let qInv = bigint::Elem::from_be_bytes_padded(qInv, pm)
            .map_err(|error::Unspecified| KeyRejected::invalid_component())?;

        let qInv = bigint::elem_mul(p.oneRR.as_ref(), qInv, pm);
        let q_mod_p = bigint::elem_reduced(pm.alloc_zero(), &q_mod_n, pm, q.modulus.len_bits());
        let q_mod_p = bigint::elem_mul(p.oneRR.as_ref(), q_mod_p, pm);
        bigint::verify_inverses_consttime(&qInv, q_mod_p, pm)
            .map_err(|error::Unspecified| KeyRejected::inconsistent_components())?;

        let p = PrivateCrtPrime::new(p, dP, cpu_features)?;
        let q = PrivateCrtPrime::new(q, dQ, cpu_features)?;

        Ok(Self {
            p,
            q,
            qInv,
            public: public_key,
        })
    }

    pub fn public(&self) -> &PublicKey {
        &self.public
    }

    #[deprecated = "Use `public().modulus_len()`"]
    #[inline]
    pub fn public_modulus_len(&self) -> usize {
        self.public().modulus_len()
    }
}

impl signature::KeyPair for KeyPair {
    type PublicKey = PublicKey;

    fn public_key(&self) -> &Self::PublicKey {
        self.public()
    }
}

struct PrivatePrime<M> {
    modulus: bigint::OwnedModulus<M>,
    oneRR: bigint::One<M, RR>,
}

impl<M> PrivatePrime<M> {
    fn new(
        p: untrusted::Input,
        n_bits: BitLength,
        cpu_features: cpu::Features,
    ) -> Result<Self, KeyRejected> {
        let p = bigint::OwnedModulusValue::from_be_bytes(p)?;

        if p.len_bits() != n_bits.half_rounded_up() {
            return Err(KeyRejected::inconsistent_components());
        }

        if p.len_bits().as_bits() % 512 != 0 {
            return Err(KeyRejected::private_modulus_len_not_multiple_of_512_bits());
        }

        let p = bigint::OwnedModulus::from(p);
        let pm = p.modulus(cpu_features);
        let oneRR = bigint::One::newRR(pm.alloc_zero(), &pm);

        Ok(Self { modulus: p, oneRR })
    }
}

struct PrivateCrtPrime<M> {
    modulus: bigint::OwnedModulus<M>,
    oneRRR: bigint::One<M, RRR>,
    exponent: bigint::PrivateExponent,
}

impl<M> PrivateCrtPrime<M> {

    fn new(
        p: PrivatePrime<M>,
        dP: untrusted::Input,
        cpu_features: cpu::Features,
    ) -> Result<Self, KeyRejected> {
        let m = &p.modulus.modulus(cpu_features);

        let dP = bigint::PrivateExponent::from_be_bytes_padded(dP, m)
            .map_err(|error::Unspecified| KeyRejected::inconsistent_components())?;

        let oneRRR = bigint::One::newRRR(p.oneRR, m);

        Ok(Self {
            modulus: p.modulus,
            oneRRR,
            exponent: dP,
        })
    }
}

fn elem_exp_consttime<M>(
    c: &bigint::Elem<N>,
    p: &PrivateCrtPrime<M>,
    other_prime_len_bits: BitLength,
    cpu_features: cpu::Features,
) -> Result<bigint::Elem<M>, error::Unspecified> {
    let m = &p.modulus.modulus(cpu_features);
    bigint::elem_exp_consttime(
        m.alloc_zero(),
        c,
        &p.oneRRR,
        &p.exponent,
        m,
        other_prime_len_bits,
    )
    .map_err(error::erase::<LimbSliceError>)
}

enum P {}

enum Q {}

enum D {}

impl KeyPair {

    pub fn sign(
        &self,
        padding_alg: &'static dyn RsaEncoding,
        rng: &dyn rand::SecureRandom,
        msg: &[u8],
        signature: &mut [u8],
    ) -> Result<(), error::Unspecified> {
        let cpu_features = cpu::features();

        if signature.len() != self.public().modulus_len() {
            return Err(error::Unspecified);
        }

        let m_hash = digest::digest(padding_alg.digest_alg(), msg);

        padding::encode(
            padding_alg,
            m_hash,
            signature,
            self.public().inner().n().len_bits(),
            rng,
        )?;

        let m = self.private_exponentiate(signature, cpu_features)?;

        m.fill_be_bytes(signature);

        Ok(())
    }

    fn private_exponentiate(
        &self,
        base: &[u8],
        cpu_features: cpu::Features,
    ) -> Result<bigint::Elem<N>, error::Unspecified> {
        assert_eq!(base.len(), self.public().modulus_len());

        let n = &self.public.inner().n().value(cpu_features);
        let n_one = self.public.inner().n().oneRR();

        let base = bigint::Elem::from_be_bytes_padded(untrusted::Input::from(base), n)?;

        let c = base;

        let q_bits = self.q.modulus.len_bits();
        let m_1 = elem_exp_consttime(&c, &self.p, q_bits, cpu_features)?;
        let m_2 = elem_exp_consttime(&c, &self.q, self.p.modulus.len_bits(), cpu_features)?;

        let h = {
            let p = &self.p.modulus.modulus(cpu_features);
            let m_2 = bigint::elem_reduced_once(p.alloc_zero(), &m_2, p, q_bits);
            let m_1_minus_m_2 = bigint::elem_sub(m_1, &m_2, p);
            bigint::elem_mul(&self.qInv, m_1_minus_m_2, p)
        };

        let p_bits = self.p.modulus.len_bits();
        let h = bigint::elem_widen(n.alloc_zero(), h, n, p_bits)?;
        let q_mod_n = self.q.modulus.to_elem(n)?;
        let q_mod_n = bigint::elem_mul(n_one, q_mod_n, n);
        let q_times_h = bigint::elem_mul(&q_mod_n, h, n);
        let m_2 = bigint::elem_widen(n.alloc_zero(), m_2, n, q_bits)?;
        let m = bigint::elem_add(m_2, q_times_h, n);

        {
            let verify = n.alloc_zero();
            let verify = self
                .public
                .inner()
                .exponentiate_elem(verify, &m, cpu_features);
            bigint::elem_verify_equal_consttime(&verify, &c)?;
        }

        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil as test;
    use alloc::vec;

    #[test]
    fn test_rsakeypair_private_exponentiate() {
        let cpu = cpu::features();
        test::run(
            test_vector_file!("keypair_private_exponentiate_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                let key = test_case.consume_bytes("Key");
                let key = KeyPair::from_pkcs8(&key).unwrap();
                let test_cases = &[
                    test_case.consume_bytes("p"),
                    test_case.consume_bytes("p_plus_1"),
                    test_case.consume_bytes("p_minus_1"),
                    test_case.consume_bytes("q"),
                    test_case.consume_bytes("q_plus_1"),
                    test_case.consume_bytes("q_minus_1"),
                ];
                for test_case in test_cases {

                    let mut padded = vec![0; key.public.modulus_len()];
                    let zeroes = padded.len() - test_case.len();
                    padded[zeroes..].copy_from_slice(test_case);
                    let _: bigint::Elem<_> = key.private_exponentiate(&padded, cpu).unwrap();
                }
                Ok(())
            },
        );
    }
}

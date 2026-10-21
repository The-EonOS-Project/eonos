















use super::digest_scalar::digest_scalar;
use crate::{
    arithmetic::montgomery::*,
    cpu, digest,
    ec::{
        self,
        suite_b::{ops::*, private_key},
    },
    error,
    io::der,
    limb, pkcs8, rand, sealed, signature,
};

pub struct EcdsaSigningAlgorithm {
    curve: &'static ec::Curve,
    private_scalar_ops: &'static PrivateScalarOps,
    private_key_ops: &'static PrivateKeyOps,
    digest_alg: &'static digest::Algorithm,
    pkcs8_template: &'static pkcs8::Template,
    format_rs: fn(ops: &'static ScalarOps, r: &Scalar, s: &Scalar, out: &mut [u8]) -> usize,
    id: AlgorithmID,
}

#[derive(Debug, Eq, PartialEq)]
enum AlgorithmID {
    ECDSA_P256_SHA256_FIXED_SIGNING,
    ECDSA_P384_SHA384_FIXED_SIGNING,
    ECDSA_P256_SHA256_ASN1_SIGNING,
    ECDSA_P384_SHA384_ASN1_SIGNING,
}

derive_debug_via_id!(EcdsaSigningAlgorithm);

impl PartialEq for EcdsaSigningAlgorithm {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for EcdsaSigningAlgorithm {}

impl sealed::Sealed for EcdsaSigningAlgorithm {}


pub struct EcdsaKeyPair {
    d: Scalar<R>,
    nonce_key: NonceRandomKey,
    alg: &'static EcdsaSigningAlgorithm,
    public_key: PublicKey,
}

derive_debug_via_field!(EcdsaKeyPair, stringify!(EcdsaKeyPair), public_key);

impl EcdsaKeyPair {
    
    
    
    
    
    
    
    
    
    
    
    pub fn generate_pkcs8(
        alg: &'static EcdsaSigningAlgorithm,
        rng: &dyn rand::SecureRandom,
    ) -> Result<pkcs8::Document, error::Unspecified> {
        let cpu = cpu::features();
        let private_key = ec::Seed::generate(alg.curve, rng, cpu)?;
        let public_key = private_key.compute_public_key(cpu)?;
        Ok(pkcs8::wrap_key(
            alg.pkcs8_template,
            private_key.bytes_less_safe(),
            public_key.as_ref(),
        ))
    }

    
    
    
    
    
    
    
    
    
    
    pub fn from_pkcs8(
        alg: &'static EcdsaSigningAlgorithm,
        pkcs8: &[u8],
        rng: &dyn rand::SecureRandom,
    ) -> Result<Self, error::KeyRejected> {
        let key_pair = ec::suite_b::key_pair_from_pkcs8(
            alg.curve,
            alg.pkcs8_template,
            untrusted::Input::from(pkcs8),
            cpu::features(),
        )?;
        Self::new(alg, key_pair, rng)
    }

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    pub fn from_private_key_and_public_key(
        alg: &'static EcdsaSigningAlgorithm,
        private_key: &[u8],
        public_key: &[u8],
        rng: &dyn rand::SecureRandom,
    ) -> Result<Self, error::KeyRejected> {
        let key_pair = ec::suite_b::key_pair_from_bytes(
            alg.curve,
            untrusted::Input::from(private_key),
            untrusted::Input::from(public_key),
            cpu::features(),
        )?;
        Self::new(alg, key_pair, rng)
    }

    fn new(
        alg: &'static EcdsaSigningAlgorithm,
        key_pair: ec::KeyPair,
        rng: &dyn rand::SecureRandom,
    ) -> Result<Self, error::KeyRejected> {
        let cpu = cpu::features();

        let (seed, public_key) = key_pair.split();
        let n = &alg.private_scalar_ops.scalar_ops.scalar_modulus(cpu);
        let d = private_key::private_key_as_scalar(n, &seed);
        let d = alg.private_scalar_ops.to_mont(&d, cpu);

        let nonce_key = NonceRandomKey::new(alg, &seed, rng)?;
        Ok(Self {
            d,
            nonce_key,
            alg,
            public_key: PublicKey(public_key),
        })
    }

    
    pub fn sign(
        &self,
        rng: &dyn rand::SecureRandom,
        message: &[u8],
    ) -> Result<signature::Signature, error::Unspecified> {
        let cpu = cpu::features();

        
        let h = digest::digest(self.alg.digest_alg, message);

        
        
        
        let nonce_rng = NonceRandom {
            key: &self.nonce_key,
            message_digest: &h,
            rng,
        };

        self.sign_digest(h, &nonce_rng, cpu)
    }

    #[cfg(test)]
    fn sign_with_fixed_nonce_during_test(
        &self,
        rng: &dyn rand::SecureRandom,
        message: &[u8],
    ) -> Result<signature::Signature, error::Unspecified> {
        
        let h = digest::digest(self.alg.digest_alg, message);

        self.sign_digest(h, rng, cpu::features())
    }

    
    
    fn sign_digest(
        &self,
        h: digest::Digest,
        rng: &dyn rand::SecureRandom,
        cpu: cpu::Features,
    ) -> Result<signature::Signature, error::Unspecified> {
        
        

        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        

        let ops = self.alg.private_scalar_ops;
        let scalar_ops = ops.scalar_ops;
        let cops = scalar_ops.common;
        let private_key_ops = self.alg.private_key_ops;
        let q = &cops.elem_modulus(cpu);
        let n = &scalar_ops.scalar_modulus(cpu);

        for _ in 0..100 {
            
            
            let k = private_key::random_scalar(self.alg.private_key_ops, n, rng)?;
            let k_inv = ops.scalar_inv_to_mont(&k, cpu);

            
            let r = private_key_ops.point_mul_base(&k, cpu);

            
            let r = {
                let (x, _) = private_key::affine_from_jacobian(private_key_ops, q, &r)?;
                let x = q.elem_unencoded(&x);
                n.elem_reduced_to_scalar(&x)
            };
            if n.is_zero(&r) {
                continue;
            }

            

            
            let e = digest_scalar(n, h);

            
            let s = {
                let mut e_plus_dr = scalar_ops.scalar_product(&self.d, &r, cpu);
                n.add_assign(&mut e_plus_dr, &e);
                scalar_ops.scalar_product(&k_inv, &e_plus_dr, cpu)
            };
            if n.is_zero(&s) {
                continue;
            }

            
            return Ok(signature::Signature::new(|sig_bytes| {
                (self.alg.format_rs)(scalar_ops, &r, &s, sig_bytes)
            }));
        }

        Err(error::Unspecified)
    }
}



struct NonceRandom<'a> {
    key: &'a NonceRandomKey,
    message_digest: &'a digest::Digest,
    rng: &'a dyn rand::SecureRandom,
}

impl core::fmt::Debug for NonceRandom<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NonceRandom").finish()
    }
}

impl rand::sealed::SecureRandom for NonceRandom<'_> {
    fn fill_impl(&self, dest: &mut [u8]) -> Result<(), error::Unspecified> {
        
        
        
        
        
        
        
        let digest_alg = self.key.0.algorithm();
        let mut ctx = digest::Context::new(digest_alg);

        
        let key = self.key.0.as_ref();
        ctx.update(key);

        
        
        
        assert!(key.len() <= digest_alg.block_len() / 2);
        {
            let mut rand = [0u8; digest::MAX_BLOCK_LEN];
            let rand = &mut rand[..digest_alg.block_len() - key.len()];
            assert!(rand.len() >= dest.len());
            self.rng.fill(rand)?;
            ctx.update(rand);
        }

        ctx.update(self.message_digest.as_ref());

        let nonce = ctx.finish();

        
        
        dest.copy_from_slice(nonce.as_ref());

        Ok(())
    }
}

impl sealed::Sealed for NonceRandom<'_> {}

struct NonceRandomKey(digest::Digest);

impl NonceRandomKey {
    fn new(
        alg: &EcdsaSigningAlgorithm,
        seed: &ec::Seed,
        rng: &dyn rand::SecureRandom,
    ) -> Result<Self, error::KeyRejected> {
        let mut rand = [0; digest::MAX_OUTPUT_LEN];
        let rand = &mut rand[0..alg.curve.elem_scalar_seed_len];

        
        
        
        rng.fill(rand)
            .map_err(|error::Unspecified| error::KeyRejected::rng_failed())?;

        let mut ctx = digest::Context::new(alg.digest_alg);
        ctx.update(rand);
        ctx.update(seed.bytes_less_safe());
        Ok(Self(ctx.finish()))
    }
}

impl signature::KeyPair for EcdsaKeyPair {
    type PublicKey = PublicKey;

    fn public_key(&self) -> &Self::PublicKey {
        &self.public_key
    }
}

#[derive(Clone, Copy)]
pub struct PublicKey(ec::PublicKey);

derive_debug_self_as_ref_hex_bytes!(PublicKey);

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

fn format_rs_fixed(ops: &'static ScalarOps, r: &Scalar, s: &Scalar, out: &mut [u8]) -> usize {
    let scalar_len = ops.scalar_bytes_len();

    let (r_out, rest) = out.split_at_mut(scalar_len);
    limb::big_endian_from_limbs(ops.leak_limbs(r), r_out);

    let (s_out, _) = rest.split_at_mut(scalar_len);
    limb::big_endian_from_limbs(ops.leak_limbs(s), s_out);

    2 * scalar_len
}

fn format_rs_asn1(ops: &'static ScalarOps, r: &Scalar, s: &Scalar, out: &mut [u8]) -> usize {
    
    
    fn format_integer_tlv(ops: &ScalarOps, a: &Scalar, out: &mut [u8]) -> usize {
        let mut fixed = [0u8; ec::SCALAR_MAX_BYTES + 1];
        let fixed = &mut fixed[..(ops.scalar_bytes_len() + 1)];
        limb::big_endian_from_limbs(ops.leak_limbs(a), &mut fixed[1..]);

        
        
        debug_assert_eq!(fixed[0], 0);

        
        let first_index = fixed.iter().position(|b| *b != 0).unwrap();

        
        let first_index = if fixed[first_index] & 0x80 != 0 {
            first_index - 1
        } else {
            first_index
        };
        let value = &fixed[first_index..];

        out[0] = der::Tag::Integer.into();

        
        assert!(value.len() < 128);
        #[allow(clippy::cast_possible_truncation)]
        {
            out[1] = value.len() as u8;
        }

        out[2..][..value.len()].copy_from_slice(value);

        2 + value.len()
    }

    out[0] = der::Tag::Sequence.into();
    let r_tlv_len = format_integer_tlv(ops, r, &mut out[2..]);
    let s_tlv_len = format_integer_tlv(ops, s, &mut out[2..][r_tlv_len..]);

    
    let value_len = r_tlv_len + s_tlv_len;
    assert!(value_len < 128);
    #[allow(clippy::cast_possible_truncation)]
    {
        out[1] = value_len as u8;
    }

    2 + value_len
}






pub static ECDSA_P256_SHA256_FIXED_SIGNING: EcdsaSigningAlgorithm = EcdsaSigningAlgorithm {
    curve: &ec::suite_b::curve::P256,
    private_scalar_ops: &p256::PRIVATE_SCALAR_OPS,
    private_key_ops: &p256::PRIVATE_KEY_OPS,
    digest_alg: &digest::SHA256,
    pkcs8_template: &EC_PUBLIC_KEY_P256_PKCS8_V1_TEMPLATE,
    format_rs: format_rs_fixed,
    id: AlgorithmID::ECDSA_P256_SHA256_FIXED_SIGNING,
};






pub static ECDSA_P384_SHA384_FIXED_SIGNING: EcdsaSigningAlgorithm = EcdsaSigningAlgorithm {
    curve: &ec::suite_b::curve::P384,
    private_scalar_ops: &p384::PRIVATE_SCALAR_OPS,
    private_key_ops: &p384::PRIVATE_KEY_OPS,
    digest_alg: &digest::SHA384,
    pkcs8_template: &EC_PUBLIC_KEY_P384_PKCS8_V1_TEMPLATE,
    format_rs: format_rs_fixed,
    id: AlgorithmID::ECDSA_P384_SHA384_FIXED_SIGNING,
};






pub static ECDSA_P256_SHA256_ASN1_SIGNING: EcdsaSigningAlgorithm = EcdsaSigningAlgorithm {
    curve: &ec::suite_b::curve::P256,
    private_scalar_ops: &p256::PRIVATE_SCALAR_OPS,
    private_key_ops: &p256::PRIVATE_KEY_OPS,
    digest_alg: &digest::SHA256,
    pkcs8_template: &EC_PUBLIC_KEY_P256_PKCS8_V1_TEMPLATE,
    format_rs: format_rs_asn1,
    id: AlgorithmID::ECDSA_P256_SHA256_ASN1_SIGNING,
};






pub static ECDSA_P384_SHA384_ASN1_SIGNING: EcdsaSigningAlgorithm = EcdsaSigningAlgorithm {
    curve: &ec::suite_b::curve::P384,
    private_scalar_ops: &p384::PRIVATE_SCALAR_OPS,
    private_key_ops: &p384::PRIVATE_KEY_OPS,
    digest_alg: &digest::SHA384,
    pkcs8_template: &EC_PUBLIC_KEY_P384_PKCS8_V1_TEMPLATE,
    format_rs: format_rs_asn1,
    id: AlgorithmID::ECDSA_P384_SHA384_ASN1_SIGNING,
};

static EC_PUBLIC_KEY_P256_PKCS8_V1_TEMPLATE: pkcs8::Template = pkcs8::Template {
    bytes: include_bytes!("ecPublicKey_p256_pkcs8_v1_template.der"),
    alg_id_range: core::ops::Range { start: 8, end: 27 },
    curve_id_index: 9,
    private_key_index: 0x24,
};

static EC_PUBLIC_KEY_P384_PKCS8_V1_TEMPLATE: pkcs8::Template = pkcs8::Template {
    bytes: include_bytes!("ecPublicKey_p384_pkcs8_v1_template.der"),
    alg_id_range: core::ops::Range { start: 8, end: 24 },
    curve_id_index: 9,
    private_key_index: 0x23,
};

#[cfg(test)]
mod tests {
    use crate::testutil as test;
    use crate::{rand, signature};

    #[test]
    fn signature_ecdsa_sign_fixed_test() {
        let rng = rand::SystemRandom::new();

        test::run(
            test_vector_file!("ecdsa_sign_fixed_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                let curve_name = test_case.consume_string("Curve");
                let digest_name = test_case.consume_string("Digest");
                let msg = test_case.consume_bytes("Msg");
                let d = test_case.consume_bytes("d");
                let q = test_case.consume_bytes("Q");
                let k = test_case.consume_bytes("k");

                let expected_result = test_case.consume_bytes("Sig");

                let alg = match (curve_name.as_str(), digest_name.as_str()) {
                    ("P-256", "SHA256") => &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
                    ("P-384", "SHA384") => &signature::ECDSA_P384_SHA384_FIXED_SIGNING,
                    _ => {
                        panic!("Unsupported curve+digest: {}+{}", curve_name, digest_name);
                    }
                };

                let private_key =
                    signature::EcdsaKeyPair::from_private_key_and_public_key(alg, &d, &q, &rng)
                        .unwrap();
                let rng = test::rand::FixedSliceRandom { bytes: &k };

                let actual_result = private_key
                    .sign_with_fixed_nonce_during_test(&rng, &msg)
                    .unwrap();

                assert_eq!(actual_result.as_ref(), &expected_result[..]);

                Ok(())
            },
        );
    }

    #[test]
    fn signature_ecdsa_sign_asn1_test() {
        let rng = rand::SystemRandom::new();

        test::run(
            test_vector_file!("ecdsa_sign_asn1_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                let curve_name = test_case.consume_string("Curve");
                let digest_name = test_case.consume_string("Digest");
                let msg = test_case.consume_bytes("Msg");
                let d = test_case.consume_bytes("d");
                let q = test_case.consume_bytes("Q");
                let k = test_case.consume_bytes("k");

                let expected_result = test_case.consume_bytes("Sig");

                let alg = match (curve_name.as_str(), digest_name.as_str()) {
                    ("P-256", "SHA256") => &signature::ECDSA_P256_SHA256_ASN1_SIGNING,
                    ("P-384", "SHA384") => &signature::ECDSA_P384_SHA384_ASN1_SIGNING,
                    _ => {
                        panic!("Unsupported curve+digest: {}+{}", curve_name, digest_name);
                    }
                };

                let private_key =
                    signature::EcdsaKeyPair::from_private_key_and_public_key(alg, &d, &q, &rng)
                        .unwrap();
                let rng = test::rand::FixedSliceRandom { bytes: &k };

                let actual_result = private_key
                    .sign_with_fixed_nonce_during_test(&rng, &msg)
                    .unwrap();

                assert_eq!(actual_result.as_ref(), &expected_result[..]);

                Ok(())
            },
        );
    }
}

















use super::{super::ops::*, eddsa_digest, ED25519_PUBLIC_KEY_LEN};
use crate::{
    cpu, digest, error,
    io::der,
    pkcs8, rand,
    signature::{self, KeyPair as SigningKeyPair},
};


pub struct Ed25519KeyPair {
    
    private_scalar: Scalar,

    
    private_prefix: Prefix,

    
    public_key: PublicKey,
}

derive_debug_via_field!(Ed25519KeyPair, stringify!(Ed25519KeyPair), public_key);

impl Ed25519KeyPair {
    
    
    
    
    
    
    
    
    
    pub fn generate_pkcs8(
        rng: &dyn rand::SecureRandom,
    ) -> Result<pkcs8::Document, error::Unspecified> {
        let cpu_features = cpu::features();
        let seed: [u8; SEED_LEN] = rand::generate(rng)?.expose();
        let key_pair = Self::from_seed_(&seed, cpu_features);
        Ok(pkcs8::wrap_key(
            &PKCS8_TEMPLATE,
            &seed[..],
            key_pair.public_key().as_ref(),
        ))
    }

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, error::KeyRejected> {
        let version = pkcs8::Version::V2Only(pkcs8::PublicKeyOptions {
            accept_legacy_ed25519_public_key_tag: true,
        });
        let (seed, public_key) = unwrap_pkcs8(version, untrusted::Input::from(pkcs8))?;
        Self::from_seed_and_public_key(
            seed.as_slice_less_safe(),
            public_key.unwrap().as_slice_less_safe(),
        )
    }

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    pub fn from_pkcs8_maybe_unchecked(pkcs8: &[u8]) -> Result<Self, error::KeyRejected> {
        let version = pkcs8::Version::V1OrV2(pkcs8::PublicKeyOptions {
            accept_legacy_ed25519_public_key_tag: true,
        });
        let (seed, public_key) = unwrap_pkcs8(version, untrusted::Input::from(pkcs8))?;
        if let Some(public_key) = public_key {
            Self::from_seed_and_public_key(
                seed.as_slice_less_safe(),
                public_key.as_slice_less_safe(),
            )
        } else {
            Self::from_seed_unchecked(seed.as_slice_less_safe())
        }
    }

    
    
    
    
    
    
    
    
    
    
    pub fn from_seed_and_public_key(
        seed: &[u8],
        public_key: &[u8],
    ) -> Result<Self, error::KeyRejected> {
        let pair = Self::from_seed_unchecked(seed)?;

        
        
        
        if public_key != pair.public_key.as_ref() {
            let err = if public_key.len() != pair.public_key.as_ref().len() {
                error::KeyRejected::invalid_encoding()
            } else {
                error::KeyRejected::inconsistent_components()
            };
            return Err(err);
        }

        Ok(pair)
    }

    
    
    
    
    
    
    
    
    
    pub fn from_seed_unchecked(seed: &[u8]) -> Result<Self, error::KeyRejected> {
        let seed = seed
            .try_into()
            .map_err(|_| error::KeyRejected::invalid_encoding())?;
        Ok(Self::from_seed_(seed, cpu::features()))
    }

    fn from_seed_(seed: &Seed, cpu_features: cpu::Features) -> Self {
        let h = digest::digest(&digest::SHA512, seed);
        let (private_scalar, private_prefix) = h.as_ref().split_at(SCALAR_LEN);

        let private_scalar =
            MaskedScalar::from_bytes_masked(private_scalar.try_into().unwrap()).into();

        let a = ExtPoint::from_scalarmult_base(&private_scalar, cpu_features);

        Self {
            private_scalar,
            private_prefix: private_prefix.try_into().unwrap(),
            public_key: PublicKey(a.into_encoded_point(cpu_features)),
        }
    }

    
    pub fn sign(&self, msg: &[u8]) -> signature::Signature {
        let cpu_features = cpu::features();
        signature::Signature::new(|signature_bytes| {
            prefixed_extern! {
                fn x25519_sc_muladd(
                    s: &mut [u8; SCALAR_LEN],
                    a: &Scalar,
                    b: &Scalar,
                    c: &Scalar,
                );
            }

            let (signature_bytes, _unused) = signature_bytes.split_at_mut(ELEM_LEN + SCALAR_LEN);
            let (signature_r, signature_s) = signature_bytes.split_at_mut(ELEM_LEN);
            let nonce = {
                let mut ctx = digest::Context::new(&digest::SHA512);
                ctx.update(&self.private_prefix);
                ctx.update(msg);
                ctx.finish()
            };
            let nonce = Scalar::from_sha512_digest_reduced(nonce);

            let r = ExtPoint::from_scalarmult_base(&nonce, cpu_features);
            signature_r.copy_from_slice(&r.into_encoded_point(cpu_features));
            let hram_digest = eddsa_digest(signature_r, self.public_key.as_ref(), msg);
            let hram = Scalar::from_sha512_digest_reduced(hram_digest);
            unsafe {
                x25519_sc_muladd(
                    signature_s.try_into().unwrap(),
                    &hram,
                    &self.private_scalar,
                    &nonce,
                );
            }

            SIGNATURE_LEN
        })
    }
}

impl signature::KeyPair for Ed25519KeyPair {
    type PublicKey = PublicKey;

    fn public_key(&self) -> &Self::PublicKey {
        &self.public_key
    }
}

#[derive(Clone, Copy)]
pub struct PublicKey([u8; ED25519_PUBLIC_KEY_LEN]);

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

derive_debug_self_as_ref_hex_bytes!(PublicKey);

fn unwrap_pkcs8(
    version: pkcs8::Version,
    input: untrusted::Input,
) -> Result<(untrusted::Input, Option<untrusted::Input>), error::KeyRejected> {
    let (private_key, public_key) = pkcs8::unwrap_key(&PKCS8_TEMPLATE, version, input)?;
    let private_key = private_key
        .read_all(error::Unspecified, |input| {
            der::expect_tag_and_get_value(input, der::Tag::OctetString)
        })
        .map_err(|error::Unspecified| error::KeyRejected::invalid_encoding())?;
    Ok((private_key, public_key))
}

type Prefix = [u8; PREFIX_LEN];
const PREFIX_LEN: usize = digest::SHA512_OUTPUT_LEN - SCALAR_LEN;

const SIGNATURE_LEN: usize = ELEM_LEN + SCALAR_LEN;

type Seed = [u8; SEED_LEN];
const SEED_LEN: usize = 32;

static PKCS8_TEMPLATE: pkcs8::Template = pkcs8::Template {
    bytes: include_bytes!("ed25519_pkcs8_v2_template.der"),
    alg_id_range: core::ops::Range { start: 7, end: 12 },
    curve_id_index: 0,
    private_key_index: 0x10,
};


































































































































































































































































use crate::{cpu, debug, ec, error, sealed};

pub use crate::ec::{
    curve25519::ed25519::{
        signing::Ed25519KeyPair,
        verification::{EdDSAParameters, ED25519},
        ED25519_PUBLIC_KEY_LEN,
    },
    suite_b::ecdsa::{
        signing::{
            EcdsaKeyPair, EcdsaSigningAlgorithm, ECDSA_P256_SHA256_ASN1_SIGNING,
            ECDSA_P256_SHA256_FIXED_SIGNING, ECDSA_P384_SHA384_ASN1_SIGNING,
            ECDSA_P384_SHA384_FIXED_SIGNING,
        },
        verification::{
            EcdsaVerificationAlgorithm, ECDSA_P256_SHA256_ASN1, ECDSA_P256_SHA256_FIXED,
            ECDSA_P256_SHA384_ASN1, ECDSA_P384_SHA256_ASN1, ECDSA_P384_SHA384_ASN1,
            ECDSA_P384_SHA384_FIXED,
        },
    },
};

#[cfg(feature = "alloc")]
pub use crate::rsa::{
    padding::{
        RsaEncoding, RSA_PKCS1_SHA256, RSA_PKCS1_SHA384, RSA_PKCS1_SHA512, RSA_PSS_SHA256,
        RSA_PSS_SHA384, RSA_PSS_SHA512,
    },
    verification::{
        RsaPublicKeyComponents, RSA_PKCS1_1024_8192_SHA1_FOR_LEGACY_USE_ONLY,
        RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY,
        RSA_PKCS1_1024_8192_SHA512_FOR_LEGACY_USE_ONLY,
        RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY, RSA_PKCS1_2048_8192_SHA256,
        RSA_PKCS1_2048_8192_SHA384, RSA_PKCS1_2048_8192_SHA512, RSA_PKCS1_3072_8192_SHA384,
        RSA_PSS_2048_8192_SHA256, RSA_PSS_2048_8192_SHA384, RSA_PSS_2048_8192_SHA512,
    },
    RsaParameters,
};


#[cfg(feature = "alloc")]
pub type RsaKeyPair = crate::rsa::KeyPair;


#[derive(Clone, Copy)]
pub struct Signature {
    value: [u8; MAX_LEN],
    len: usize,
}

impl Signature {
    
    pub(crate) fn new<F>(fill: F) -> Self
    where
        F: FnOnce(&mut [u8; MAX_LEN]) -> usize,
    {
        let mut r = Self {
            value: [0; MAX_LEN],
            len: 0,
        };
        r.len = fill(&mut r.value);
        r
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.value[..self.len]
    }
}


pub trait KeyPair: core::fmt::Debug + Send + Sized + Sync {
    
    type PublicKey: AsRef<[u8]> + core::fmt::Debug + Clone + Send + Sized + Sync;

    
    fn public_key(&self) -> &Self::PublicKey;
}





pub(crate) const MAX_LEN: usize = 1    + 2    +
    (2 * (1    + 1    + 1    + ec::SCALAR_MAX_BYTES));


pub trait VerificationAlgorithm: core::fmt::Debug + Sync + sealed::Sealed {
    
    
    fn verify(
        &self,
        public_key: untrusted::Input,
        msg: untrusted::Input,
        signature: untrusted::Input,
    ) -> Result<(), error::Unspecified>;
}


#[derive(Clone, Copy)]
pub struct UnparsedPublicKey<B> {
    algorithm: &'static dyn VerificationAlgorithm,
    bytes: B,
}

impl<B> AsRef<[u8]> for UnparsedPublicKey<B>
where
    B: AsRef<[u8]>,
{
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl<B: core::fmt::Debug> core::fmt::Debug for UnparsedPublicKey<B>
where
    B: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        f.debug_struct("UnparsedPublicKey")
            .field("algorithm", &self.algorithm)
            .field("bytes", &debug::HexStr(self.bytes.as_ref()))
            .finish()
    }
}

impl<B> UnparsedPublicKey<B> {
    
    
    
    #[inline]
    pub fn new(algorithm: &'static dyn VerificationAlgorithm, bytes: B) -> Self {
        Self { algorithm, bytes }
    }

    
    
    
    
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), error::Unspecified>
    where
        B: AsRef<[u8]>,
    {
        let _ = cpu::features();
        self.algorithm.verify(
            untrusted::Input::from(self.bytes.as_ref()),
            untrusted::Input::from(message),
            untrusted::Input::from(signature),
        )
    }
}

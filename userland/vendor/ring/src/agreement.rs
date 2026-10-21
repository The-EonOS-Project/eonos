





























































use crate::{cpu, debug, ec, error, rand};

pub use crate::ec::{
    curve25519::x25519::X25519,
    suite_b::ecdh::{ECDH_P256, ECDH_P384},
};


pub struct Algorithm {
    pub(crate) curve: &'static ec::Curve,
    pub(crate) ecdh: fn(
        out: &mut [u8],
        private_key: &ec::Seed,
        peer_public_key: untrusted::Input,
        cpu: cpu::Features,
    ) -> Result<(), error::Unspecified>,
}

derive_debug_via_field!(Algorithm, curve);

impl Eq for Algorithm {}
impl PartialEq for Algorithm {
    fn eq(&self, other: &Self) -> bool {
        self.curve.id == other.curve.id
    }
}




pub struct EphemeralPrivateKey {
    private_key: ec::Seed,
    algorithm: &'static Algorithm,
}

derive_debug_via_field!(
    EphemeralPrivateKey,
    stringify!(EphemeralPrivateKey),
    algorithm
);

impl EphemeralPrivateKey {
    
    pub fn generate(
        alg: &'static Algorithm,
        rng: &dyn rand::SecureRandom,
    ) -> Result<Self, error::Unspecified> {
        let cpu_features = cpu::features();

        
        
        
        
        let private_key = ec::Seed::generate(alg.curve, rng, cpu_features)?;
        Ok(Self {
            private_key,
            algorithm: alg,
        })
    }

    
    #[inline(always)]
    pub fn compute_public_key(&self) -> Result<PublicKey, error::Unspecified> {
        
        
        
        
        
        self.private_key
            .compute_public_key(cpu::features())
            .map(|public_key| PublicKey {
                algorithm: self.algorithm,
                bytes: public_key,
            })
    }

    
    #[inline]
    pub fn algorithm(&self) -> &'static Algorithm {
        self.algorithm
    }

    
    #[deprecated]
    #[cfg(test)]
    pub fn bytes(&self) -> &[u8] {
        self.bytes_for_test()
    }

    #[cfg(test)]
    pub(super) fn bytes_for_test(&self) -> &[u8] {
        self.private_key.bytes_less_safe()
    }
}


#[derive(Clone)]
pub struct PublicKey {
    algorithm: &'static Algorithm,
    bytes: ec::PublicKey,
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl core::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        f.debug_struct("PublicKey")
            .field("algorithm", &self.algorithm)
            .field("bytes", &debug::HexStr(self.as_ref()))
            .finish()
    }
}

impl PublicKey {
    
    #[inline]
    pub fn algorithm(&self) -> &'static Algorithm {
        self.algorithm
    }
}


#[derive(Clone, Copy)]
pub struct UnparsedPublicKey<B> {
    algorithm: &'static Algorithm,
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
    
    pub fn new(algorithm: &'static Algorithm, bytes: B) -> Self {
        Self { algorithm, bytes }
    }

    
    #[inline]
    pub fn algorithm(&self) -> &'static Algorithm {
        self.algorithm
    }

    
    #[inline]
    pub fn bytes(&self) -> &B {
        &self.bytes
    }
}


















#[inline]
pub fn agree_ephemeral<B: AsRef<[u8]>, R>(
    my_private_key: EphemeralPrivateKey,
    peer_public_key: &UnparsedPublicKey<B>,
    kdf: impl FnOnce(&[u8]) -> R,
) -> Result<R, error::Unspecified> {
    let peer_public_key = UnparsedPublicKey {
        algorithm: peer_public_key.algorithm,
        bytes: peer_public_key.bytes.as_ref(),
    };
    agree_ephemeral_(my_private_key, peer_public_key, kdf, cpu::features())
}

fn agree_ephemeral_<R>(
    my_private_key: EphemeralPrivateKey,
    peer_public_key: UnparsedPublicKey<&[u8]>,
    kdf: impl FnOnce(&[u8]) -> R,
    cpu: cpu::Features,
) -> Result<R, error::Unspecified> {
    
    
    
    
    
    if peer_public_key.algorithm != my_private_key.algorithm {
        return Err(error::Unspecified);
    }

    let alg = &my_private_key.algorithm;

    
    

    
    
    

    
    

    let mut shared_key = [0u8; ec::ELEM_MAX_BYTES];
    let shared_key = &mut shared_key[..alg.curve.elem_scalar_seed_len];

    
    
    
    
    (alg.ecdh)(
        shared_key,
        &my_private_key.private_key,
        untrusted::Input::from(peer_public_key.bytes),
        cpu,
    )?;

    
    
    
    
    Ok(kdf(shared_key))
}

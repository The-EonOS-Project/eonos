










































































































use crate::{
    bb, cpu,
    digest::{self, Digest, FinishError},
    error, hkdf, rand,
};

pub(crate) use crate::digest::InputTooLongError;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Algorithm(&'static digest::Algorithm);

impl Algorithm {
    
    #[inline]
    pub fn digest_algorithm(&self) -> &'static digest::Algorithm {
        self.0
    }
}


pub static HMAC_SHA1_FOR_LEGACY_USE_ONLY: Algorithm = Algorithm(&digest::SHA1_FOR_LEGACY_USE_ONLY);


pub static HMAC_SHA256: Algorithm = Algorithm(&digest::SHA256);


pub static HMAC_SHA384: Algorithm = Algorithm(&digest::SHA384);


pub static HMAC_SHA512: Algorithm = Algorithm(&digest::SHA512);




#[derive(Clone, Copy, Debug)]
pub struct Tag(Digest);

impl AsRef<[u8]> for Tag {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}


#[derive(Clone)]
pub struct Key {
    inner: digest::BlockContext,
    outer: digest::BlockContext,
}

impl core::fmt::Debug for Key {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        f.debug_struct("Key")
            .field("algorithm", self.algorithm().digest_algorithm())
            .finish()
    }
}

impl Key {
    
    
    
    
    
    
    
    pub fn generate(
        algorithm: Algorithm,
        rng: &dyn rand::SecureRandom,
    ) -> Result<Self, error::Unspecified> {
        Self::construct(algorithm, |buf| rng.fill(buf), cpu::features())
    }

    fn construct<F>(
        algorithm: Algorithm,
        fill: F,
        cpu: cpu::Features,
    ) -> Result<Self, error::Unspecified>
    where
        F: FnOnce(&mut [u8]) -> Result<(), error::Unspecified>,
    {
        let mut key_bytes = [0; digest::MAX_OUTPUT_LEN];
        let key_bytes = &mut key_bytes[..algorithm.0.output_len()];
        fill(key_bytes)?;
        Self::try_new(algorithm, key_bytes, cpu).map_err(error::erase::<InputTooLongError>)
    }

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    pub fn new(algorithm: Algorithm, key_value: &[u8]) -> Self {
        Self::try_new(algorithm, key_value, cpu::features())
            .map_err(error::erase::<InputTooLongError>)
            .unwrap()
    }

    pub(crate) fn try_new(
        algorithm: Algorithm,
        key_value: &[u8],
        cpu_features: cpu::Features,
    ) -> Result<Self, InputTooLongError> {
        let digest_alg = algorithm.0;
        let mut key = Self {
            inner: digest::BlockContext::new(digest_alg),
            outer: digest::BlockContext::new(digest_alg),
        };

        let block_len = digest_alg.block_len();

        let key_hash;
        let key_value = if key_value.len() <= block_len {
            key_value
        } else {
            key_hash = Digest::compute_from(digest_alg, key_value, cpu_features)?;
            key_hash.as_ref()
        };

        const IPAD: u8 = 0x36;

        let mut padded_key = [IPAD; digest::MAX_BLOCK_LEN];
        let padded_key = &mut padded_key[..block_len];

        
        
        
        bb::xor_assign_at_start(&mut padded_key[..], key_value);

        let leftover = key.inner.update(padded_key, cpu_features);
        debug_assert_eq!(leftover.len(), 0);

        const OPAD: u8 = 0x5C;

        
        
        bb::xor_assign(&mut padded_key[..], IPAD ^ OPAD);
        let leftover = key.outer.update(padded_key, cpu_features);
        debug_assert_eq!(leftover.len(), 0);

        Ok(key)
    }

    
    #[inline]
    pub fn algorithm(&self) -> Algorithm {
        Algorithm(self.inner.algorithm)
    }

    pub(crate) fn sign(&self, data: &[u8], cpu: cpu::Features) -> Result<Tag, InputTooLongError> {
        let mut ctx = Context::with_key(self);
        ctx.update(data);
        ctx.try_sign(cpu)
    }

    fn verify(&self, data: &[u8], tag: &[u8], cpu: cpu::Features) -> Result<(), VerifyError> {
        let computed = self
            .sign(data, cpu)
            .map_err(VerifyError::InputTooLongError)?;
        bb::verify_slices_are_equal(computed.as_ref(), tag)
            .map_err(|_: error::Unspecified| VerifyError::Mismatch)
    }
}

impl hkdf::KeyType for Algorithm {
    fn len(&self) -> usize {
        self.digest_algorithm().output_len()
    }
}

impl From<hkdf::Okm<'_, Algorithm>> for Key {
    fn from(okm: hkdf::Okm<Algorithm>) -> Self {
        Self::construct(*okm.len(), |buf| okm.fill(buf), cpu::features()).unwrap()
    }
}




#[derive(Clone)]
pub struct Context {
    inner: digest::Context,
    outer: digest::BlockContext,
}

impl core::fmt::Debug for Context {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        f.debug_struct("Context")
            .field("algorithm", self.inner.algorithm())
            .finish()
    }
}

impl Context {
    
    
    pub fn with_key(signing_key: &Key) -> Self {
        Self {
            inner: digest::Context::clone_from(&signing_key.inner),
            outer: signing_key.outer.clone(),
        }
    }

    
    
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    
    
    
    
    
    
    
    pub fn sign(self) -> Tag {
        self.try_sign(cpu::features())
            .map_err(error::erase::<InputTooLongError>)
            .unwrap()
    }

    pub(crate) fn try_sign(self, cpu_features: cpu::Features) -> Result<Tag, InputTooLongError> {
        
        debug_assert_eq!(self.inner.algorithm(), self.outer.algorithm);
        debug_assert!(self.inner.algorithm().output_len() < self.outer.algorithm.block_len());

        let inner = self.inner.try_finish(cpu_features)?;
        let inner = inner.as_ref();
        let num_pending = inner.len();
        let buffer = &mut [0u8; digest::MAX_BLOCK_LEN];
        const _BUFFER_IS_LARGE_ENOUGH_TO_HOLD_INNER: () =
            assert!(digest::MAX_OUTPUT_LEN < digest::MAX_BLOCK_LEN);
        buffer[..num_pending].copy_from_slice(inner);

        self.outer
            .try_finish(buffer, num_pending, cpu_features)
            .map(Tag)
            .map_err(|err| match err {
                FinishError::InputTooLong(i) => {
                    
                    
                    
                    
                    
                    i
                }
                FinishError::PendingNotAPartialBlock(_) => {
                    
                    unreachable!()
                }
            })
    }
}







pub fn sign(key: &Key, data: &[u8]) -> Tag {
    key.sign(data, cpu::features())
        .map_err(error::erase::<InputTooLongError>)
        .unwrap()
}








pub fn verify(key: &Key, data: &[u8], tag: &[u8]) -> Result<(), error::Unspecified> {
    key.verify(data, tag, cpu::features())
        .map_err(|_: VerifyError| error::Unspecified)
}

enum VerifyError {
    
    
    
    
    
    #[allow(dead_code)]
    InputTooLongError(InputTooLongError),

    Mismatch,
}

#[cfg(test)]
mod tests {
    use crate::{hmac, rand};

    
    
    #[test]
    pub fn hmac_signing_key_coverage() {
        let rng = rand::SystemRandom::new();

        const HELLO_WORLD_GOOD: &[u8] = b"hello, world";
        const HELLO_WORLD_BAD: &[u8] = b"hello, worle";

        for algorithm in &[
            hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
            hmac::HMAC_SHA256,
            hmac::HMAC_SHA384,
            hmac::HMAC_SHA512,
        ] {
            let key = hmac::Key::generate(*algorithm, &rng).unwrap();
            let tag = hmac::sign(&key, HELLO_WORLD_GOOD);
            assert!(hmac::verify(&key, HELLO_WORLD_GOOD, tag.as_ref()).is_ok());
            assert!(hmac::verify(&key, HELLO_WORLD_BAD, tag.as_ref()).is_err())
        }
    }
}
























use super::{Algorithm, LessSafeKey, MAX_KEY_LEN};
use crate::{cpu, error, hkdf};


pub struct UnboundKey {
    inner: LessSafeKey,
}

impl UnboundKey {
    
    
    
    #[inline]
    pub fn new(
        algorithm: &'static Algorithm,
        key_bytes: &[u8],
    ) -> Result<Self, error::Unspecified> {
        Ok(Self {
            inner: LessSafeKey::new_(algorithm, key_bytes, cpu::features())?,
        })
    }

    
    #[inline]
    pub fn algorithm(&self) -> &'static Algorithm {
        self.inner.algorithm()
    }

    #[inline]
    pub(super) fn into_inner(self) -> LessSafeKey {
        self.inner
    }
}

impl core::fmt::Debug for UnboundKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        self.inner.fmt_debug("UnboundKey", f)
    }
}

impl From<hkdf::Okm<'_, &'static Algorithm>> for UnboundKey {
    fn from(okm: hkdf::Okm<&'static Algorithm>) -> Self {
        let mut key_bytes = [0; MAX_KEY_LEN];
        let key_bytes = &mut key_bytes[..okm.len().key_len()];
        let algorithm = *okm.len();
        okm.fill(key_bytes).unwrap();
        Self {
            inner: LessSafeKey::new_(algorithm, key_bytes, cpu::features()).unwrap(),
        }
    }
}

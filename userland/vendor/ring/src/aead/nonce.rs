use crate::error;

pub struct Nonce([u8; NONCE_LEN]);

impl Nonce {

    #[inline]
    pub fn try_assume_unique_for_key(value: &[u8]) -> Result<Self, error::Unspecified> {
        let value: &[u8; NONCE_LEN] = value.try_into()?;
        Ok(Self::assume_unique_for_key(*value))
    }

    #[inline]
    pub fn assume_unique_for_key(value: [u8; NONCE_LEN]) -> Self {
        Self(value)
    }
}

impl AsRef<[u8; NONCE_LEN]> for Nonce {
    fn as_ref(&self) -> &[u8; NONCE_LEN] {
        &self.0
    }
}

pub const NONCE_LEN: usize = 96 / 8;

use crate::{
    cpu, error,
    polyfill::{u64_from_usize, usize_from_u64_saturated},
};

pub use self::{
    algorithm::{Algorithm, AES_128_GCM, AES_256_GCM, CHACHA20_POLY1305},
    less_safe_key::LessSafeKey,
    nonce::{Nonce, NONCE_LEN},
    opening_key::OpeningKey,
    sealing_key::SealingKey,
    unbound_key::UnboundKey,
};

pub trait NonceSequence {

    fn advance(&mut self) -> Result<Nonce, error::Unspecified>;
}

pub trait BoundKey<N: NonceSequence>: core::fmt::Debug {

    fn new(key: UnboundKey, nonce_sequence: N) -> Self;

    fn algorithm(&self) -> &'static Algorithm;
}

#[derive(Clone, Copy)]
pub struct Aad<A>(A);

impl<A: AsRef<[u8]>> Aad<A> {

    #[inline]
    pub fn from(aad: A) -> Self {
        Self(aad)
    }
}

impl<A> AsRef<[u8]> for Aad<A>
where
    A: AsRef<[u8]>,
{
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Aad<[u8; 0]> {

    pub fn empty() -> Self {
        Self::from([])
    }
}

impl<A> core::fmt::Debug for Aad<A>
where
    A: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Aad").field(&self.0).finish()
    }
}

impl<A> PartialEq for Aad<A>
where
    A: PartialEq,
{
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<A> Eq for Aad<A> where A: Eq {}

#[allow(clippy::large_enum_variant, variant_size_differences)]
#[derive(Clone)]
enum KeyInner {
    AesGcm(aes_gcm::Key),
    ChaCha20Poly1305(chacha20_poly1305::Key),
}

const fn max_input_len(block_len: usize, overhead_blocks_per_nonce: usize) -> usize {

    usize_from_u64_saturated(
        ((1u64 << 32) - u64_from_usize(overhead_blocks_per_nonce)) * u64_from_usize(block_len),
    )
}

#[must_use]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Tag([u8; TAG_LEN]);

impl AsRef<[u8]> for Tag {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl TryFrom<&[u8]> for Tag {
    type Error = error::Unspecified;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let raw_tag: [u8; TAG_LEN] = value.try_into().map_err(|_| error::Unspecified)?;
        Ok(Self::from(raw_tag))
    }
}

impl From<[u8; TAG_LEN]> for Tag {
    #[inline]
    fn from(value: [u8; TAG_LEN]) -> Self {
        Self(value)
    }
}

const MAX_KEY_LEN: usize = 32;

const TAG_LEN: usize = 16;

pub const MAX_TAG_LEN: usize = TAG_LEN;

mod aes;
mod aes_gcm;
mod algorithm;
mod chacha;
mod chacha20_poly1305;
pub mod chacha20_poly1305_openssh;
mod gcm;
mod less_safe_key;
mod nonce;
mod opening_key;
mod overlapping;
mod poly1305;
pub mod quic;
mod sealing_key;
mod shift;
mod unbound_key;

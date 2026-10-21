use crate::{cpu, error, rand};

pub use self::keys::{KeyPair, PublicKey, Seed};

pub struct Curve {
    pub public_key_len: usize,
    pub elem_scalar_seed_len: usize,

    pub id: CurveID,

    check_private_key_bytes: fn(bytes: &[u8], cpu: cpu::Features) -> Result<(), error::Unspecified>,

    generate_private_key: fn(
        rng: &dyn rand::SecureRandom,
        &mut [u8],
        cpu: cpu::Features,
    ) -> Result<(), error::Unspecified>,

    public_from_private: fn(
        public_out: &mut [u8],
        private_key: &Seed,
        cpu: cpu::Features,
    ) -> Result<(), error::Unspecified>,
}

derive_debug_via_id!(Curve);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CurveID {
    Curve25519,
    P256,
    P384,
}

const ELEM_MAX_BITS: usize = 384;
pub const ELEM_MAX_BYTES: usize = (ELEM_MAX_BITS + 7) / 8;

pub const SCALAR_MAX_BYTES: usize = ELEM_MAX_BYTES;
const SEED_MAX_BYTES: usize = ELEM_MAX_BYTES;

pub const PKCS8_DOCUMENT_MAX_LEN: usize = 40 + SCALAR_MAX_BYTES + keys::PUBLIC_KEY_MAX_LEN;

pub mod curve25519;
mod keys;
pub mod suite_b;

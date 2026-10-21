




















use crate::{
    arithmetic::bigint,
    bits, error,
    io::{self, der},
};

pub(crate) mod padding;


const PUBLIC_KEY_PUBLIC_MODULUS_MAX_LEN: usize =
    bits::BitLength::from_bits(8192).as_usize_bytes_rounded_up();


const PRIVATE_KEY_PUBLIC_MODULUS_MAX_BITS: bits::BitLength = bits::BitLength::from_bits(4096);


#[derive(Debug)]
pub struct RsaParameters {
    padding_alg: &'static dyn padding::Verification,
    min_bits: bits::BitLength,
}

fn parse_public_key(
    input: untrusted::Input,
) -> Result<(io::Positive, io::Positive), error::Unspecified> {
    input.read_all(error::Unspecified, |input| {
        der::nested(input, der::Tag::Sequence, error::Unspecified, |input| {
            let n = der::positive_integer(input)?;
            let e = der::positive_integer(input)?;
            Ok((n, e))
        })
    })
}



enum N {}

impl bigint::PublicModulus for N {}

mod keypair;
mod keypair_components;
mod public_exponent;
mod public_key;
mod public_key_components;
mod public_modulus;

pub(crate) mod verification;

use self::{public_exponent::PublicExponent, public_modulus::PublicModulus};

pub use self::{
    keypair::KeyPair, keypair_components::KeyPairComponents, public_key::PublicKey,
    public_key_components::PublicKeyComponents,
};

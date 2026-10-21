

















































































































use self::{derive_error::DeriveError, verify_error::VerifyError};
use crate::{
    bb, cpu, digest,
    error::{self, TooMuchOutputRequestedError},
    hmac::{self, InputTooLongError},
};
use core::num::NonZeroU32;


#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Algorithm(hmac::Algorithm);


pub static PBKDF2_HMAC_SHA1: Algorithm = Algorithm(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY);


pub static PBKDF2_HMAC_SHA256: Algorithm = Algorithm(hmac::HMAC_SHA256);


pub static PBKDF2_HMAC_SHA384: Algorithm = Algorithm(hmac::HMAC_SHA384);


pub static PBKDF2_HMAC_SHA512: Algorithm = Algorithm(hmac::HMAC_SHA512);




























pub fn derive(
    algorithm: Algorithm,
    iterations: NonZeroU32,
    salt: &[u8],
    secret: &[u8],
    out: &mut [u8],
) {
    let cpu = cpu::features();
    try_derive(algorithm, iterations, salt, secret, out, cpu)
        .map_err(error::erase::<DeriveError>)
        .unwrap()
}

fn try_derive(
    algorithm: Algorithm,
    iterations: NonZeroU32,
    salt: &[u8],
    secret: &[u8],
    out: &mut [u8],
    cpu: cpu::Features,
) -> Result<(), DeriveError> {
    let digest_alg = algorithm.0.digest_algorithm();
    let output_len = digest_alg.output_len();

    
    
    
    

    let secret =
        hmac::Key::try_new(algorithm.0, secret, cpu).map_err(DeriveError::secret_too_long)?;

    
    out.fill(0);

    let mut idx: u32 = 0;

    let out_len = out.len();
    for chunk in out.chunks_mut(output_len) {
        idx = idx.checked_add(1).ok_or_else(|| {
            DeriveError::too_much_output_requested(TooMuchOutputRequestedError::new(out_len))
        })?;
        
        
        derive_block(&secret, iterations, salt, idx, chunk, cpu)
            .map_err(DeriveError::salt_too_long)?;
    }
    Ok(())
}

fn derive_block(
    secret: &hmac::Key,
    iterations: NonZeroU32,
    salt: &[u8],
    idx: u32,
    out: &mut [u8],
    cpu: cpu::Features,
) -> Result<(), InputTooLongError> {
    let mut ctx = hmac::Context::with_key(secret);
    ctx.update(salt);
    ctx.update(&u32::to_be_bytes(idx));

    let mut u = ctx.try_sign(cpu)?;

    let mut remaining: u32 = iterations.into();
    loop {
        bb::xor_assign_at_start(&mut out[..], u.as_ref());

        if remaining == 1 {
            break;
        }
        remaining -= 1;

        
        
        
        u = secret.sign(u.as_ref(), cpu)?
    }
    Ok(())
}

cold_exhaustive_error! {
    enum derive_error::DeriveError {
        secret_too_long => SecretTooLong(InputTooLongError),
        salt_too_long => SaltTooLong(InputTooLongError),
        too_much_output_requested => TooMuchOutputRequested(TooMuchOutputRequestedError),
    }
}

cold_exhaustive_error! {
    enum verify_error::VerifyError {
        mismatch => Mismatch(()),
        secret_too_long => SecretTooLong(InputTooLongError),
        salt_too_long => SaltTooLong(InputTooLongError),
        previously_derived_empty => PreviouslyDerivedEmpty(usize),
    }
}
















pub fn verify(
    algorithm: Algorithm,
    iterations: NonZeroU32,
    salt: &[u8],
    secret: &[u8],
    previously_derived: &[u8],
) -> Result<(), error::Unspecified> {
    let cpu = cpu::features();
    try_verify(algorithm, iterations, salt, secret, previously_derived, cpu)
        .map_err(error::erase::<VerifyError>)
}

fn try_verify(
    algorithm: Algorithm,
    iterations: NonZeroU32,
    salt: &[u8],
    secret: &[u8],
    previously_derived: &[u8],
    cpu: cpu::Features,
) -> Result<(), VerifyError> {
    let digest_alg = algorithm.0.digest_algorithm();

    if previously_derived.is_empty() {
        return Err(VerifyError::previously_derived_empty(0));
    }

    let mut derived_buf = [0u8; digest::MAX_OUTPUT_LEN];

    let output_len = digest_alg.output_len();
    let secret =
        hmac::Key::try_new(algorithm.0, secret, cpu).map_err(VerifyError::secret_too_long)?;
    let mut idx: u32 = 0;

    let mut matches = 1;

    for previously_derived_chunk in previously_derived.chunks(output_len) {
        idx = idx.checked_add(1).ok_or_else(|| {
            
            
            VerifyError::mismatch(())
        })?;

        let derived_chunk = &mut derived_buf[..previously_derived_chunk.len()];
        derived_chunk.fill(0);

        derive_block(&secret, iterations, salt, idx, derived_chunk, cpu)
            .map_err(VerifyError::salt_too_long)?;

        
        #[allow(clippy::bool_to_int_with_if)]
        let current_block_matches =
            if bb::verify_slices_are_equal(derived_chunk, previously_derived_chunk).is_ok() {
                1
            } else {
                0
            };

        matches &= current_block_matches;
    }

    if matches == 0 {
        return Err(VerifyError::mismatch(()));
    }

    Ok(())
}

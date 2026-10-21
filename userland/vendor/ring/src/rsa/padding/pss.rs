













use super::{super::PUBLIC_KEY_PUBLIC_MODULUS_MAX_LEN, mgf1, Padding, RsaEncoding, Verification};
use crate::{bb, bits, digest, error, rand};







#[allow(clippy::upper_case_acronyms)] 
#[derive(Debug)]
pub struct PSS {
    digest_alg: &'static digest::Algorithm,
}

impl crate::sealed::Sealed for PSS {}

impl Padding for PSS {
    fn digest_alg(&self) -> &'static digest::Algorithm {
        self.digest_alg
    }
}

impl RsaEncoding for PSS {
    
    
    fn encode(
        &self,
        m_hash: digest::Digest,
        m_out: &mut [u8],
        mod_bits: bits::BitLength,
        rng: &dyn rand::SecureRandom,
    ) -> Result<(), error::Unspecified> {
        let metrics = PSSMetrics::new(self.digest_alg, mod_bits)?;

        
        
        
        
        
        
        let em = if metrics.top_byte_mask == 0xff {
            m_out[0] = 0;
            &mut m_out[1..]
        } else {
            m_out
        };
        assert_eq!(em.len(), metrics.em_len);

        

        

        let (db, digest_terminator) = em.split_at_mut(metrics.db_len);

        let separator_pos = db.len() - 1 - metrics.s_len;

        
        let salt: &[u8] = {
            let salt = &mut db[(separator_pos + 1)..];
            rng.fill(salt)?; 
            salt
        };

        
        let h = pss_digest(self.digest_alg, m_hash, salt);

        
        db[..separator_pos].fill(0); 

        
        db[separator_pos] = 0x01;

        
        mgf1(self.digest_alg, h.as_ref(), db);

        
        db[0] &= metrics.top_byte_mask;

        
        digest_terminator[..metrics.h_len].copy_from_slice(h.as_ref());
        digest_terminator[metrics.h_len] = 0xbc;

        Ok(())
    }
}

impl Verification for PSS {
    
    
    fn verify(
        &self,
        m_hash: digest::Digest,
        m: &mut untrusted::Reader,
        mod_bits: bits::BitLength,
    ) -> Result<(), error::Unspecified> {
        let metrics = PSSMetrics::new(self.digest_alg, mod_bits)?;

        
        
        
        
        
        
        
        
        
        if metrics.top_byte_mask == 0xff {
            if m.read_byte()? != 0 {
                return Err(error::Unspecified);
            }
        };
        let em = m;

        
        

        

        

        
        let masked_db = em.read_bytes(metrics.db_len)?;
        let h_hash = em.read_bytes(metrics.h_len)?;

        
        if em.read_byte()? != 0xbc {
            return Err(error::Unspecified);
        }

        
        let mut db = [0u8; PUBLIC_KEY_PUBLIC_MODULUS_MAX_LEN];
        let db = &mut db[..metrics.db_len];

        mgf1(self.digest_alg, h_hash.as_slice_less_safe(), db);

        masked_db.read_all(error::Unspecified, |masked_bytes| {
            
            let b = masked_bytes.read_byte()?;
            if b & !metrics.top_byte_mask != 0 {
                return Err(error::Unspecified);
            }
            db[0] ^= b;

            
            let db_rest = &mut db[1..];
            let masked_bytes = masked_bytes.read_bytes(db_rest.len())?;
            bb::xor_assign_at_start(db_rest, masked_bytes.as_slice_less_safe());
            Ok(())
        })?;

        
        db[0] &= metrics.top_byte_mask;

        
        let ps_len = metrics.ps_len;
        if db[0..ps_len].iter().any(|&db| db != 0) {
            return Err(error::Unspecified);
        }
        if db[metrics.ps_len] != 1 {
            return Err(error::Unspecified);
        }

        
        let salt = &db[(db.len() - metrics.s_len)..];

        
        let h_prime = pss_digest(self.digest_alg, m_hash, salt);

        
        if h_hash.as_slice_less_safe() != h_prime.as_ref() {
            return Err(error::Unspecified);
        }

        Ok(())
    }
}

struct PSSMetrics {
    #[cfg_attr(not(feature = "alloc"), allow(dead_code))]
    em_len: usize,
    db_len: usize,
    ps_len: usize,
    s_len: usize,
    h_len: usize,
    top_byte_mask: u8,
}

impl PSSMetrics {
    fn new(
        digest_alg: &'static digest::Algorithm,
        mod_bits: bits::BitLength,
    ) -> Result<Self, error::Unspecified> {
        let em_bits = mod_bits.try_sub_1()?;
        let em_len = em_bits.as_usize_bytes_rounded_up();
        let leading_zero_bits = (8 * em_len) - em_bits.as_bits();
        debug_assert!(leading_zero_bits < 8);
        let top_byte_mask = 0xffu8 >> leading_zero_bits;

        let h_len = digest_alg.output_len();

        
        let s_len = h_len;

        
        
        
        
        
        
        
        let db_len = em_len.checked_sub(1 + s_len).ok_or(error::Unspecified)?;
        let ps_len = db_len.checked_sub(h_len + 1).ok_or(error::Unspecified)?;

        debug_assert!(em_bits.as_bits() >= (8 * h_len) + (8 * s_len) + 9);

        Ok(Self {
            em_len,
            db_len,
            ps_len,
            s_len,
            h_len,
            top_byte_mask,
        })
    }
}

fn pss_digest(
    digest_alg: &'static digest::Algorithm,
    m_hash: digest::Digest,
    salt: &[u8],
) -> digest::Digest {
    
    const PREFIX_ZEROS: [u8; 8] = [0u8; 8];

    
    let mut ctx = digest::Context::new(digest_alg);
    ctx.update(&PREFIX_ZEROS);
    ctx.update(m_hash.as_ref());
    ctx.update(salt);
    ctx.finish()
}

macro_rules! rsa_pss_padding {
    ( $vis:vis $PADDING_ALGORITHM:ident, $digest_alg:expr, $doc_str:expr ) => {
        #[doc=$doc_str]
        $vis static $PADDING_ALGORITHM: PSS = PSS {
            digest_alg: $digest_alg,
        };
    };
}

rsa_pss_padding!(
    pub RSA_PSS_SHA256,
    &digest::SHA256,
    "RSA PSS padding using SHA-256 for RSA signatures.\n\nSee
                 \"`RSA_PSS_*` Details\" in `ring::signature`'s module-level
                 documentation for more details."
);

rsa_pss_padding!(
    pub RSA_PSS_SHA384,
    &digest::SHA384,
    "RSA PSS padding using SHA-384 for RSA signatures.\n\nSee
                 \"`RSA_PSS_*` Details\" in `ring::signature`'s module-level
                 documentation for more details."
);

rsa_pss_padding!(
    pub RSA_PSS_SHA512,
    &digest::SHA512,
    "RSA PSS padding using SHA-512 for RSA signatures.\n\nSee
                 \"`RSA_PSS_*` Details\" in `ring::signature`'s module-level
                 documentation for more details."
);

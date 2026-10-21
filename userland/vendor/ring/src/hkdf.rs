use crate::{error, hmac};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Algorithm(hmac::Algorithm);

impl Algorithm {

    #[inline]
    pub fn hmac_algorithm(&self) -> hmac::Algorithm {
        self.0
    }
}

pub static HKDF_SHA1_FOR_LEGACY_USE_ONLY: Algorithm =
    Algorithm(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY);

pub static HKDF_SHA256: Algorithm = Algorithm(hmac::HMAC_SHA256);

pub static HKDF_SHA384: Algorithm = Algorithm(hmac::HMAC_SHA384);

pub static HKDF_SHA512: Algorithm = Algorithm(hmac::HMAC_SHA512);

impl KeyType for Algorithm {
    fn len(&self) -> usize {
        self.0.digest_algorithm().output_len()
    }
}

#[derive(Debug)]
pub struct Salt(hmac::Key);

impl Salt {

    pub fn new(algorithm: Algorithm, value: &[u8]) -> Self {
        Self(hmac::Key::new(algorithm.0, value))
    }

    pub fn extract(&self, secret: &[u8]) -> Prk {

        let salt = &self.0;
        let prk = hmac::sign(salt, secret);
        Prk(hmac::Key::new(salt.algorithm(), prk.as_ref()))
    }

    #[inline]
    pub fn algorithm(&self) -> Algorithm {
        Algorithm(self.0.algorithm())
    }
}

impl From<Okm<'_, Algorithm>> for Salt {
    fn from(okm: Okm<'_, Algorithm>) -> Self {
        Self(hmac::Key::from(Okm {
            prk: okm.prk,
            info: okm.info,
            len: okm.len().0,
            len_cached: okm.len_cached,
        }))
    }
}

pub trait KeyType {

    fn len(&self) -> usize;
}

#[derive(Clone, Debug)]
pub struct Prk(hmac::Key);

impl Prk {

    pub fn new_less_safe(algorithm: Algorithm, value: &[u8]) -> Self {
        Self(hmac::Key::new(algorithm.hmac_algorithm(), value))
    }

    #[inline]
    pub fn expand<'a, L: KeyType>(
        &'a self,
        info: &'a [&'a [u8]],
        len: L,
    ) -> Result<Okm<'a, L>, error::Unspecified> {
        let len_cached = len.len();
        if len_cached > 255 * self.0.algorithm().digest_algorithm().output_len() {
            return Err(error::Unspecified);
        }
        Ok(Okm {
            prk: self,
            info,
            len,
            len_cached,
        })
    }
}

impl From<Okm<'_, Algorithm>> for Prk {
    fn from(okm: Okm<Algorithm>) -> Self {
        Self(hmac::Key::from(Okm {
            prk: okm.prk,
            info: okm.info,
            len: okm.len().0,
            len_cached: okm.len_cached,
        }))
    }
}

#[derive(Debug)]
pub struct Okm<'a, L: KeyType> {
    prk: &'a Prk,
    info: &'a [&'a [u8]],
    len: L,
    len_cached: usize,
}

impl<L: KeyType> Okm<'_, L> {

    #[inline]
    pub fn len(&self) -> &L {
        &self.len
    }

    #[inline]
    pub fn fill(self, out: &mut [u8]) -> Result<(), error::Unspecified> {
        fill_okm(self.prk, self.info, out, self.len_cached)
    }
}

fn fill_okm(
    prk: &Prk,
    info: &[&[u8]],
    out: &mut [u8],
    len: usize,
) -> Result<(), error::Unspecified> {
    if out.len() != len {
        return Err(error::Unspecified);
    }

    let digest_alg = prk.0.algorithm().digest_algorithm();
    assert!(digest_alg.block_len() >= digest_alg.output_len());

    let mut ctx = hmac::Context::with_key(&prk.0);

    let mut n = 1u8;
    let mut out = out;
    loop {
        for info in info {
            ctx.update(info);
        }
        ctx.update(&[n]);

        let t = ctx.sign();
        let t = t.as_ref();

        out = if out.len() < digest_alg.output_len() {
            let len = out.len();
            out.copy_from_slice(&t[..len]);
            &mut []
        } else {
            let (this_chunk, rest) = out.split_at_mut(digest_alg.output_len());
            this_chunk.copy_from_slice(t);
            rest
        };

        if out.is_empty() {
            return Ok(());
        }

        ctx = hmac::Context::with_key(&prk.0);
        ctx.update(t);
        n = n.checked_add(1).unwrap();
    }
}

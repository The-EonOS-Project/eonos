use super::PublicKey;
use core::iter::FromIterator;

#[derive(Clone, Copy)]
pub struct PublicKeyComponents<B> {

    pub n: B,

    pub e: B,
}

impl<B> core::fmt::Debug for PublicKeyComponents<B>
where
    B: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        f.debug_struct("PublicKeyComponents")
            .field("n", &self.n)
            .field("e", &self.e)
            .finish()
    }
}

impl<B> From<&PublicKey> for PublicKeyComponents<B>
where
    B: FromIterator<u8>,
{
    fn from(public_key: &PublicKey) -> Self {
        Self {
            n: public_key.inner().n().be_bytes().collect(),
            e: public_key.inner().e().be_bytes().collect(),
        }
    }
}















use super::{Block, Counter, EncryptBlock, EncryptCtr32, Iv, KeyBytes, Overlapping, AES_KEY};
use crate::error;

#[derive(Clone)]
pub struct Key {
    inner: AES_KEY,
}

impl Key {
    pub(in super::super) fn new(bytes: KeyBytes<'_>) -> Result<Self, error::Unspecified> {
        let inner = unsafe { set_encrypt_key!(aes_nohw_set_encrypt_key, bytes) }?;
        Ok(Self { inner })
    }
}

impl EncryptBlock for Key {
    fn encrypt_block(&self, block: Block) -> Block {
        unsafe { encrypt_block!(aes_nohw_encrypt, block, &self.inner) }
    }

    fn encrypt_iv_xor_block(&self, iv: Iv, block: Block) -> Block {
        super::encrypt_iv_xor_block_using_encrypt_block(self, iv, block)
    }
}

impl EncryptCtr32 for Key {
    fn ctr32_encrypt_within(&self, in_out: Overlapping<'_>, ctr: &mut Counter) {
        unsafe { ctr32_encrypt_blocks!(aes_nohw_ctr32_encrypt_blocks, in_out, &self.inner, ctr) }
    }
}

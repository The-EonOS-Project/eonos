













#![cfg(any(
    all(target_arch = "aarch64", target_endian = "little"),
    target_arch = "x86",
    target_arch = "x86_64"
))]

use super::{Block, Counter, EncryptBlock, EncryptCtr32, Iv, KeyBytes, Overlapping, AES_KEY};
use crate::{cpu, error};
use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(all(target_arch = "aarch64", target_endian = "little"))] {
        pub(in super::super) type RequiredCpuFeatures = cpu::arm::Aes;
        pub(in super::super) type OptionalCpuFeatures = ();
    } else if #[cfg(any(target_arch = "x86", target_arch = "x86_64"))] {
        use cpu::intel::{Aes, Avx, Ssse3};
        
        
        
        pub(in super::super) type RequiredCpuFeatures = (Aes, Ssse3);
        pub(in super::super) type OptionalCpuFeatures = Avx;
    }
}

#[derive(Clone)]
pub struct Key {
    inner: AES_KEY,
}

impl Key {
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    pub(in super::super) fn new(
        bytes: KeyBytes<'_>,
        _required_cpu_features: RequiredCpuFeatures,
        _optional_cpu_features: Option<OptionalCpuFeatures>,
    ) -> Result<Self, error::Unspecified> {
        let inner = unsafe { set_encrypt_key!(aes_hw_set_encrypt_key, bytes) }?;
        Ok(Self { inner })
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub(in super::super) fn new(
        bytes: KeyBytes<'_>,
        (Aes { .. }, Ssse3 { .. }): RequiredCpuFeatures,
        optional_cpu_features: Option<OptionalCpuFeatures>,
    ) -> Result<Self, error::Unspecified> {
        
        
        let inner = if let Some(Avx { .. }) = optional_cpu_features {
            unsafe { set_encrypt_key!(aes_hw_set_encrypt_key_alt, bytes) }?
        } else {
            unsafe { set_encrypt_key!(aes_hw_set_encrypt_key_base, bytes) }?
        };
        Ok(Self { inner })
    }

    #[cfg(any(
        all(target_arch = "aarch64", target_endian = "little"),
        target_arch = "x86_64"
    ))]
    #[must_use]
    pub(in super::super) fn inner_less_safe(&self) -> &AES_KEY {
        &self.inner
    }
}

impl EncryptBlock for Key {
    fn encrypt_block(&self, block: Block) -> Block {
        super::encrypt_block_using_encrypt_iv_xor_block(self, block)
    }

    fn encrypt_iv_xor_block(&self, iv: Iv, block: Block) -> Block {
        super::encrypt_iv_xor_block_using_ctr32(self, iv, block)
    }
}

impl EncryptCtr32 for Key {
    fn ctr32_encrypt_within(&self, in_out: Overlapping<'_>, ctr: &mut Counter) {
        unsafe { ctr32_encrypt_blocks!(aes_hw_ctr32_encrypt_blocks, in_out, &self.inner, ctr) }
    }
}

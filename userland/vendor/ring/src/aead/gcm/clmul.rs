













#![cfg(any(
    all(target_arch = "aarch64", target_endian = "little"),
    target_arch = "x86",
    target_arch = "x86_64"
))]

use super::{ffi::KeyValue, HTable, UpdateBlock, Xi};
use crate::aead::gcm::ffi::BLOCK_LEN;
use crate::cpu;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use {super::UpdateBlocks, crate::polyfill::slice::AsChunks};

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
pub(in super::super) type RequiredCpuFeatures = cpu::arm::PMull;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(in super::super) type RequiredCpuFeatures = (cpu::intel::ClMul, cpu::intel::Ssse3);

#[derive(Clone)]
pub struct Key {
    h_table: HTable,
}

impl Key {
    #[cfg_attr(target_arch = "x86_64", inline(never))]
    pub(in super::super) fn new(value: KeyValue, _cpu: RequiredCpuFeatures) -> Self {
        Self {
            h_table: unsafe { htable_new!(gcm_init_clmul, value) },
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub(super) fn inner(&self) -> &HTable {
        &self.h_table
    }
}

impl UpdateBlock for Key {
    #[cfg(target_arch = "aarch64")]
    fn update_block(&self, xi: &mut Xi, a: [u8; BLOCK_LEN]) {
        prefixed_extern! {
            fn gcm_gmult_clmul(xi: &mut Xi, Htable: &HTable);
        }
        xi.bitxor_assign(a);
        unsafe { self.h_table.gmult(gcm_gmult_clmul, xi) };
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn update_block(&self, xi: &mut Xi, a: [u8; BLOCK_LEN]) {
        self.update_blocks(xi, (&a).into())
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl UpdateBlocks for Key {
    fn update_blocks(&self, xi: &mut Xi, input: AsChunks<u8, { BLOCK_LEN }>) {
        unsafe { ghash!(gcm_ghash_clmul, xi, &self.h_table, input) }
    }
}

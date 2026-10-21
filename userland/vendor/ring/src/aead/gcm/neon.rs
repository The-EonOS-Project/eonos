













#![cfg(any(
    all(target_arch = "aarch64", target_endian = "little"),
    all(target_arch = "arm", target_endian = "little")
))]

use super::{HTable, KeyValue, UpdateBlock, UpdateBlocks, Xi, BLOCK_LEN};
use crate::{cpu, polyfill::slice::AsChunks};

pub(in super::super) type RequiredCpuFeatures = cpu::arm::Neon;

#[derive(Clone)]
pub struct Key {
    h_table: HTable,
}

impl Key {
    pub(in super::super) fn new(value: KeyValue, _cpu: RequiredCpuFeatures) -> Self {
        Self {
            h_table: unsafe { htable_new!(gcm_init_neon, value) },
        }
    }
}

impl UpdateBlock for Key {
    fn update_block(&self, xi: &mut Xi, a: [u8; BLOCK_LEN]) {
        prefixed_extern! {
            fn gcm_gmult_neon(xi: &mut Xi, Htable: &HTable);
        }
        xi.bitxor_assign(a);
        unsafe { self.h_table.gmult(gcm_gmult_neon, xi) };
    }
}

impl UpdateBlocks for Key {
    fn update_blocks(&self, xi: &mut Xi, input: AsChunks<u8, BLOCK_LEN>) {
        unsafe { ghash!(gcm_ghash_neon, xi, &self.h_table, input) }
    }
}

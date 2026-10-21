













#![cfg(target_arch = "x86_64")]

use super::{HTable, KeyValue, UpdateBlock, UpdateBlocks, Xi, BLOCK_LEN};
use crate::{cpu::intel, polyfill::slice::AsChunks};

#[derive(Clone)]
pub struct Key {
    h_table: HTable,
}

impl Key {
    #[inline(never)]
    pub(in super::super) fn new(
        value: KeyValue,
        _required_cpu_features: (intel::ClMul, intel::Avx, intel::Movbe),
    ) -> Self {
        Self {
            h_table: unsafe { htable_new!(gcm_init_avx, value) },
        }
    }

    pub(super) fn inner(&self) -> &HTable {
        &self.h_table
    }
}

impl UpdateBlock for Key {
    fn update_block(&self, xi: &mut Xi, a: [u8; BLOCK_LEN]) {
        self.update_blocks(xi, (&a).into())
    }
}

impl UpdateBlocks for Key {
    fn update_blocks(&self, xi: &mut Xi, input: AsChunks<u8, BLOCK_LEN>) {
        unsafe { ghash!(gcm_ghash_avx, xi, self.inner(), input) }
    }
}

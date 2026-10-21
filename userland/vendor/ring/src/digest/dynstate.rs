













use super::{format_output, sha1, sha2, Output};
use crate::{cpu, polyfill::slice};
use core::mem::size_of;




#[derive(Clone)]
pub(super) enum DynState {
    As64(sha2::State64),
    As32(sha2::State32),
}

impl DynState {
    pub const fn new32(initial_state: sha2::State32) -> Self {
        Self::As32(initial_state)
    }

    pub const fn new64(initial_state: sha2::State64) -> Self {
        Self::As64(initial_state)
    }

    pub fn format_output(self) -> Output {
        match self {
            Self::As64(state) => {
                format_output::<_, _, { size_of::<u64>() }>(state, u64::to_be_bytes)
            }
            Self::As32(state) => {
                format_output::<_, _, { size_of::<u32>() }>(state, u32::to_be_bytes)
            }
        }
    }
}

pub(super) fn sha1_block_data_order<'d>(
    state: &mut DynState,
    data: &'d [u8],
    _cpu_features: cpu::Features,
) -> (usize, &'d [u8]) {
    let state = match state {
        DynState::As32(state) => state,
        _ => {
            unreachable!();
        }
    };

    let (full_blocks, leftover) = slice::as_chunks(data);
    sha1::sha1_block_data_order(state, full_blocks);
    (full_blocks.as_flattened().len(), leftover)
}

pub(super) fn sha256_block_data_order<'d>(
    state: &mut DynState,
    data: &'d [u8],
    cpu_features: cpu::Features,
) -> (usize, &'d [u8]) {
    let state = match state {
        DynState::As32(state) => state,
        _ => {
            unreachable!();
        }
    };

    let (full_blocks, leftover) = slice::as_chunks(data);
    sha2::block_data_order_32(state, full_blocks, cpu_features);
    (full_blocks.len() * sha2::SHA256_BLOCK_LEN.into(), leftover)
}

pub(super) fn sha512_block_data_order<'d>(
    state: &mut DynState,
    data: &'d [u8],
    cpu_features: cpu::Features,
) -> (usize, &'d [u8]) {
    let state = match state {
        DynState::As64(state) => state,
        _ => {
            unreachable!();
        }
    };

    let (full_blocks, leftover) = slice::as_chunks(data);
    sha2::block_data_order_64(state, full_blocks, cpu_features);
    (full_blocks.len() * sha2::SHA512_BLOCK_LEN.into(), leftover)
}

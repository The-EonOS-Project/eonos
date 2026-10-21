use super::{BlockLen, CHAINING_WORDS};
use crate::{cpu, polyfill::slice::AsChunks};
use cfg_if::cfg_if;
use core::num::Wrapping;

pub(in super::super) const SHA512_BLOCK_LEN: BlockLen = BlockLen::_1024;

pub type State64 = [Wrapping<u64>; CHAINING_WORDS];

pub(crate) fn block_data_order_64(
    state: &mut State64,
    data: AsChunks<u8, { SHA512_BLOCK_LEN.into() }>,
    cpu: cpu::Features,
) {
    cfg_if! {
        if #[cfg(all(target_arch = "aarch64", target_endian = "little"))] {
            use cpu::{GetFeature as _, arm::Sha512};
            if let Some(cpu) = cpu.get_feature() {
                sha2_64_ffi!(unsafe { Sha512 => sha512_block_data_order_hw }, state, data, cpu)
            } else {
                sha2_64_ffi!(unsafe { () => sha512_block_data_order_nohw }, state, data, ())
            }
        } else if #[cfg(all(target_arch = "arm", target_endian = "little"))] {
            use cpu::{GetFeature as _, arm::Neon};
            if let Some(cpu) = cpu.get_feature() {
                sha2_64_ffi!(unsafe { Neon => sha512_block_data_order_neon }, state, data, cpu)
            } else {
                sha2_64_ffi!(unsafe { () => sha512_block_data_order_nohw }, state, data, ())
            }
        } else if #[cfg(target_arch = "x86_64")] {
            use cpu::{GetFeature as _, intel::{Avx, IntelCpu}};
            if let Some(cpu) = cpu.get_feature() {

                sha2_64_ffi!(unsafe { (Avx, IntelCpu) => sha512_block_data_order_avx },
                    state, data, cpu);
            } else {
                sha2_64_ffi!(unsafe { () => sha512_block_data_order_nohw }, state, data, ())
            }
        } else {
            let _ = cpu; 
            *state = super::fallback::block_data_order(*state, data)
        }
    }
}

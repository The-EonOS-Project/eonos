use super::{Tag, TAG_LEN};
#[cfg(all(target_arch = "arm", target_endian = "little"))]
use crate::cpu::GetFeature as _;
use crate::{cpu, polyfill::slice::AsChunks};

mod ffi_arm_neon;
mod ffi_fallback;

pub(super) struct Key {
    key_and_nonce: [u8; KEY_LEN],
}

pub(super) const BLOCK_LEN: usize = 16;
pub(super) const KEY_LEN: usize = 2 * BLOCK_LEN;

impl Key {
    #[inline]
    pub(super) fn new(key_and_nonce: [u8; KEY_LEN]) -> Self {
        Self { key_and_nonce }
    }
}

pub(super) enum Context {
    #[cfg(all(target_arch = "arm", target_endian = "little"))]
    ArmNeon(ffi_arm_neon::State),
    Fallback(ffi_fallback::State),
}

impl Context {
    #[inline]
    pub(super) fn from_key(key: Key, cpu: cpu::Features) -> Self {
        #[cfg(all(target_arch = "arm", target_endian = "little"))]
        if let Some(cpu) = cpu.get_feature() {
            return ffi_arm_neon::State::new_context(key, cpu);
        }
        let _: cpu::Features = cpu;
        ffi_fallback::State::new_context(key)
    }

    pub fn update_block(&mut self, input: [u8; BLOCK_LEN]) {
        self.update(AsChunks::from_ref(&input))
    }

    pub fn update(&mut self, input: AsChunks<u8, BLOCK_LEN>) {
        self.update_internal(input.as_flattened());
    }

    fn update_internal(&mut self, input: &[u8]) {
        match self {
            #[cfg(all(target_arch = "arm", target_endian = "little"))]
            Self::ArmNeon(state) => state.update_internal(input),
            Self::Fallback(state) => state.update_internal(input),
        }
    }

    pub(super) fn finish(mut self, input: &[u8]) -> Tag {
        self.update_internal(input);
        match self {
            #[cfg(all(target_arch = "arm", target_endian = "little"))]
            Self::ArmNeon(state) => state.finish(),
            Self::Fallback(state) => state.finish(),
        }
    }
}

pub(super) fn sign(key: Key, input: &[u8], cpu_features: cpu::Features) -> Tag {
    let ctx = Context::from_key(key, cpu_features);
    ctx.finish(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil as test;

    #[test]
    pub fn test_poly1305() {
        let cpu_features = cpu::features();
        test::run(
            test_vector_file!("poly1305_test.txt"),
            |section, test_case| {
                assert_eq!(section, "");
                let key = test_case.consume_bytes("Key");
                let key: &[u8; KEY_LEN] = key.as_slice().try_into().unwrap();
                let input = test_case.consume_bytes("Input");
                let expected_mac = test_case.consume_bytes("MAC");
                let key = Key::new(*key);
                let Tag(actual_mac) = sign(key, &input, cpu_features);
                assert_eq!(expected_mac, actual_mac.as_ref());

                Ok(())
            },
        )
    }
}

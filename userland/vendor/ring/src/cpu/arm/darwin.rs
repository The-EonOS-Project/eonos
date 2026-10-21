













use super::{Aes, Neon, PMull, Sha256, Sha512, CAPS_STATIC};
use crate::polyfill::cstr;


















pub const MIN_STATIC_FEATURES: u32 = Neon::mask() | Aes::mask() | Sha256::mask() | PMull::mask();
pub const FORCE_DYNAMIC_DETECTION: u32 = !MIN_STATIC_FEATURES;





#[allow(clippy::assertions_on_constants)]
const _AARCH64_APPLE_TARGETS_EXPECTED_FEATURES: () =
    assert!((CAPS_STATIC & MIN_STATIC_FEATURES) == MIN_STATIC_FEATURES);




#[allow(clippy::assertions_on_constants)]
const _AARCH64_APPLE_DARWIN_TARGETS_EXPECTED_FEATURES: () =
    assert!(CAPS_STATIC == MIN_STATIC_FEATURES);

pub fn detect_features() -> u32 {
    fn detect_feature(name: cstr::Ref) -> bool {
        use crate::polyfill;
        use core::mem;
        use libc::{c_int, c_void};

        let mut value: c_int = 0;
        let mut len = mem::size_of_val(&value);
        let value_ptr = polyfill::ptr::from_mut(&mut value).cast::<c_void>();
        
        let rc = unsafe {
            libc::sysctlbyname(name.as_ptr(), value_ptr, &mut len, core::ptr::null_mut(), 0)
        };
        
        if rc != 0 {
            return false;
        }
        debug_assert_eq!(len, mem::size_of_val(&value));
        if len != mem::size_of_val(&value) {
            return false;
        }
        value != 0
    }

    
    const _ASSERT_NEON_DETECTED: () = assert!((CAPS_STATIC & Neon::mask()) == Neon::mask());

    let mut features = 0;

    
    const SHA512_NAME: cstr::Ref =
        cstr::unwrap_const_from_bytes_with_nul(b"hw.optional.armv8_2_sha512\0");
    if detect_feature(SHA512_NAME) {
        features |= Sha512::mask();
    }

    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu;

    #[test]
    fn sha512_detection() {
        
        const _SHA512_NOT_STATICALLY_DETECTED: () = assert!((CAPS_STATIC & Sha512::mask()) == 0);

        if cfg!(target_os = "macos") {
            use crate::cpu::{arm::Sha512, GetFeature as _};

            
            assert!(cfg!(target_feature = "sha3"));

            
            let cpu = cpu::features();
            assert!(matches!(cpu.get_feature(), Some(Sha512 { .. })));
        }
    }
}

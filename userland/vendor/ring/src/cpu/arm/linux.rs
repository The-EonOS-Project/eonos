













use super::Neon;




















pub const FORCE_DYNAMIC_DETECTION: u32 = !Neon::mask();



#[cfg(target_env = "uclibc")]
pub fn detect_features() -> u32 {
    0
}

#[cfg(all(
    not(target_env = "uclibc"),
    all(target_arch = "aarch64", target_endian = "little")
))]
pub fn detect_features() -> u32 {
    use super::{Aes, PMull, Sha256, Sha512, CAPS_STATIC};
    use libc::{getauxval, AT_HWCAP, HWCAP_AES, HWCAP_PMULL, HWCAP_SHA2, HWCAP_SHA512};

    let mut features = 0;

    
    const _ASSERT_NEON_DETECTED: () = assert!((CAPS_STATIC & Neon::mask()) == Neon::mask());

    let caps = unsafe { getauxval(AT_HWCAP) };

    if caps & HWCAP_AES == HWCAP_AES {
        features |= Aes::mask();
    }
    if caps & HWCAP_PMULL == HWCAP_PMULL {
        features |= PMull::mask();
    }
    if caps & HWCAP_SHA2 == HWCAP_SHA2 {
        features |= Sha256::mask();
    }
    if caps & HWCAP_SHA512 == HWCAP_SHA512 {
        features |= Sha512::mask();
    }

    features
}

#[cfg(all(
    not(target_env = "uclibc"),
    all(target_arch = "arm", target_endian = "little")
))]
pub fn detect_features() -> u32 {
    use super::CAPS_STATIC;

    
    
    
    
    use libc::c_ulong;
    extern "C" {
        pub fn getauxval(type_: c_ulong) -> c_ulong;
    }
    const AT_HWCAP: c_ulong = 16;
    const HWCAP_NEON: c_ulong = 1 << 12;

    let mut features = 0;

    if CAPS_STATIC & Neon::mask() != Neon::mask() {
        let caps = unsafe { getauxval(AT_HWCAP) };

        
        
        if caps & HWCAP_NEON == HWCAP_NEON {
            features |= Neon::mask();
        }
    }

    features
}

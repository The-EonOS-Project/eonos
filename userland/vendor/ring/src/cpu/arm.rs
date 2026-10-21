use super::CAPS_STATIC;

mod abi_assumptions {
    use core::mem::size_of;

    #[cfg(target_arch = "aarch64")]
    const _ASSUMED_POINTER_SIZE: usize = 8;
    #[cfg(target_arch = "arm")]
    const _ASSUMED_POINTER_SIZE: usize = 4;
    const _ASSUMED_USIZE_SIZE: () = assert!(size_of::<usize>() == _ASSUMED_POINTER_SIZE);
    const _ASSUMED_REF_SIZE: () = assert!(size_of::<&'static u8>() == _ASSUMED_POINTER_SIZE);

    const _ASSUMED_ENDIANNESS: () = assert!(cfg!(target_endian = "little"));
}

cfg_if::cfg_if! {
    if #[cfg(all(all(target_arch = "aarch64", target_endian = "little"),
                 any(target_os = "ios", target_os = "macos", target_os = "tvos", target_os = "visionos", target_os = "watchos")))] {
        mod darwin;
        use darwin as detect;
    } else if #[cfg(all(all(target_arch = "aarch64", target_endian = "little"), target_os = "fuchsia"))] {
        mod fuchsia;
        use fuchsia as detect;
    } else if #[cfg(any(target_os = "android", target_os = "linux"))] {
        mod linux;
        use linux as detect;
    } else if #[cfg(all(all(target_arch = "aarch64", target_endian = "little"), target_os = "windows"))] {
        mod windows;
        use windows as detect;
    } else {
        mod detect {
            pub const FORCE_DYNAMIC_DETECTION: u32 = 0;
            pub fn detect_features() -> u32 { 0 }
        }
    }
}

impl_get_feature! {
    features: [

        { ("aarch64", "arm") => Neon },

        { ("aarch64") => PMull },

        { ("aarch64") => Aes },

        { ("aarch64") => Sha256 },

        { ("aarch64") => Sha512 },
    ],
}

pub(super) mod featureflags {
    pub(in super::super) use super::detect::FORCE_DYNAMIC_DETECTION;
    use super::*;
    use crate::{
        cpu,
        polyfill::{once_cell::race, usize_from_u32},
    };
    use core::num::NonZeroUsize;
    #[cfg(all(target_arch = "arm", target_endian = "little"))]
    use core::sync::atomic::{AtomicU32, Ordering};

    pub(in super::super) fn get_or_init() -> cpu::Features {
        fn init() -> NonZeroUsize {
            let detected = detect::detect_features();
            let filtered = (if cfg!(feature = "unstable-testing-arm-no-hw") {
                !Neon::mask()
            } else {
                0
            }) | (if cfg!(feature = "unstable-testing-arm-no-neon") {
                Neon::mask()
            } else {
                0
            });
            let detected = detected & !filtered;
            let merged = CAPS_STATIC | detected;

            #[cfg(all(
                target_arch = "arm",
                target_endian = "little",
                target_has_atomic = "32"
            ))]
            if (merged & Neon::mask()) == Neon::mask() {

                prefixed_extern! {
                    static neon_available: AtomicU32;
                }

                let p = unsafe { &neon_available };
                p.store(1, Ordering::Relaxed);
            }

            let merged = usize_from_u32(merged) | (1 << (Shift::Initialized as u32));
            NonZeroUsize::new(merged).unwrap() 
        }

        let _: NonZeroUsize = FEATURES.get_or_init(init);

        unsafe { cpu::Features::new_after_feature_flags_written_and_synced_unchecked() }
    }

    pub(in super::super) fn get(_cpu_features: cpu::Features) -> u32 {

        let features = FEATURES.get().map(NonZeroUsize::get).unwrap_or(0);

        #[allow(clippy::cast_possible_truncation)]
        let features = features as u32;

        features
    }

    static FEATURES: race::OnceNonZeroUsize = race::OnceNonZeroUsize::new();

    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    #[rustfmt::skip]
    pub(in super::super) const STATIC_DETECTED: u32 = 0
        | (if cfg!(target_feature = "neon") { Neon::mask() } else { 0 })
        | (if cfg!(target_feature = "aes") { Aes::mask() } else { 0 })
        | (if cfg!(target_feature = "aes") { PMull::mask() } else { 0 })
        | (if cfg!(target_feature = "sha2") { Sha256::mask() } else { 0 })
        | (if cfg!(target_feature = "sha3") { Sha512::mask() } else { 0 })
        ;

    #[cfg(all(target_arch = "arm", target_endian = "little"))]
    pub(in super::super) const STATIC_DETECTED: u32 = 0;
}

#[allow(clippy::assertions_on_constants)]
const _AARCH64_HAS_NEON: () = assert!(
    ((CAPS_STATIC & Neon::mask()) == Neon::mask())
        || !cfg!(all(target_arch = "aarch64", target_endian = "little"))
);

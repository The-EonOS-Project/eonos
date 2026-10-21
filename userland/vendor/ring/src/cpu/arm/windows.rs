













use super::{Aes, Neon, PMull, Sha256, CAPS_STATIC};
use windows_sys::Win32::System::Threading::{
    IsProcessorFeaturePresent, PF_ARM_V8_CRYPTO_INSTRUCTIONS_AVAILABLE,
};

pub const FORCE_DYNAMIC_DETECTION: u32 = 0;

pub fn detect_features() -> u32 {
    
    const _ASSERT_NEON_DETECTED: () = assert!((CAPS_STATIC & Neon::mask()) == Neon::mask());

    let mut features = 0;

    let result = unsafe { IsProcessorFeaturePresent(PF_ARM_V8_CRYPTO_INSTRUCTIONS_AVAILABLE) };

    if result != 0 {
        
        features |= Aes::mask();
        features |= PMull::mask();
        features |= Sha256::mask();
    }

    features
}















use cfg_if::cfg_if;

mod abi_assumptions {
    use core::mem::size_of;

    
    
    
    
    
    const _ASSUMES_SSE2: () =
        assert!(cfg!(target_feature = "sse") && cfg!(target_feature = "sse2"));

    #[cfg(target_arch = "x86_64")]
    const _ASSUMED_POINTER_SIZE: usize = 8;
    #[cfg(target_arch = "x86")]
    const _ASSUMED_POINTER_SIZE: usize = 4;
    const _ASSUMED_USIZE_SIZE: () = assert!(size_of::<usize>() == _ASSUMED_POINTER_SIZE);
    const _ASSUMED_REF_SIZE: () = assert!(size_of::<&'static u8>() == _ASSUMED_POINTER_SIZE);

    const _ASSUMED_ENDIANNESS: () = assert!(cfg!(target_endian = "little"));
}

pub(super) mod featureflags {
    use super::super::CAPS_STATIC;
    use crate::{
        cpu,
        polyfill::{once_cell::race, usize_from_u32},
    };
    use core::num::NonZeroUsize;

    pub(in super::super) fn get_or_init() -> cpu::Features {
        
        
        prefixed_extern! {
            fn OPENSSL_cpuid_setup(out: &mut [u32; 4]);
        }

        let _: NonZeroUsize = FEATURES.get_or_init(|| {
            let mut cpuid = [0; 4];
            
            unsafe {
                OPENSSL_cpuid_setup(&mut cpuid);
            }
            let detected = super::cpuid_to_caps_and_set_c_flags(&cpuid);
            let merged = CAPS_STATIC | detected;

            let merged = usize_from_u32(merged) | (1 << (super::Shift::Initialized as u32));
            NonZeroUsize::new(merged).unwrap() 
        });

        
        
        unsafe { cpu::Features::new_after_feature_flags_written_and_synced_unchecked() }
    }

    pub(in super::super) fn get(_cpu_features: cpu::Features) -> u32 {
        
        
        
        
        
        
        let features = FEATURES.get().map(NonZeroUsize::get).unwrap_or(0);

        
        #[allow(clippy::cast_possible_truncation)]
        let features = features as u32;

        features
    }

    static FEATURES: race::OnceNonZeroUsize = race::OnceNonZeroUsize::new();

    #[cfg(target_arch = "x86")]
    #[rustfmt::skip]
    pub const STATIC_DETECTED: u32 = 0
        | (if cfg!(target_feature = "sse2") { super::Sse2::mask() } else { 0 })
        ;

    
    
    
    #[cfg(target_arch = "x86_64")]
    #[rustfmt::skip]
    pub const STATIC_DETECTED: u32 = 0
        | if cfg!(target_feature = "sse4.1") { super::Sse41::mask() } else { 0 }
        | if cfg!(target_feature = "ssse3") { super::Ssse3::mask() } else { 0 }
        ;

    pub const FORCE_DYNAMIC_DETECTION: u32 = 0;
}

fn cpuid_to_caps_and_set_c_flags(cpuid: &[u32; 4]) -> u32 {
    
    
    
    

    
    #[cfg(target_arch = "x86_64")]
    use core::{mem::align_of, sync::atomic::AtomicU32};
    #[cfg(target_arch = "x86_64")]
    const _ATOMIC32_ALIGNMENT_EQUALS_U32_ALIGNMENT: () =
        assert!(align_of::<AtomicU32>() == align_of::<u32>());

    fn check(leaf: u32, bit: u32) -> bool {
        let shifted = 1 << bit;
        (leaf & shifted) == shifted
    }
    fn set(out: &mut u32, shift: Shift) {
        let shifted = 1 << (shift as u32);
        debug_assert_eq!(*out & shifted, 0);
        *out |= shifted;
        debug_assert_eq!(*out & shifted, shifted);
    }

    #[cfg(target_arch = "x86_64")]
    let is_intel = check(cpuid[0], 30); 

    
    let leaf1_ecx = cpuid[1];

    
    #[cfg(target_arch = "x86_64")]
    let (extended_features_ebx, extended_features_ecx) = (cpuid[2], cpuid[3]);

    let mut caps = 0;

    
    
    

    
    
    
    
    
    
    
    
    
    
    
    const _SSE_REQUIRED: () = assert!(cfg!(target_feature = "sse"));
    const _SSE2_REQUIRED: () = assert!(cfg!(target_feature = "sse2"));

    #[cfg(all(target_arch = "x86", not(target_feature = "sse2")))]
    {
        
        
        
        
        
        
        
        
        
        
        
        let leaf1_edx = cpuid[0];
        let sse1_available = check(leaf1_edx, 25);
        let sse2_available = check(leaf1_edx, 26);
        if sse1_available && sse2_available {
            set(&mut caps, Shift::Sse2);
        }
    }

    
    
    
    
    #[cfg(target_arch = "x86_64")]
    const _SSE2_REQUIRED_X86_64: () = assert!(cfg!(target_feature = "sse2"));
    #[cfg(target_arch = "x86_64")]
    const _SSE_REQUIRED_X86_64: () = assert!(cfg!(target_feature = "sse2"));

    
    
    
    if check(leaf1_ecx, 9) {
        set(&mut caps, Shift::Ssse3);
    }

    
    
    
    
    
    
    
    #[cfg(target_arch = "x86_64")]
    if check(leaf1_ecx, 19) {
        set(&mut caps, Shift::Sse41);
    }

    

    
    
    
    let avx_available = check(leaf1_ecx, 28);
    if avx_available {
        set(&mut caps, Shift::Avx);
    }

    #[cfg(target_arch = "x86_64")]
    if avx_available {
        
        
        
        
        
        let vaes_available = check(extended_features_ecx, 9);
        let vclmul_available = check(extended_features_ecx, 10);
        if vaes_available && vclmul_available {
            set(&mut caps, Shift::VAesClmul);
        }
    }

    
    
    
    
    #[cfg(target_arch = "x86_64")]
    if check(extended_features_ebx, 5) {
        set(&mut caps, Shift::Avx2);

        
        prefixed_extern! {
            static avx2_available: AtomicU32;
        }
        
        
        
        
        let flag = unsafe { &avx2_available };
        flag.store(1, core::sync::atomic::Ordering::Relaxed);
    }

    
    
    
    
    
    
    
    
    
    
    if check(leaf1_ecx, 1) {
        set(&mut caps, Shift::ClMul);
    }
    if check(leaf1_ecx, 25) {
        set(&mut caps, Shift::Aes);
    }
    
    
    #[cfg(target_arch = "x86_64")]
    if check(extended_features_ebx, 29) {
        set(&mut caps, Shift::Sha);
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_intel {
            set(&mut caps, Shift::IntelCpu);
        }

        if check(leaf1_ecx, 22) {
            set(&mut caps, Shift::Movbe);
        }

        let adx_available = check(extended_features_ebx, 19);
        if adx_available {
            set(&mut caps, Shift::Adx);
        }

        
        
        
        
        
        
        
        
        
        
        
        
        let believe_bmi_bits = !is_intel || (adx_available || avx_available);

        if check(extended_features_ebx, 3) && believe_bmi_bits {
            set(&mut caps, Shift::Bmi1);
        }

        let bmi2_available = check(extended_features_ebx, 8) && believe_bmi_bits;
        if bmi2_available {
            set(&mut caps, Shift::Bmi2);
        }

        if adx_available && bmi2_available {
            
            prefixed_extern! {
                static adx_bmi2_available: AtomicU32;
            }
            
            
            
            
            let flag = unsafe { &adx_bmi2_available };
            flag.store(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    caps
}

impl_get_feature! {
    features: [
        { ("x86_64") => VAesClmul },
        { ("x86", "x86_64") => ClMul },
        { ("x86", "x86_64") => Ssse3 },
        { ("x86_64") => Sse41 },
        { ("x86_64") => Movbe },
        { ("x86", "x86_64") => Aes },
        { ("x86", "x86_64") => Avx },
        { ("x86_64") => Bmi1 },
        { ("x86_64") => Avx2 },
        { ("x86_64") => Bmi2 },
        { ("x86_64") => Adx },
        
        
        { ("x86_64") => Sha },
        
        { ("x86") => Sse2 },
    ],
}

cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[derive(Clone, Copy)]
        pub(crate) struct IntelCpu(super::Features);

        impl super::GetFeature<IntelCpu> for super::features::Values {
            fn get_feature(&self) -> Option<IntelCpu> {
                const MASK: u32 = 1 << (Shift::IntelCpu as u32);
                if (self.values() & MASK) == MASK {
                    Some(IntelCpu(self.cpu()))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    
    #[cfg(target_arch = "x86")]
    #[test]
    fn x86_has_sse2() {
        use super::*;
        use crate::cpu::{self, GetFeature as _};
        assert!(matches!(cpu::features().get_feature(), Some(Sse2 { .. })))
    }
}

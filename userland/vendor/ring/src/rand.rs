


















use crate::error;


pub trait SecureRandom: sealed::SecureRandom {
    
    fn fill(&self, dest: &mut [u8]) -> Result<(), error::Unspecified>;
}

impl<T> SecureRandom for T
where
    T: sealed::SecureRandom,
{
    #[inline(always)]
    fn fill(&self, dest: &mut [u8]) -> Result<(), error::Unspecified> {
        self.fill_impl(dest)
    }
}





pub struct Random<T: RandomlyConstructable>(T);

impl<T: RandomlyConstructable> Random<T> {
    
    #[inline]
    pub fn expose(self) -> T {
        self.0
    }
}


#[inline]
pub fn generate<T: RandomlyConstructable>(
    rng: &dyn SecureRandom,
) -> Result<Random<T>, error::Unspecified> {
    let mut r = T::zero();
    rng.fill(r.as_mut_bytes())?;
    Ok(Random(r))
}

pub(crate) mod sealed {
    use crate::error;

    pub trait SecureRandom: core::fmt::Debug {
        
        fn fill_impl(&self, dest: &mut [u8]) -> Result<(), error::Unspecified>;
    }

    pub trait RandomlyConstructable: Sized {
        fn zero() -> Self; 
        fn as_mut_bytes(&mut self) -> &mut [u8]; 
    }

    impl<const N: usize> RandomlyConstructable for [u8; N] {
        #[inline]
        fn zero() -> Self {
            [0; N]
        }

        #[inline]
        fn as_mut_bytes(&mut self) -> &mut [u8] {
            &mut self[..]
        }
    }
}


pub trait RandomlyConstructable: sealed::RandomlyConstructable {}
impl<T> RandomlyConstructable for T where T: sealed::RandomlyConstructable {}

















#[derive(Clone, Debug)]
pub struct SystemRandom(());

impl SystemRandom {
    
    #[inline(always)]
    pub fn new() -> Self {
        Self(())
    }
}

impl crate::sealed::Sealed for SystemRandom {}




#[cfg(any(
    all(feature = "less-safe-getrandom-custom-or-rdrand", target_os = "none"),
    all(feature = "less-safe-getrandom-espidf", target_os = "espidf"),
    target_os = "aix",
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "fuchsia",
    target_os = "haiku",
    target_os = "hermit",
    target_os = "hurd",
    target_os = "horizon",
    target_os = "illumos",
    target_os = "linux",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "redox",
    target_os = "solaris",
    target_os = "vita",
    target_os = "windows",
    all(
        target_vendor = "apple",
        any(
            target_os = "ios",
            target_os = "macos",
            target_os = "tvos",
            target_os = "visionos",
            target_os = "watchos",
        )
    ),
    all(
        target_arch = "wasm32",
        any(
            target_os = "wasi",
            all(target_os = "unknown", feature = "wasm32_unknown_unknown_js")
        )
    ),
))]
impl sealed::SecureRandom for SystemRandom {
    #[inline(always)]
    fn fill_impl(&self, dest: &mut [u8]) -> Result<(), error::Unspecified> {
        getrandom::getrandom(dest).map_err(|_| error::Unspecified)
    }
}

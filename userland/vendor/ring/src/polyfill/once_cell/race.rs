use core::sync::atomic;

use atomic::{AtomicUsize, Ordering};
use core::num::NonZeroUsize;

pub struct OnceNonZeroUsize {
    inner: AtomicUsize,
}

impl OnceNonZeroUsize {

    #[inline]
    pub const fn new() -> OnceNonZeroUsize {
        OnceNonZeroUsize {
            inner: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn get(&self) -> Option<NonZeroUsize> {
        let val = self.inner.load(Ordering::Acquire);
        NonZeroUsize::new(val)
    }

    pub fn get_or_init<F>(&self, f: F) -> NonZeroUsize
    where
        F: FnOnce() -> NonZeroUsize,
    {
        let val = self.inner.load(Ordering::Acquire);
        match NonZeroUsize::new(val) {
            Some(it) => it,
            None => self.init(f),
        }
    }

    #[cold]
    #[inline(never)]
    fn init(&self, f: impl FnOnce() -> NonZeroUsize) -> NonZeroUsize {
        let mut val = f().get();
        let exchange = self
            .inner
            .compare_exchange(0, val, Ordering::AcqRel, Ordering::Acquire);
        if let Err(old) = exchange {
            val = old;
        }
        unsafe { NonZeroUsize::new_unchecked(val) }
    }
}

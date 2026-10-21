













#![allow(dead_code)]

use core::arch::asm;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

use spin::{Mutex, MutexGuard};



#[inline]
pub fn irq_save() -> bool {
    let flags: u64;
    unsafe {
        
        
        asm!("pushfq", "pop {0}", "cli", out(reg) flags);
    }
    flags & (1 << 9) != 0
}


#[inline]
pub fn irq_restore(was_enabled: bool) {
    if was_enabled {
        unsafe { asm!("sti") };
    }
}

pub struct IrqMutex<T> {
    inner: Mutex<T>,
}

pub struct IrqMutexGuard<'a, T> {
    guard: ManuallyDrop<MutexGuard<'a, T>>,
    irq_was_enabled: bool,
}

impl<T> IrqMutex<T> {
    pub const fn new(value: T) -> Self {
        IrqMutex {
            inner: Mutex::new(value),
        }
    }

    pub fn lock(&self) -> IrqMutexGuard<'_, T> {
        let irq_was_enabled = irq_save();
        IrqMutexGuard {
            guard: ManuallyDrop::new(self.inner.lock()),
            irq_was_enabled,
        }
    }

    
    
    pub fn try_lock(&self) -> Option<IrqMutexGuard<'_, T>> {
        let irq_was_enabled = irq_save();
        match self.inner.try_lock() {
            Some(g) => Some(IrqMutexGuard {
                guard: ManuallyDrop::new(g),
                irq_was_enabled,
            }),
            None => {
                irq_restore(irq_was_enabled);
                None
            }
        }
    }

    
    
    
    
    pub unsafe fn force_unlock(&self) {
        self.inner.force_unlock();
    }
}

impl<T> Deref for IrqMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> DerefMut for IrqMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T> Drop for IrqMutexGuard<'_, T> {
    fn drop(&mut self) {
        
        unsafe { ManuallyDrop::drop(&mut self.guard) };
        irq_restore(self.irq_was_enabled);
    }
}

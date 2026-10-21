














#[allow(dead_code)]
#[inline(always)]
pub fn from_mut<T: ?Sized>(r: &mut T) -> *mut T {
    r
}


#[allow(dead_code)]
#[inline(always)]
pub const fn from_ref<T: ?Sized>(r: &T) -> *const T {
    r
}

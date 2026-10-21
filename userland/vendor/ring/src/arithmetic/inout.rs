pub(crate) use crate::error::LenMismatchError;
use core::num::NonZeroUsize;

pub(crate) trait AliasingSlices2<T> {

    #[inline(always)]
    fn with_non_dangling_non_null_pointers_ra<R>(
        self,
        expected_len: NonZeroUsize,
        f: impl FnOnce(*mut T, *const T) -> R,
    ) -> Result<R, LenMismatchError>
    where
        Self: Sized,
    {
        self.with_potentially_dangling_non_null_pointers_ra(expected_len.get(), f)
    }

    fn with_potentially_dangling_non_null_pointers_ra<R>(
        self,
        expected_len: usize,
        f: impl FnOnce(*mut T, *const T) -> R,
    ) -> Result<R, LenMismatchError>;
}

impl<T> AliasingSlices2<T> for &mut [T] {
    fn with_potentially_dangling_non_null_pointers_ra<R>(
        self,
        expected_len: usize,
        f: impl FnOnce(*mut T, *const T) -> R,
    ) -> Result<R, LenMismatchError> {
        let r = self;
        if r.len() != expected_len {
            return Err(LenMismatchError::new(r.len()));
        }
        Ok(f(r.as_mut_ptr(), r.as_ptr()))
    }
}

impl<T> AliasingSlices2<T> for (&mut [T], &[T]) {
    fn with_potentially_dangling_non_null_pointers_ra<R>(
        self,
        expected_len: usize,
        f: impl FnOnce(*mut T, *const T) -> R,
    ) -> Result<R, LenMismatchError> {
        let (r, a) = self;
        if r.len() != expected_len {
            return Err(LenMismatchError::new(r.len()));
        }
        if a.len() != expected_len {
            return Err(LenMismatchError::new(a.len()));
        }
        Ok(f(r.as_mut_ptr(), a.as_ptr()))
    }
}

pub(crate) trait AliasingSlices3<T> {

    #[inline(always)]
    fn with_non_dangling_non_null_pointers_rab<R>(
        self,
        expected_len: NonZeroUsize,
        f: impl FnOnce(*mut T, *const T, *const T) -> R,
    ) -> Result<R, LenMismatchError>
    where
        Self: Sized,
    {
        self.with_potentially_dangling_non_null_pointers_rab(expected_len.get(), f)
    }

    fn with_potentially_dangling_non_null_pointers_rab<R>(
        self,
        expected_len: usize,
        f: impl FnOnce(*mut T, *const T, *const T) -> R,
    ) -> Result<R, LenMismatchError>;
}

impl<T> AliasingSlices3<T> for &mut [T] {
    fn with_potentially_dangling_non_null_pointers_rab<R>(
        self,
        expected_len: usize,
        f: impl FnOnce(*mut T, *const T, *const T) -> R,
    ) -> Result<R, LenMismatchError> {
        <Self as AliasingSlices2<T>>::with_potentially_dangling_non_null_pointers_ra(
            self,
            expected_len,
            |r, a| f(r, r, a),
        )
    }
}

impl<T> AliasingSlices3<T> for (&mut [T], &[T], &[T]) {
    fn with_potentially_dangling_non_null_pointers_rab<R>(
        self,
        expected_len: usize,
        f: impl FnOnce(*mut T, *const T, *const T) -> R,
    ) -> Result<R, LenMismatchError> {
        let (r, a, b) = self;
        ((r, a), b).with_potentially_dangling_non_null_pointers_rab(expected_len, f)
    }
}

impl<RA, T> AliasingSlices3<T> for (RA, &[T])
where
    RA: AliasingSlices2<T>,
{
    fn with_potentially_dangling_non_null_pointers_rab<R>(
        self,
        expected_len: usize,
        f: impl FnOnce(*mut T, *const T, *const T) -> R,
    ) -> Result<R, LenMismatchError> {
        let (ra, b) = self;
        if b.len() != expected_len {
            return Err(LenMismatchError::new(b.len()));
        }
        ra.with_potentially_dangling_non_null_pointers_ra(expected_len, |r, a| f(r, a, b.as_ptr()))
    }
}

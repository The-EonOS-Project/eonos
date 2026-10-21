mod as_chunks;
mod as_chunks_mut;

pub use as_chunks::{as_chunks, AsChunks};
pub use as_chunks_mut::{as_chunks_mut, AsChunksMut};

#[inline(always)]
pub fn split_at_checked<T>(slice: &[T], i: usize) -> Option<(&[T], &[T])> {
    if slice.len() >= i {
        Some(slice.split_at(i))
    } else {
        None
    }
}

#[inline(always)]
pub fn split_first_chunk_mut<T, const N: usize>(
    slice: &mut [T],
) -> Option<(&mut [T; N], &mut [T])> {
    if slice.len() >= N {
        let (head, tail) = slice.split_at_mut(N);
        head.try_into().ok().map(|head| (head, tail))
    } else {
        None
    }
}

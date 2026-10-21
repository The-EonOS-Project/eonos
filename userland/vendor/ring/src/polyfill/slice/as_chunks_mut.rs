













use super::AsChunks;

#[inline(always)]
pub fn as_chunks_mut<T, const N: usize>(slice: &mut [T]) -> (AsChunksMut<T, N>, &mut [T]) {
    assert!(N != 0, "chunk size must be non-zero");
    let len = slice.len() / N;
    let (multiple_of_n, remainder) = slice.split_at_mut(len * N);
    (AsChunksMut(multiple_of_n), remainder)
}

pub struct AsChunksMut<'a, T, const N: usize>(&'a mut [T]);

impl<T, const N: usize> AsChunksMut<'_, T, N> {
    #[inline(always)]
    pub fn as_flattened(&self) -> &[T] {
        self.0
    }

    #[inline(always)]
    pub fn as_flattened_mut(&mut self) -> &mut [T] {
        self.0
    }

    #[cfg(target_arch = "aarch64")]
    pub fn as_ptr(&self) -> *const [T; N] {
        self.0.as_ptr().cast()
    }

    #[cfg(target_arch = "x86_64")]
    pub fn as_ptr(&self) -> *const [T; N] {
        self.0.as_ptr().cast()
    }

    #[cfg(target_arch = "aarch64")]
    pub fn as_mut_ptr(&mut self) -> *mut [T; N] {
        self.0.as_mut_ptr().cast()
    }

    #[cfg(target_arch = "x86_64")]
    #[inline(always)]
    pub fn as_mut(&mut self) -> AsChunksMut<T, N> {
        AsChunksMut(self.0)
    }

    #[inline(always)]
    pub fn as_ref(&self) -> AsChunks<T, N> {
        AsChunks::<T, N>::from(self)
    }

    
    
    #[inline(always)]
    pub fn chunks_mut<const CHUNK_LEN: usize>(&mut self) -> AsChunksMutChunksMutIter<T, N> {
        AsChunksMutChunksMutIter(self.0.chunks_mut(CHUNK_LEN * N))
    }

    #[cfg(target_arch = "x86_64")]
    #[inline(always)]
    pub fn split_at_mut(&mut self, mid: usize) -> (AsChunksMut<T, N>, AsChunksMut<T, N>) {
        let (before, after) = self.0.split_at_mut(mid * N);
        (AsChunksMut(before), AsChunksMut(after))
    }
}

pub struct AsChunksMutChunksMutIter<'a, T, const N: usize>(core::slice::ChunksMut<'a, T>);

impl<'a, T, const N: usize> Iterator for AsChunksMutChunksMutIter<'a, T, N> {
    type Item = AsChunksMut<'a, T, N>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(AsChunksMut)
    }
}

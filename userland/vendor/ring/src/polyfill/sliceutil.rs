pub fn overwrite_at_start<T: Copy>(a: &mut [T], b: &[T]) {
    a.iter_mut().zip(b).for_each(|(a, b)| {
        *a = *b;
    });
}

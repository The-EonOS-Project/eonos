pub struct InputTooLongError<T = usize> {

    #[allow(dead_code)]
    imprecise_input_length: T,
}

impl<T> InputTooLongError<T> {
    #[cold]
    #[inline(never)]
    pub(crate) fn new(imprecise_input_length: T) -> Self {
        Self {
            imprecise_input_length,
        }
    }
}

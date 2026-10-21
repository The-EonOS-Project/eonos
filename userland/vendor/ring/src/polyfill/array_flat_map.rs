use core::iter::FlatMap;

#[derive(Clone)]
pub struct ArrayFlatMap<I, Item, F, const LEN: usize> {
    inner: FlatMap<I, [Item; LEN], F>,
    remaining: usize,
}

impl<I, Item, F, const LEN: usize> ArrayFlatMap<I, Item, F, LEN>
where
    I: ExactSizeIterator,
    F: FnMut(I::Item) -> [Item; LEN],
{

    pub fn new(inner: I, f: F) -> Option<Self> {
        let remaining = inner.len().checked_mul(LEN)?;
        let inner = inner.flat_map(f);
        Some(Self { inner, remaining })
    }
}

impl<I, Item, F, const LEN: usize> Iterator for ArrayFlatMap<I, Item, F, LEN>
where
    I: Iterator,
    F: FnMut(I::Item) -> [Item; LEN],
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.inner.next();
        if result.is_some() {
            self.remaining -= 1;
        }
        result
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<I, Item, F, const LEN: usize> ExactSizeIterator for ArrayFlatMap<I, Item, F, LEN>
where
    I: Iterator,
    F: FnMut(I::Item) -> [Item; LEN],
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn test_array_flat_map() {
        static TEST_CASES: &[(&[u16], fn(u16) -> [u8; 2], &[u8])] = &[

            (&[], u16::to_be_bytes, &[]),

            (
                &[0x0102, 0x0304, 0x0506],
                u16::to_be_bytes,
                &[1, 2, 3, 4, 5, 6],
            ),

            (
                &[0x0102, 0x0304, 0x0506],
                u16::to_le_bytes,
                &[2, 1, 4, 3, 6, 5],
            ),
        ];
        TEST_CASES.iter().copied().for_each(|(input, f, expected)| {
            let mapped = ArrayFlatMap::new(input.iter().copied(), f).unwrap();
            super::super::test::assert_iterator(mapped, expected);
        });
    }

    #[test]
    fn test_array_flat_map_len_overflow() {
        struct DownwardCounter {
            remaining: usize,
        }
        impl Iterator for DownwardCounter {
            type Item = usize;

            fn next(&mut self) -> Option<Self::Item> {
                if self.remaining > 0 {
                    let result = self.remaining;
                    self.remaining -= 1;
                    Some(result)
                } else {
                    None
                }
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                (self.remaining, Some(self.remaining))
            }
        }
        impl ExactSizeIterator for DownwardCounter {}

        const MAX: usize = usize::MAX / size_of::<usize>();

        static TEST_CASES: &[(usize, bool)] = &[(MAX, true), (MAX + 1, false)];
        TEST_CASES.iter().copied().for_each(|(input_len, is_some)| {
            let inner = DownwardCounter {
                remaining: input_len,
            };
            let mapped = ArrayFlatMap::new(inner, usize::to_be_bytes);
            assert_eq!(mapped.is_some(), is_some);
            if let Some(mapped) = mapped {
                assert_eq!(mapped.len(), input_len * size_of::<usize>());
            }
        });
    }
}

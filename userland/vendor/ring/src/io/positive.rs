use crate::error;

#[derive(Copy, Clone)]
pub struct Positive<'a>(untrusted::Input<'a>);

impl<'a> Positive<'a> {
    #[inline]
    pub(crate) fn from_be_bytes(input: untrusted::Input<'a>) -> Result<Self, error::Unspecified> {

        let &first_byte = input
            .as_slice_less_safe()
            .first()
            .ok_or(error::Unspecified)?;

        if first_byte == 0 {
            return Err(error::Unspecified);
        }
        Ok(Self(input))
    }

    #[inline]
    pub fn big_endian_without_leading_zero(&self) -> &'a [u8] {
        self.big_endian_without_leading_zero_as_input()
            .as_slice_less_safe()
    }

    #[inline]
    pub(crate) fn big_endian_without_leading_zero_as_input(&self) -> untrusted::Input<'a> {
        self.0
    }
}

impl Positive<'_> {

    pub fn first_byte(&self) -> u8 {

        self.0.as_slice_less_safe()[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_be_bytes() {
        static TEST_CASES: &[(&[u8], Result<&[u8], error::Unspecified>)] = &[

            (&[], Err(error::Unspecified)),

            (&[0x00], Err(error::Unspecified)),

            (&[0x00, 0x01], Err(error::Unspecified)),
            (&[0x01], Ok(&[0x01])),

            (&[0xff], Ok(&[0xff])),
            (&[0x00, 0xff], Err(error::Unspecified)),

            (&[0x01, 0x00], Ok(&[0x01, 0x00])),
            (&[0x01, 0x00, 0x00], Ok(&[0x01, 0x00, 0x00])),

            (&[0x01, 0x01], Ok(&[0x01, 0x01])),

            (&[0x01, 0x00, 0x01], Ok(&[0x01, 0x00, 0x01])),
            (&[0x01, 0x01, 0x01], Ok(&[0x01, 0x01, 0x01])),
        ];
        for &(input, result) in TEST_CASES {
            let input = untrusted::Input::from(input);
            assert_eq!(
                Positive::from_be_bytes(input).map(|p| p.big_endian_without_leading_zero()),
                result
            );
        }
    }
}

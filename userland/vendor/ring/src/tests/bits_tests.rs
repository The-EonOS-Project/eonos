use crate::{
    bits::{BitLength, FromByteLen as _},
    polyfill::u64_from_usize,
};

#[test]
fn test_from_byte_len_overflow() {
    const USIZE_MAX_VALID_BYTES: usize = usize::MAX / 8;

    match BitLength::<usize>::from_byte_len(USIZE_MAX_VALID_BYTES) {
        Ok(bits) => {
            assert_eq!(bits.as_usize_bytes_rounded_up(), USIZE_MAX_VALID_BYTES);
            assert_eq!(bits.as_bits(), usize::MAX & !0b111);
        }
        Err(_) => unreachable!(),
    }

    assert!(BitLength::<usize>::from_byte_len(USIZE_MAX_VALID_BYTES + 1).is_err());

    {
        let r = BitLength::<u64>::from_byte_len(USIZE_MAX_VALID_BYTES + 1);
        if cfg!(target_pointer_width = "64") {
            assert!(r.is_err());
        } else {
            match r {
                Ok(bits) => {
                    assert_eq!(
                        bits.as_bits(),
                        (u64_from_usize(USIZE_MAX_VALID_BYTES) + 1) * 8
                    );
                }
                Err(_) => unreachable!(),
            }
        }
    }

    const U64_MAX_VALID_BYTES: u64 = u64::MAX / 8;

    match BitLength::<u64>::from_byte_len(U64_MAX_VALID_BYTES) {
        Ok(bits) => assert_eq!(bits.as_bits(), u64::MAX & !0b111),
        Err(_) => unreachable!(),
    };

    assert!(BitLength::<u64>::from_byte_len(U64_MAX_VALID_BYTES + 1).is_err());
}

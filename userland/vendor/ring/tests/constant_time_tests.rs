#![allow(missing_docs)]

#[allow(deprecated)]
use constant_time::verify_slices_are_equal;
#[allow(deprecated)]
use ring::constant_time;
use ring::{error, rand};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::{wasm_bindgen_test as test, wasm_bindgen_test_configure};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
wasm_bindgen_test_configure!(run_in_browser);

#[allow(deprecated)]
#[test]
fn test_verify_slices_are_equal() {
    let initial: [u8; 256] = rand::generate(&rand::SystemRandom::new()).unwrap().expose();

    {
        let copy = initial;
        for len in 0..copy.len() {

            assert_eq!(
                verify_slices_are_equal(&initial, &copy[..len]),
                Err(error::Unspecified)
            );

            assert_eq!(
                verify_slices_are_equal(&initial[..len], &copy[..len]),
                Ok(())
            );
        }

        assert_eq!(verify_slices_are_equal(&initial, &copy), Ok(()));
    }

    for i in 0..initial.len() {
        for bit in 0..8 {
            let mut copy = initial;
            copy[i] ^= 1u8 << bit;

            for len in 0..=initial.len() {

                assert_ne!(&initial[..], &copy[..]);

                let a = &initial[..len];
                let b = &copy[..len];

                let expected_result = if i < len {

                    Err(error::Unspecified)
                } else {

                    Ok(())
                };
                assert_eq!(a == b, expected_result.is_ok()); 
                assert_eq!(verify_slices_are_equal(a, b), expected_result);
                assert_eq!(verify_slices_are_equal(b, a), expected_result);
            }
        }
    }
}

#![allow(missing_docs)]

use ring::rand::{self, SecureRandom as _};
#[allow(deprecated)]
use ring::test;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::{wasm_bindgen_test as test, wasm_bindgen_test_configure};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
wasm_bindgen_test_configure!(run_in_browser);

#[test]
fn test_system_random_lengths() {
    const LINUX_LIMIT: usize = 256;
    const WEB_LIMIT: usize = 65536;

    let lengths = [
        0,
        1,
        2,
        3,
        96,
        LINUX_LIMIT - 1,
        LINUX_LIMIT,
        LINUX_LIMIT + 1,
        LINUX_LIMIT * 2,
        511,
        512,
        513,
        4096,
        WEB_LIMIT - 1,
        WEB_LIMIT,
        WEB_LIMIT + 1,
        WEB_LIMIT * 2,
    ];

    for len in lengths.iter() {
        let mut buf = vec![0; *len];

        let rng = rand::SystemRandom::new();
        assert!(rng.fill(&mut buf).is_ok());

        if *len >= 96 {
            assert!(buf.iter().any(|x| *x != 0));
        }
    }
}

#[test]
fn test_system_random_traits() {
    test::compile_time_assert_clone::<rand::SystemRandom>();
    test::compile_time_assert_send::<rand::SystemRandom>();

    assert_eq!(
        "SystemRandom(())",
        format!("{:?}", rand::SystemRandom::new())
    );
}

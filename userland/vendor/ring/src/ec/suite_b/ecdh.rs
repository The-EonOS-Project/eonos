use super::{ops::*, private_key::*, public_key::*};
use crate::{agreement, cpu, ec, error};

macro_rules! ecdh {
    ( $NAME:ident, $curve:expr, $name_str:expr, $private_key_ops:expr,
      $public_key_ops:expr, $ecdh:ident ) => {
        #[doc = "ECDH using the NSA Suite B"]
        #[doc=$name_str]
        #[doc = "curve."]

        pub static $NAME: agreement::Algorithm = agreement::Algorithm {
            curve: $curve,
            ecdh: $ecdh,
        };

        fn $ecdh(
            out: &mut [u8],
            my_private_key: &ec::Seed,
            peer_public_key: untrusted::Input,
            cpu: cpu::Features,
        ) -> Result<(), error::Unspecified> {
            ecdh(
                $private_key_ops,
                $public_key_ops,
                out,
                my_private_key,
                peer_public_key,
                cpu,
            )
        }
    };
}

ecdh!(
    ECDH_P256,
    &ec::suite_b::curve::P256,
    "P-256 (secp256r1)",
    &p256::PRIVATE_KEY_OPS,
    &p256::PUBLIC_KEY_OPS,
    p256_ecdh
);

ecdh!(
    ECDH_P384,
    &ec::suite_b::curve::P384,
    "P-384 (secp384r1)",
    &p384::PRIVATE_KEY_OPS,
    &p384::PUBLIC_KEY_OPS,
    p384_ecdh
);

fn ecdh(
    private_key_ops: &PrivateKeyOps,
    public_key_ops: &PublicKeyOps,
    out: &mut [u8],
    my_private_key: &ec::Seed,
    peer_public_key: untrusted::Input,
    cpu: cpu::Features,
) -> Result<(), error::Unspecified> {

    let q = &public_key_ops.common.elem_modulus(cpu);

    let peer_public_key = parse_uncompressed_point(public_key_ops, q, peer_public_key)?;

    let n = &private_key_ops.common.scalar_modulus(cpu);
    let my_private_key = private_key_as_scalar(n, my_private_key);
    let product = private_key_ops.point_mul(&my_private_key, &peer_public_key, cpu);

    big_endian_affine_from_jacobian(private_key_ops, q, out, None, &product)

}

#[cfg(test)]
mod tests {
    use super::super::ops;
    use crate::testutil as test;
    use crate::{agreement, ec, limb};

    static SUPPORTED_SUITE_B_ALGS: [(&str, &agreement::Algorithm, &ec::Curve, &ops::CommonOps); 2] = [
        (
            "P-256",
            &agreement::ECDH_P256,
            &super::super::curve::P256,
            &ops::p256::COMMON_OPS,
        ),
        (
            "P-384",
            &agreement::ECDH_P384,
            &super::super::curve::P384,
            &ops::p384::COMMON_OPS,
        ),
    ];

    #[test]
    fn test_agreement_suite_b_ecdh_generate() {

        let random_00 = test::rand::FixedByteRandom { byte: 0x00 };

        let random_ff = test::rand::FixedByteRandom { byte: 0xff };

        for &(_, alg, curve, ops) in SUPPORTED_SUITE_B_ALGS.iter() {

            assert!(agreement::EphemeralPrivateKey::generate(alg, &random_00).is_err());

            assert!(agreement::EphemeralPrivateKey::generate(alg, &random_ff).is_err());

            let mut n_bytes = [0u8; ec::SCALAR_MAX_BYTES];
            let num_bytes = curve.elem_scalar_seed_len;
            limb::big_endian_from_limbs(ops.n_limbs(), &mut n_bytes[..num_bytes]);
            {
                let n_bytes = &mut n_bytes[..num_bytes];
                let rng = test::rand::FixedSliceRandom { bytes: n_bytes };
                assert!(agreement::EphemeralPrivateKey::generate(alg, &rng).is_err());
            }

            let mut n_minus_1_bytes = n_bytes;
            {
                let n_minus_1_bytes = &mut n_minus_1_bytes[..num_bytes];
                n_minus_1_bytes[num_bytes - 1] -= 1;
                let rng = test::rand::FixedSliceRandom {
                    bytes: n_minus_1_bytes,
                };
                let key = agreement::EphemeralPrivateKey::generate(alg, &rng).unwrap();
                assert_eq!(n_minus_1_bytes, key.bytes_for_test());
            }

            let mut n_plus_1_bytes = n_bytes;
            {
                let n_plus_1_bytes = &mut n_plus_1_bytes[..num_bytes];
                n_plus_1_bytes[num_bytes - 1] += 1;
                let rng = test::rand::FixedSliceRandom {
                    bytes: n_plus_1_bytes,
                };
                assert!(agreement::EphemeralPrivateKey::generate(alg, &rng).is_err());
            }

            {
                let bytes = [
                    &n_bytes[..num_bytes],
                    &n_plus_1_bytes[..num_bytes],
                    &[0u8; ec::SCALAR_MAX_BYTES][..num_bytes],
                    &n_minus_1_bytes[..num_bytes],
                ];
                let rng = test::rand::FixedSliceSequenceRandom {
                    bytes: &bytes,
                    current: core::cell::UnsafeCell::new(0),
                };
                let key = agreement::EphemeralPrivateKey::generate(alg, &rng).unwrap();
                assert_eq!(&n_minus_1_bytes[..num_bytes], key.bytes_for_test());
            }
        }
    }
}

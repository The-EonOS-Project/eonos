use crate::{cpu, ec, error, rand};

macro_rules! suite_b_curve {
    ( $NAME:ident, $bits:expr, $private_key_ops:expr, $id:expr,
      $check_private_key_bytes:ident, $generate_private_key:ident,
      $public_from_private:ident) => {

        pub static $NAME: ec::Curve = ec::Curve {
            public_key_len: 1 + (2 * (($bits + 7) / 8)),
            elem_scalar_seed_len: ($bits + 7) / 8,
            id: $id,
            check_private_key_bytes: $check_private_key_bytes,
            generate_private_key: $generate_private_key,
            public_from_private: $public_from_private,
        };

        fn $check_private_key_bytes(
            bytes: &[u8],
            cpu: cpu::Features,
        ) -> Result<(), error::Unspecified> {
            debug_assert_eq!(bytes.len(), $bits / 8);
            ec::suite_b::private_key::check_scalar_big_endian_bytes($private_key_ops, bytes, cpu)
        }

        fn $generate_private_key(
            rng: &dyn rand::SecureRandom,
            out: &mut [u8],
            cpu: cpu::Features,
        ) -> Result<(), error::Unspecified> {
            ec::suite_b::private_key::generate_private_scalar_bytes($private_key_ops, rng, out, cpu)
        }

        fn $public_from_private(
            public_out: &mut [u8],
            private_key: &ec::Seed,
            cpu: cpu::Features,
        ) -> Result<(), error::Unspecified> {
            ec::suite_b::private_key::public_from_private(
                $private_key_ops,
                public_out,
                private_key,
                cpu,
            )
        }
    };
}

suite_b_curve!(
    P256,
    256,
    &ec::suite_b::ops::p256::PRIVATE_KEY_OPS,
    ec::CurveID::P256,
    p256_check_private_key_bytes,
    p256_generate_private_key,
    p256_public_from_private
);

suite_b_curve!(
    P384,
    384,
    &ec::suite_b::ops::p384::PRIVATE_KEY_OPS,
    ec::CurveID::P384,
    p384_check_private_key_bytes,
    p384_generate_private_key,
    p384_public_from_private
);

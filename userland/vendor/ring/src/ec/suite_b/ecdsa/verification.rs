use super::digest_scalar::digest_scalar;
use crate::{
    arithmetic::montgomery::*,
    cpu, digest,
    ec::suite_b::{ops::*, public_key::*, verify_jacobian_point_is_on_the_curve},
    error,
    io::der,
    limb, sealed, signature,
};

pub struct EcdsaVerificationAlgorithm {
    ops: &'static PublicScalarOps,
    digest_alg: &'static digest::Algorithm,
    split_rs:
        for<'a> fn(
            ops: &'static ScalarOps,
            input: &mut untrusted::Reader<'a>,
        )
            -> Result<(untrusted::Input<'a>, untrusted::Input<'a>), error::Unspecified>,
    id: AlgorithmID,
}

#[derive(Debug)]
enum AlgorithmID {
    ECDSA_P256_SHA256_ASN1,
    ECDSA_P256_SHA256_FIXED,
    ECDSA_P256_SHA384_ASN1,
    ECDSA_P384_SHA256_ASN1,
    ECDSA_P384_SHA384_ASN1,
    ECDSA_P384_SHA384_FIXED,
}

derive_debug_via_id!(EcdsaVerificationAlgorithm);

impl signature::VerificationAlgorithm for EcdsaVerificationAlgorithm {
    fn verify(
        &self,
        public_key: untrusted::Input,
        msg: untrusted::Input,
        signature: untrusted::Input,
    ) -> Result<(), error::Unspecified> {
        let cpu = cpu::features();
        let e = {

            let h = digest::digest(self.digest_alg, msg.as_slice_less_safe());

            let n = &self.ops.scalar_ops.scalar_modulus(cpu);
            digest_scalar(n, h)
        };

        self.verify_digest(public_key, e, signature)
    }
}

impl EcdsaVerificationAlgorithm {

    fn verify_digest(
        &self,
        public_key: untrusted::Input,
        e: Scalar,
        signature: untrusted::Input,
    ) -> Result<(), error::Unspecified> {
        let cpu = cpu::features();

        let public_key_ops = self.ops.public_key_ops;
        let scalar_ops = self.ops.scalar_ops;
        let q = &public_key_ops.common.elem_modulus(cpu);
        let n = &scalar_ops.scalar_modulus(cpu);

        let peer_pub_key = parse_uncompressed_point(public_key_ops, q, public_key)?;

        let (r, s) = signature.read_all(error::Unspecified, |input| {
            (self.split_rs)(scalar_ops, input)
        })?;

        let r = scalar_parse_big_endian_variable(n, limb::AllowZero::No, r)?;
        let s = scalar_parse_big_endian_variable(n, limb::AllowZero::No, s)?;

        let w = self.ops.scalar_inv_to_mont_vartime(&s, cpu);

        let u1 = scalar_ops.scalar_product(&e, &w, cpu);
        let u2 = scalar_ops.scalar_product(&r, &w, cpu);

        let product = (self.ops.twin_mul)(&u1, &u2, &peer_pub_key, cpu);

        let z2 = verify_jacobian_point_is_on_the_curve(q, &product)?;

        let x = q.point_x(&product);
        fn sig_r_equals_x(q: &Modulus<Q>, r: &Elem<Unencoded>, x: &Elem<R>, z2: &Elem<R>) -> bool {
            let r_jacobian = q.elem_product(z2, r);
            let x = q.elem_unencoded(x);
            q.elems_are_equal(&r_jacobian, &x).leak()
        }
        let mut r = self.ops.scalar_as_elem(&r);
        if sig_r_equals_x(q, &r, &x, &z2) {
            return Ok(());
        }
        if q.elem_less_than_vartime(&r, &self.ops.q_minus_n) {
            let n = Elem::from(self.ops.n());
            q.add_assign(&mut r, &n);
            if sig_r_equals_x(q, &r, &x, &z2) {
                return Ok(());
            }
        }

        Err(error::Unspecified)
    }
}

impl sealed::Sealed for EcdsaVerificationAlgorithm {}

fn split_rs_fixed<'a>(
    ops: &'static ScalarOps,
    input: &mut untrusted::Reader<'a>,
) -> Result<(untrusted::Input<'a>, untrusted::Input<'a>), error::Unspecified> {
    let scalar_len = ops.scalar_bytes_len();
    let r = input.read_bytes(scalar_len)?;
    let s = input.read_bytes(scalar_len)?;
    Ok((r, s))
}

fn split_rs_asn1<'a>(
    _ops: &'static ScalarOps,
    input: &mut untrusted::Reader<'a>,
) -> Result<(untrusted::Input<'a>, untrusted::Input<'a>), error::Unspecified> {
    der::nested(input, der::Tag::Sequence, error::Unspecified, |input| {
        let r = der::positive_integer(input)?.big_endian_without_leading_zero_as_input();
        let s = der::positive_integer(input)?.big_endian_without_leading_zero_as_input();
        Ok((r, s))
    })
}

pub static ECDSA_P256_SHA256_FIXED: EcdsaVerificationAlgorithm = EcdsaVerificationAlgorithm {
    ops: &p256::PUBLIC_SCALAR_OPS,
    digest_alg: &digest::SHA256,
    split_rs: split_rs_fixed,
    id: AlgorithmID::ECDSA_P256_SHA256_FIXED,
};

pub static ECDSA_P384_SHA384_FIXED: EcdsaVerificationAlgorithm = EcdsaVerificationAlgorithm {
    ops: &p384::PUBLIC_SCALAR_OPS,
    digest_alg: &digest::SHA384,
    split_rs: split_rs_fixed,
    id: AlgorithmID::ECDSA_P384_SHA384_FIXED,
};

pub static ECDSA_P256_SHA256_ASN1: EcdsaVerificationAlgorithm = EcdsaVerificationAlgorithm {
    ops: &p256::PUBLIC_SCALAR_OPS,
    digest_alg: &digest::SHA256,
    split_rs: split_rs_asn1,
    id: AlgorithmID::ECDSA_P256_SHA256_ASN1,
};

pub static ECDSA_P256_SHA384_ASN1: EcdsaVerificationAlgorithm = EcdsaVerificationAlgorithm {
    ops: &p256::PUBLIC_SCALAR_OPS,
    digest_alg: &digest::SHA384,
    split_rs: split_rs_asn1,
    id: AlgorithmID::ECDSA_P256_SHA384_ASN1,
};

pub static ECDSA_P384_SHA256_ASN1: EcdsaVerificationAlgorithm = EcdsaVerificationAlgorithm {
    ops: &p384::PUBLIC_SCALAR_OPS,
    digest_alg: &digest::SHA256,
    split_rs: split_rs_asn1,
    id: AlgorithmID::ECDSA_P384_SHA256_ASN1,
};

pub static ECDSA_P384_SHA384_ASN1: EcdsaVerificationAlgorithm = EcdsaVerificationAlgorithm {
    ops: &p384::PUBLIC_SCALAR_OPS,
    digest_alg: &digest::SHA384,
    split_rs: split_rs_asn1,
    id: AlgorithmID::ECDSA_P384_SHA384_ASN1,
};

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use crate::testutil as test;
    use alloc::{vec, vec::Vec};

    #[test]
    fn test_digest_based_test_vectors() {
        let cpu = cpu::features();
        test::run(
            test_vector_file!("../../../../crypto/fipsmodule/ecdsa/ecdsa_verify_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                let curve_name = test_case.consume_string("Curve");

                let public_key = {
                    let mut public_key = vec![0x04];
                    public_key.extend(&test_case.consume_bytes("X"));
                    public_key.extend(&test_case.consume_bytes("Y"));
                    public_key
                };

                let digest = test_case.consume_bytes("Digest");

                let sig = {
                    let mut sig = Vec::new();
                    sig.extend(&test_case.consume_bytes("R"));
                    sig.extend(&test_case.consume_bytes("S"));
                    sig
                };

                let invalid = test_case.consume_optional_string("Invalid");

                let alg = match curve_name.as_str() {
                    "P-256" => &ECDSA_P256_SHA256_FIXED,
                    "P-384" => &ECDSA_P384_SHA384_FIXED,
                    _ => {
                        panic!("Unsupported curve: {}", curve_name);
                    }
                };
                let n = &alg.ops.scalar_ops.scalar_modulus(cpu);

                let digest = super::super::digest_scalar::digest_bytes_scalar(n, &digest[..]);
                let actual_result = alg.verify_digest(
                    untrusted::Input::from(&public_key[..]),
                    digest,
                    untrusted::Input::from(&sig[..]),
                );
                assert_eq!(actual_result.is_ok(), invalid.is_none());

                Ok(())
            },
        );
    }
}

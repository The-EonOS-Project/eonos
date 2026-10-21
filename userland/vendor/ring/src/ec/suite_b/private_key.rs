use super::{ops::*, verify_affine_point_is_on_the_curve};
use crate::{arithmetic::montgomery::R, cpu, ec, error, limb, rand};

pub(super) fn random_scalar(
    ops: &PrivateKeyOps,
    n: &Modulus<N>,
    rng: &dyn rand::SecureRandom,
) -> Result<Scalar, error::Unspecified> {
    let mut bytes = [0; ec::SCALAR_MAX_BYTES];
    let bytes = &mut bytes[..ops.common.len()];
    generate_private_scalar_bytes(ops, rng, bytes, n.cpu())?;
    scalar_from_big_endian_bytes(n, bytes)
}

pub(super) fn generate_private_scalar_bytes(
    ops: &PrivateKeyOps,
    rng: &dyn rand::SecureRandom,
    out: &mut [u8],
    cpu: cpu::Features,
) -> Result<(), error::Unspecified> {

    let candidate = out;

    for _ in 0..100 {

        rng.fill(candidate)?;

        if check_scalar_big_endian_bytes(ops, candidate, cpu).is_err() {
            continue;
        }

        return Ok(());
    }

    Err(error::Unspecified)
}

#[inline]
pub(super) fn private_key_as_scalar(n: &Modulus<N>, private_key: &ec::Seed) -> Scalar {

    scalar_from_big_endian_bytes(n, private_key.bytes_less_safe()).unwrap()
}

pub(super) fn check_scalar_big_endian_bytes(
    ops: &PrivateKeyOps,
    bytes: &[u8],
    cpu: cpu::Features,
) -> Result<(), error::Unspecified> {
    debug_assert_eq!(bytes.len(), ops.common.len());
    let n = &ops.common.scalar_modulus(cpu);
    scalar_from_big_endian_bytes(n, bytes).map(|_| ())
}

pub(super) fn scalar_from_big_endian_bytes(
    n: &Modulus<N>,
    bytes: &[u8],
) -> Result<Scalar, error::Unspecified> {

    scalar_parse_big_endian_fixed_consttime(n, untrusted::Input::from(bytes))
}

pub(super) fn public_from_private(
    ops: &PrivateKeyOps,
    public_out: &mut [u8],
    my_private_key: &ec::Seed,
    cpu: cpu::Features,
) -> Result<(), error::Unspecified> {
    let q = &ops.common.elem_modulus(cpu);
    let elem_and_scalar_bytes = ops.common.len();
    debug_assert_eq!(public_out.len(), 1 + (2 * elem_and_scalar_bytes));
    let n = &ops.common.scalar_modulus(cpu);
    let my_private_key = private_key_as_scalar(n, my_private_key);
    let my_public_key = ops.point_mul_base(&my_private_key, cpu);
    public_out[0] = 4; 
    let (x_out, y_out) = public_out[1..].split_at_mut(elem_and_scalar_bytes);

    big_endian_affine_from_jacobian(ops, q, x_out, Some(y_out), &my_public_key)
}

pub(super) fn affine_from_jacobian(
    ops: &PrivateKeyOps,
    q: &Modulus<Q>,
    p: &Point,
) -> Result<(Elem<R>, Elem<R>), error::Unspecified> {
    let z = q.point_z(p);

    assert!(q.elem_verify_is_not_zero(&z).is_ok());

    let x = q.point_x(p);
    let y = q.point_y(p);

    let zz_inv = ops.elem_inverse_squared(q, &z);

    let x_aff = q.elem_product(&x, &zz_inv);

    let y_aff = {
        let zzzz_inv = q.elem_squared(&zz_inv);
        let zzz_inv = q.elem_product(&z, &zzzz_inv);
        q.elem_product(&y, &zzz_inv)
    };

    verify_affine_point_is_on_the_curve(q, (&x_aff, &y_aff))?;

    Ok((x_aff, y_aff))
}

pub(super) fn big_endian_affine_from_jacobian(
    ops: &PrivateKeyOps,
    q: &Modulus<Q>,
    x_out: &mut [u8],
    y_out: Option<&mut [u8]>,
    p: &Point,
) -> Result<(), error::Unspecified> {
    let (x_aff, y_aff) = affine_from_jacobian(ops, q, p)?;
    let x = q.elem_unencoded(&x_aff);
    limb::big_endian_from_limbs(ops.leak_limbs(&x), x_out);
    if let Some(y_out) = y_out {
        let y = q.elem_unencoded(&y_aff);
        limb::big_endian_from_limbs(ops.leak_limbs(&y), y_out);
    }

    Ok(())
}

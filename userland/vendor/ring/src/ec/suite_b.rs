use self::ops::*;
use crate::{arithmetic::montgomery::*, cpu, ec, error, io::der, pkcs8};

fn verify_affine_point_is_on_the_curve(
    q: &Modulus<Q>,
    (x, y): (&Elem<R>, &Elem<R>),
) -> Result<(), error::Unspecified> {
    verify_affine_point_is_on_the_curve_scaled(q, (x, y), &Elem::from(q.a()), &Elem::from(q.b()))
}

fn verify_jacobian_point_is_on_the_curve(
    q: &Modulus<Q>,
    p: &Point,
) -> Result<Elem<R>, error::Unspecified> {
    let z = q.point_z(p);

    q.elem_verify_is_not_zero(&z)?;

    let x = q.point_x(p);
    let y = q.point_y(p);

    let z2 = q.elem_squared(&z);
    let z4 = q.elem_squared(&z2);
    let z4_a = q.elem_product(&z4, &Elem::from(q.a()));
    let z6 = q.elem_product(&z4, &z2);
    let z6_b = q.elem_product(&z6, &Elem::from(q.b()));
    verify_affine_point_is_on_the_curve_scaled(q, (&x, &y), &z4_a, &z6_b)?;
    Ok(z2)
}

fn verify_affine_point_is_on_the_curve_scaled(
    q: &Modulus<Q>,
    (x, y): (&Elem<R>, &Elem<R>),
    a_scaled: &Elem<R>,
    b_scaled: &Elem<R>,
) -> Result<(), error::Unspecified> {
    let lhs = q.elem_squared(y);

    let mut rhs = q.elem_squared(x);
    q.add_assign(&mut rhs, a_scaled);
    q.elem_mul(&mut rhs, x);
    q.add_assign(&mut rhs, b_scaled);

    if !q.elems_are_equal(&lhs, &rhs).leak() {
        return Err(error::Unspecified);
    }

    Ok(())
}

pub(crate) fn key_pair_from_pkcs8(
    curve: &'static ec::Curve,
    template: &pkcs8::Template,
    input: untrusted::Input,
    cpu_features: cpu::Features,
) -> Result<ec::KeyPair, error::KeyRejected> {
    let (ec_private_key, _) = pkcs8::unwrap_key(template, pkcs8::Version::V1Only, input)?;
    let (private_key, public_key) =
        ec_private_key.read_all(error::KeyRejected::invalid_encoding(), |input| {

            der::nested(
                input,
                der::Tag::Sequence,
                error::KeyRejected::invalid_encoding(),
                |input| key_pair_from_pkcs8_(template, input),
            )
        })?;
    key_pair_from_bytes(curve, private_key, public_key, cpu_features)
}

fn key_pair_from_pkcs8_<'a>(
    template: &pkcs8::Template,
    input: &mut untrusted::Reader<'a>,
) -> Result<(untrusted::Input<'a>, untrusted::Input<'a>), error::KeyRejected> {
    let version = der::small_nonnegative_integer(input)
        .map_err(|error::Unspecified| error::KeyRejected::invalid_encoding())?;
    if version != 1 {
        return Err(error::KeyRejected::version_not_supported());
    }

    let private_key = der::expect_tag_and_get_value(input, der::Tag::OctetString)
        .map_err(|error::Unspecified| error::KeyRejected::invalid_encoding())?;

    if input.peek(u8::from(der::Tag::ContextSpecificConstructed0)) {
        let actual_alg_id =
            der::expect_tag_and_get_value(input, der::Tag::ContextSpecificConstructed0)
                .map_err(|error::Unspecified| error::KeyRejected::invalid_encoding())?;
        if actual_alg_id.as_slice_less_safe() != template.curve_oid().as_slice_less_safe() {
            return Err(error::KeyRejected::wrong_algorithm());
        }
    }

    let public_key = der::nested(
        input,
        der::Tag::ContextSpecificConstructed1,
        error::Unspecified,
        der::bit_string_with_no_unused_bits,
    )
    .map_err(|error::Unspecified| error::KeyRejected::invalid_encoding())?;

    Ok((private_key, public_key))
}

pub(crate) fn key_pair_from_bytes(
    curve: &'static ec::Curve,
    private_key_bytes: untrusted::Input,
    public_key_bytes: untrusted::Input,
    cpu_features: cpu::Features,
) -> Result<ec::KeyPair, error::KeyRejected> {
    let seed = ec::Seed::from_bytes(curve, private_key_bytes, cpu_features)
        .map_err(|error::Unspecified| error::KeyRejected::invalid_component())?;

    let r = ec::KeyPair::derive(seed, cpu_features)
        .map_err(|error::Unspecified| error::KeyRejected::unexpected_error())?;
    if public_key_bytes.as_slice_less_safe() != r.public_key().as_ref() {
        return Err(error::KeyRejected::inconsistent_components());
    }

    Ok(r)
}

pub mod curve;

pub mod ecdh;

pub mod ecdsa;

mod ops;

mod private_key;
mod public_key;

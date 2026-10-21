use super::{
    elem::{binary_op, binary_op_assign},
    elem_sqr_mul, elem_sqr_mul_acc, PublicModulus, *,
};

pub(super) const NUM_LIMBS: usize = 256 / LIMB_BITS;

pub static COMMON_OPS: CommonOps = CommonOps {
    num_limbs: elem::NumLimbs::P256,

    q: PublicModulus {
        p: limbs_from_hex("ffffffff00000001000000000000000000000000ffffffffffffffffffffffff"),
        rr: PublicElem::from_hex("4fffffffdfffffffffffffffefffffffbffffffff0000000000000003"),
    },
    n: PublicElem::from_hex("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551"),

    a: PublicElem::from_hex("fffffffc00000004000000000000000000000003fffffffffffffffffffffffc"),
    b: PublicElem::from_hex("dc30061d04874834e5a220abf7212ed6acf005cd78843090d89cdf6229c4bddf"),

    elem_mul_mont: p256_mul_mont,
    elem_sqr_mont: p256_sqr_mont,
};

#[cfg(test)]
pub(super) static GENERATOR: (PublicElem<R>, PublicElem<R>) = (
    PublicElem::from_hex("18905f76a53755c679fb732b7762251075ba95fc5fedb60179e730d418a9143c"),
    PublicElem::from_hex("8571ff1825885d85d2e88688dd21f3258b4ab8e4ba19e45cddf25357ce95560a"),
);

pub static PRIVATE_KEY_OPS: PrivateKeyOps = PrivateKeyOps {
    common: &COMMON_OPS,
    elem_inv_squared: p256_elem_inv_squared,
    point_mul_base_impl: p256_point_mul_base_impl,
    point_mul_impl: p256_point_mul,
    point_add_jacobian_impl: p256_point_add,
};

fn p256_elem_inv_squared(q: &Modulus<Q>, a: &Elem<R>) -> Elem<R> {

    #[inline]
    fn sqr_mul(q: &Modulus<Q>, a: &Elem<R>, squarings: LeakyWord, b: &Elem<R>) -> Elem<R> {
        elem_sqr_mul(&COMMON_OPS, a, squarings, b, q.cpu())
    }

    #[inline]
    fn sqr_mul_acc(q: &Modulus<Q>, a: &mut Elem<R>, squarings: LeakyWord, b: &Elem<R>) {
        elem_sqr_mul_acc(&COMMON_OPS, a, squarings, b, q.cpu())
    }

    let b_1 = &a;
    let b_11 = sqr_mul(q, b_1, 1, b_1);
    let b_111 = sqr_mul(q, &b_11, 1, b_1);
    let f_11 = sqr_mul(q, &b_111, 3, &b_111);
    let fff = sqr_mul(q, &f_11, 6, &f_11);
    let fff_111 = sqr_mul(q, &fff, 3, &b_111);
    let fffffff_11 = sqr_mul(q, &fff_111, 15, &fff_111);
    let ffffffff = sqr_mul(q, &fffffff_11, 2, &b_11);

    let mut acc = sqr_mul(q, &ffffffff, 31 + 1, b_1);

    sqr_mul_acc(q, &mut acc, 96 + 32, &ffffffff);

    sqr_mul_acc(q, &mut acc, 32, &ffffffff);

    sqr_mul_acc(q, &mut acc, 30, &fffffff_11);

    q.elem_square(&mut acc);
    q.elem_square(&mut acc);

    acc
}

fn p256_point_mul_base_impl(g_scalar: &Scalar, _cpu: cpu::Features) -> Point {
    prefixed_extern! {
        fn p256_point_mul_base(
            r: *mut Limb,          
            g_scalar: *const Limb, 
        );
    }

    let mut r = Point::new_at_infinity();
    unsafe {
        p256_point_mul_base(r.xyz.as_mut_ptr(), g_scalar.limbs.as_ptr());
    }
    r
}

pub static PUBLIC_KEY_OPS: PublicKeyOps = PublicKeyOps {
    common: &COMMON_OPS,
};

pub static SCALAR_OPS: ScalarOps = ScalarOps {
    common: &COMMON_OPS,
    scalar_mul_mont: p256_scalar_mul_mont,
};

pub static PUBLIC_SCALAR_OPS: PublicScalarOps = PublicScalarOps {
    scalar_ops: &SCALAR_OPS,
    public_key_ops: &PUBLIC_KEY_OPS,

    #[cfg(any(
        all(target_arch = "aarch64", target_endian = "little"),
        target_arch = "x86_64"
    ))]
    twin_mul: twin_mul_nistz256,

    #[cfg(not(any(
        all(target_arch = "aarch64", target_endian = "little"),
        target_arch = "x86_64"
    )))]
    twin_mul: |g_scalar, p_scalar, p_xy, cpu| {
        twin_mul_inefficient(&PRIVATE_KEY_OPS, g_scalar, p_scalar, p_xy, cpu)
    },

    q_minus_n: PublicElem::from_hex("4319055358e8617b0c46353d039cdaae"),

    scalar_inv_to_mont_vartime: |s, cpu| PRIVATE_SCALAR_OPS.scalar_inv_to_mont(s, cpu),
};

#[cfg(any(
    all(target_arch = "aarch64", target_endian = "little"),
    target_arch = "x86_64"
))]
fn twin_mul_nistz256(
    g_scalar: &Scalar,
    p_scalar: &Scalar,
    p_xy: &(Elem<R>, Elem<R>),
    cpu: cpu::Features,
) -> Point {
    let scaled_g = point_mul_base_vartime(g_scalar, cpu);
    let scaled_p = PRIVATE_KEY_OPS.point_mul(p_scalar, p_xy, cpu::features());
    PRIVATE_KEY_OPS.point_sum(&scaled_g, &scaled_p, cpu)
}

#[cfg(any(
    all(target_arch = "aarch64", target_endian = "little"),
    target_arch = "x86_64"
))]
fn point_mul_base_vartime(g_scalar: &Scalar, _cpu: cpu::Features) -> Point {
    prefixed_extern! {
        fn p256_point_mul_base_vartime(r: *mut Limb,          
                                       g_scalar: *const Limb, 
        );
    }
    let mut scaled_g = Point::new_at_infinity();
    unsafe {
        p256_point_mul_base_vartime(scaled_g.xyz.as_mut_ptr(), g_scalar.limbs.as_ptr());
    }
    scaled_g
}

pub static PRIVATE_SCALAR_OPS: PrivateScalarOps = PrivateScalarOps {
    scalar_ops: &SCALAR_OPS,

    oneRR_mod_n: PublicScalar::from_hex(
        "66e12d94f3d956202845b2392b6bec594699799c49bd6fa683244c95be79eea2",
    ),
    scalar_inv_to_mont: p256_scalar_inv_to_mont,
};

#[allow(clippy::just_underscores_and_digits)]
fn p256_scalar_inv_to_mont(a: Scalar<R>, _cpu: cpu::Features) -> Scalar<R> {

    #[inline]
    fn mul(a: &Scalar<R>, b: &Scalar<R>) -> Scalar<R> {
        binary_op(p256_scalar_mul_mont, a, b)
    }

    #[inline]
    fn sqr(a: &Scalar<R>) -> Scalar<R> {
        let mut tmp = Scalar::zero();
        unsafe { p256_scalar_sqr_rep_mont(tmp.limbs.as_mut_ptr(), a.limbs.as_ptr(), 1) }
        tmp
    }

    fn sqr_mul(a: &Scalar<R>, squarings: LeakyWord, b: &Scalar<R>) -> Scalar<R> {
        debug_assert!(squarings >= 1);
        let mut tmp = Scalar::zero();
        unsafe { p256_scalar_sqr_rep_mont(tmp.limbs.as_mut_ptr(), a.limbs.as_ptr(), squarings) }
        mul(&tmp, b)
    }

    fn sqr_mul_acc(acc: &mut Scalar<R>, squarings: LeakyWord, b: &Scalar<R>) {
        debug_assert!(squarings >= 1);
        unsafe { p256_scalar_sqr_rep_mont(acc.limbs.as_mut_ptr(), acc.limbs.as_ptr(), squarings) }
        binary_op_assign(p256_scalar_mul_mont, acc, b);
    }

    let _1 = &a;

    let _10 = sqr(_1); 
    let _100 = sqr(&_10); 
    let _101 = mul(&_100, _1); 
    let _111 = mul(&_101, &_10); 

    let _1000 = sqr(&_100); 
    let _10000 = sqr(&_1000); 
    let _100000 = sqr(&_10000); 

    let _100111 = mul(&_111, &_100000); 
    let _101011 = mul(&_100, &_100111); 
    let _101111 = mul(&_100, &_101011); 
    let _1001111 = mul(&_100000, &_101111); 
    let _86 = sqr(&_101011); 
    let _1011011 = mul(&_101, &_86); 
    let _92 = mul(_1, &_1011011); 
    let _1100011 = mul(&_111, &_92); 
    let _10111111 = mul(&_92, &_1100011); 
    let _11011111 = mul(&_100000, &_10111111); 

    let ff = mul(&_100000, &_11011111); 
    let ffff = sqr_mul(&ff, 0 + 8, &ff);
    let ffffffff = sqr_mul(&ffff, 0 + 16, &ffff);

    let mut acc = sqr_mul(&ffffffff, 32 + 32, &ffffffff);

    sqr_mul_acc(&mut acc, 0 + 32, &ffffffff);

    sqr_mul_acc(&mut acc, 6, &_101111);
    sqr_mul_acc(&mut acc, 2 + 3, &_111);
    sqr_mul_acc(&mut acc, 2 + 8, &_11011111);
    sqr_mul_acc(&mut acc, 1 + 3, &_101);
    sqr_mul_acc(&mut acc, 1 + 7, &_1011011);
    sqr_mul_acc(&mut acc, 1 + 6, &_100111);
    sqr_mul_acc(&mut acc, 3 + 6, &_101111);
    sqr_mul_acc(&mut acc, 2 + 3, &_111);
    sqr_mul_acc(&mut acc, 3, &_101);
    sqr_mul_acc(&mut acc, 4 + 7, &_1001111);
    sqr_mul_acc(&mut acc, 2 + 3, &_111);
    sqr_mul_acc(&mut acc, 1 + 3, &_111);
    sqr_mul_acc(&mut acc, 2 + 3, &_111);
    sqr_mul_acc(&mut acc, 2 + 6, &_101011);
    sqr_mul_acc(&mut acc, 4 + 8, &_10111111);
    sqr_mul_acc(&mut acc, 3 + 7, &_1100011);
    sqr_mul_acc(&mut acc, 2 + 1, _1);
    sqr_mul_acc(&mut acc, 2 + 3, &_101);
    sqr_mul_acc(&mut acc, 1 + 7, &_1001111);

    acc
}

prefixed_extern! {
    pub(super) fn p256_mul_mont(
        r: *mut Limb,   
        a: *const Limb, 
        b: *const Limb, 
    );
    pub(super) fn p256_sqr_mont(
        r: *mut Limb,   
        a: *const Limb, 
    );

    fn p256_point_add(
        r: *mut Limb,   
        a: *const Limb, 
        b: *const Limb, 
    );
    fn p256_point_mul(
        r: *mut Limb,          
        p_scalar: *const Limb, 
        p_x: *const Limb,      
        p_y: *const Limb,      
    );

    fn p256_scalar_mul_mont(
        r: *mut Limb,   
        a: *const Limb, 
        b: *const Limb, 
    );
    fn p256_scalar_sqr_rep_mont(
        r: *mut Limb,   
        a: *const Limb, 
        rep: LeakyWord,
    );
}

#[cfg(test)]
mod tests {
    #[cfg(any(
        all(target_arch = "aarch64", target_endian = "little"),
        target_arch = "x86_64"
    ))]
    #[test]
    fn p256_point_mul_base_vartime_test() {
        use super::{super::tests::point_mul_base_tests, *};
        point_mul_base_tests(
            &PRIVATE_KEY_OPS,
            point_mul_base_vartime,
            test_vector_file!("p256_point_mul_base_tests.txt"),
        );
    }
}

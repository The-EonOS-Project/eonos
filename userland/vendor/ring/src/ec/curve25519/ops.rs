pub use super::scalar::{MaskedScalar, Scalar, SCALAR_LEN};
use crate::{
    bssl, cpu, error,
    limb::{Limb, LIMB_BITS},
};
use core::{ffi::c_int, marker::PhantomData};

#[repr(C)]
pub struct Elem<E: Encoding> {
    limbs: [Limb; ELEM_LIMBS], 
    encoding: PhantomData<E>,
}

pub trait Encoding {}
pub struct T;
impl Encoding for T {}

const ELEM_LIMBS: usize = 5 * 64 / LIMB_BITS;

impl<E: Encoding> Elem<E> {
    fn zero() -> Self {
        Self {
            limbs: Default::default(),
            encoding: PhantomData,
        }
    }
}

impl Elem<T> {
    fn negate(&mut self) {
        unsafe {
            x25519_fe_neg(self);
        }
    }
}

pub type EncodedPoint = [u8; ELEM_LEN];
pub const ELEM_LEN: usize = 32;

#[repr(C)]
pub struct ExtPoint {
    x: Elem<T>,
    y: Elem<T>,
    z: Elem<T>,
    t: Elem<T>,
}

impl ExtPoint {

    pub(super) fn from_scalarmult_base(scalar: &Scalar, cpu: cpu::Features) -> Self {
        let mut r = Self {
            x: Elem::zero(),
            y: Elem::zero(),
            z: Elem::zero(),
            t: Elem::zero(),
        };
        prefixed_extern! {
            fn x25519_ge_scalarmult_base(h: &mut ExtPoint, a: &Scalar, has_fe25519_adx: c_int);
        }
        unsafe {
            x25519_ge_scalarmult_base(&mut r, scalar, has_fe25519_adx(cpu).into());
        }
        r
    }

    pub fn from_encoded_point_vartime(encoded: &EncodedPoint) -> Result<Self, error::Unspecified> {
        let mut point = Self {
            x: Elem::zero(),
            y: Elem::zero(),
            z: Elem::zero(),
            t: Elem::zero(),
        };

        Result::from(unsafe { x25519_ge_frombytes_vartime(&mut point, encoded) }).map(|()| point)
    }

    pub(super) fn into_encoded_point(self, cpu_features: cpu::Features) -> EncodedPoint {
        encode_point(self.x, self.y, self.z, cpu_features)
    }

    pub(super) fn invert_vartime(&mut self) {
        self.x.negate();
        self.t.negate();
    }
}

#[repr(C)]
pub struct Point {
    x: Elem<T>,
    y: Elem<T>,
    z: Elem<T>,
}

impl Point {
    pub fn new_at_infinity() -> Self {
        Self {
            x: Elem::zero(),
            y: Elem::zero(),
            z: Elem::zero(),
        }
    }

    pub(super) fn into_encoded_point(self, cpu_features: cpu::Features) -> EncodedPoint {
        encode_point(self.x, self.y, self.z, cpu_features)
    }
}

fn encode_point(x: Elem<T>, y: Elem<T>, z: Elem<T>, _cpu_features: cpu::Features) -> EncodedPoint {
    let mut bytes = [0; ELEM_LEN];

    let sign_bit: u8 = unsafe {
        let mut recip = Elem::zero();
        x25519_fe_invert(&mut recip, &z);

        let mut x_over_z = Elem::zero();
        x25519_fe_mul_ttt(&mut x_over_z, &x, &recip);

        let mut y_over_z = Elem::zero();
        x25519_fe_mul_ttt(&mut y_over_z, &y, &recip);
        x25519_fe_tobytes(&mut bytes, &y_over_z);

        x25519_fe_isnegative(&x_over_z)
    };

    bytes[ELEM_LEN - 1] ^= sign_bit << 7;

    bytes
}

#[inline(always)]
pub(super) fn has_fe25519_adx(cpu: cpu::Features) -> bool {
    cfg_if::cfg_if! {
        if #[cfg(all(target_arch = "x86_64", not(target_os = "windows")))] {
            use cpu::{intel::{Adx, Bmi1, Bmi2}, GetFeature as _};
            matches!(cpu.get_feature(), Some((Adx { .. }, Bmi1 { .. }, Bmi2 { .. })))
        } else {
            let _ = cpu;
            false
        }
    }
}

prefixed_extern! {
    fn x25519_fe_invert(out: &mut Elem<T>, z: &Elem<T>);
    fn x25519_fe_isnegative(elem: &Elem<T>) -> u8;
    fn x25519_fe_mul_ttt(h: &mut Elem<T>, f: &Elem<T>, g: &Elem<T>);
    fn x25519_fe_neg(f: &mut Elem<T>);
    fn x25519_fe_tobytes(bytes: &mut EncodedPoint, elem: &Elem<T>);
    fn x25519_ge_frombytes_vartime(h: &mut ExtPoint, s: &EncodedPoint) -> bssl::Result;
}

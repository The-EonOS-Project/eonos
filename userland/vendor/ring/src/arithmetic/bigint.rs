





































use self::boxed_limbs::BoxedLimbs;
pub(crate) use self::{
    modulus::{Modulus, OwnedModulus},
    modulusvalue::OwnedModulusValue,
    private_exponent::PrivateExponent,
};
use super::{inout::AliasingSlices3, limbs512, montgomery::*, LimbSliceError, MAX_LIMBS};
use crate::{
    bits::BitLength,
    c,
    error::{self, LenMismatchError},
    limb::{self, Limb, LIMB_BITS},
    polyfill::slice::{self, AsChunks},
};
use core::{
    marker::PhantomData,
    num::{NonZeroU64, NonZeroUsize},
};

mod boxed_limbs;
mod modulus;
mod modulusvalue;
mod private_exponent;

pub trait PublicModulus {}




pub struct Storage<M> {
    limbs: BoxedLimbs<M>,
}

impl<M, E> From<Elem<M, E>> for Storage<M> {
    fn from(elem: Elem<M, E>) -> Self {
        Self { limbs: elem.limbs }
    }
}






pub struct Elem<M, E = Unencoded> {
    limbs: BoxedLimbs<M>,

    
    
    encoding: PhantomData<E>,
}

impl<M, E> Elem<M, E> {
    pub fn clone_into(&self, mut out: Storage<M>) -> Self {
        out.limbs.copy_from_slice(&self.limbs);
        Self {
            limbs: out.limbs,
            encoding: self.encoding,
        }
    }
}

impl<M, E> Elem<M, E> {
    #[inline]
    pub fn is_zero(&self) -> bool {
        limb::limbs_are_zero(&self.limbs).leak()
    }
}






fn from_montgomery_amm<M>(mut in_out: Storage<M>, m: &Modulus<M>) -> Elem<M, Unencoded> {
    let mut one = [0; MAX_LIMBS];
    one[0] = 1;
    let one = &one[..m.limbs().len()];
    limbs_mul_mont(
        (&mut in_out.limbs[..], one),
        m.limbs(),
        m.n0(),
        m.cpu_features(),
    )
    .unwrap_or_else(unwrap_impossible_limb_slice_error);
    Elem {
        limbs: in_out.limbs,
        encoding: PhantomData,
    }
}

#[cfg(any(test, not(target_arch = "x86_64")))]
impl<M> Elem<M, R> {
    #[inline]
    pub fn into_unencoded(self, m: &Modulus<M>) -> Elem<M, Unencoded> {
        from_montgomery_amm(Storage::from(self), m)
    }
}

impl<M> Elem<M, Unencoded> {
    pub fn from_be_bytes_padded(
        input: untrusted::Input,
        m: &Modulus<M>,
    ) -> Result<Self, error::Unspecified> {
        Ok(Self {
            limbs: BoxedLimbs::from_be_bytes_padded_less_than(input, m)?,
            encoding: PhantomData,
        })
    }

    #[inline]
    pub fn fill_be_bytes(&self, out: &mut [u8]) {
        
        limb::big_endian_from_limbs(&self.limbs, out)
    }
}

pub fn elem_mul_into<M, AF, BF>(
    mut out: Storage<M>,
    a: &Elem<M, AF>,
    b: &Elem<M, BF>,
    m: &Modulus<M>,
) -> Elem<M, <(AF, BF) as ProductEncoding>::Output>
where
    (AF, BF): ProductEncoding,
{
    limbs_mul_mont(
        (out.limbs.as_mut(), b.limbs.as_ref(), a.limbs.as_ref()),
        m.limbs(),
        m.n0(),
        m.cpu_features(),
    )
    .unwrap_or_else(unwrap_impossible_limb_slice_error);
    Elem {
        limbs: out.limbs,
        encoding: PhantomData,
    }
}

pub fn elem_mul<M, AF, BF>(
    a: &Elem<M, AF>,
    mut b: Elem<M, BF>,
    m: &Modulus<M>,
) -> Elem<M, <(AF, BF) as ProductEncoding>::Output>
where
    (AF, BF): ProductEncoding,
{
    limbs_mul_mont(
        (&mut b.limbs[..], &a.limbs[..]),
        m.limbs(),
        m.n0(),
        m.cpu_features(),
    )
    .unwrap_or_else(unwrap_impossible_limb_slice_error);
    Elem {
        limbs: b.limbs,
        encoding: PhantomData,
    }
}


fn elem_double<M, AF>(r: &mut Elem<M, AF>, m: &Modulus<M>) {
    limb::limbs_double_mod(&mut r.limbs, m.limbs())
        .unwrap_or_else(unwrap_impossible_len_mismatch_error)
}





pub fn elem_reduced_once<A, M>(
    mut r: Storage<M>,
    a: &Elem<A, Unencoded>,
    m: &Modulus<M>,
    other_modulus_len_bits: BitLength,
) -> Elem<M, Unencoded> {
    assert_eq!(m.len_bits(), other_modulus_len_bits);
    r.limbs.copy_from_slice(&a.limbs);
    limb::limbs_reduce_once(&mut r.limbs, m.limbs())
        .unwrap_or_else(unwrap_impossible_len_mismatch_error);
    Elem {
        limbs: r.limbs,
        encoding: PhantomData,
    }
}

#[inline]
pub fn elem_reduced<Larger, Smaller>(
    mut r: Storage<Smaller>,
    a: &Elem<Larger, Unencoded>,
    m: &Modulus<Smaller>,
    other_prime_len_bits: BitLength,
) -> Elem<Smaller, RInverse> {
    
    
    
    assert_eq!(other_prime_len_bits, m.len_bits());

    
    assert_eq!(a.limbs.len(), m.limbs().len() * 2);

    let mut tmp = [0; MAX_LIMBS];
    let tmp = &mut tmp[..a.limbs.len()];
    tmp.copy_from_slice(&a.limbs);

    limbs_from_mont_in_place(&mut r.limbs, tmp, m.limbs(), m.n0());
    Elem {
        limbs: r.limbs,
        encoding: PhantomData,
    }
}

#[inline]
fn elem_squared<M, E>(
    mut a: Elem<M, E>,
    m: &Modulus<M>,
) -> Elem<M, <(E, E) as ProductEncoding>::Output>
where
    (E, E): ProductEncoding,
{
    limbs_square_mont(&mut a.limbs, m.limbs(), m.n0(), m.cpu_features())
        .unwrap_or_else(unwrap_impossible_limb_slice_error);
    Elem {
        limbs: a.limbs,
        encoding: PhantomData,
    }
}

pub fn elem_widen<Larger, Smaller>(
    mut r: Storage<Larger>,
    a: Elem<Smaller, Unencoded>,
    m: &Modulus<Larger>,
    smaller_modulus_bits: BitLength,
) -> Result<Elem<Larger, Unencoded>, error::Unspecified> {
    if smaller_modulus_bits >= m.len_bits() {
        return Err(error::Unspecified);
    }
    let (to_copy, to_zero) = r.limbs.split_at_mut(a.limbs.len());
    to_copy.copy_from_slice(&a.limbs);
    to_zero.fill(0);
    Ok(Elem {
        limbs: r.limbs,
        encoding: PhantomData,
    })
}


pub fn elem_add<M, E>(mut a: Elem<M, E>, b: Elem<M, E>, m: &Modulus<M>) -> Elem<M, E> {
    limb::limbs_add_assign_mod(&mut a.limbs, &b.limbs, m.limbs())
        .unwrap_or_else(unwrap_impossible_len_mismatch_error);
    a
}


pub fn elem_sub<M, E>(mut a: Elem<M, E>, b: &Elem<M, E>, m: &Modulus<M>) -> Elem<M, E> {
    prefixed_extern! {
        
        fn LIMBS_sub_mod(
            r: *mut Limb,
            a: *const Limb,
            b: *const Limb,
            m: *const Limb,
            num_limbs: c::NonZero_size_t,
        );
    }
    let num_limbs = NonZeroUsize::new(m.limbs().len()).unwrap();
    (a.limbs.as_mut(), b.limbs.as_ref())
        .with_non_dangling_non_null_pointers_rab(num_limbs, |r, a, b| {
            let m = m.limbs().as_ptr(); 
            unsafe { LIMBS_sub_mod(r, a, b, m, num_limbs) }
        })
        .unwrap_or_else(unwrap_impossible_len_mismatch_error);
    a
}


pub struct One<M, E>(Elem<M, E>);

impl<M> One<M, RR> {
    
    
    
    
    
    
    
    pub(crate) fn newRR(mut out: Storage<M>, m: &Modulus<M>) -> Self {
        
        let w = m.limbs().len();

        
        let r = w * LIMB_BITS;

        m.oneR(&mut out.limbs);
        let mut acc: Elem<M, R> = Elem {
            limbs: out.limbs,
            encoding: PhantomData,
        };

        
        
        
        
        
        
        
        
        
        
        let t = w;
        let z = w.trailing_zeros();
        let d = w >> z;
        debug_assert_eq!(w, d * (1 << z));
        debug_assert!(d <= t);
        debug_assert!(t < r);
        for _ in 0..t {
            elem_double(&mut acc, m);
        }

        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        const B: u32 = if cfg!(target_pointer_width = "64") {
            6
        } else if cfg!(target_pointer_width = "32") {
            5
        } else {
            panic!("unsupported target_pointer_width")
        };
        #[allow(clippy::assertions_on_constants)]
        const _LIMB_BITS_IS_2_POW_B: () = assert!(LIMB_BITS == 1 << B);
        debug_assert_eq!(r, t * (1 << B));
        for _ in 0..B {
            acc = elem_squared(acc, m);
        }

        Self(Elem {
            limbs: acc.limbs,
            encoding: PhantomData, 
        })
    }
}

impl<M> One<M, RRR> {
    pub(crate) fn newRRR(One(oneRR): One<M, RR>, m: &Modulus<M>) -> Self {
        Self(elem_squared(oneRR, m))
    }
}

impl<M, E> AsRef<Elem<M, E>> for One<M, E> {
    fn as_ref(&self) -> &Elem<M, E> {
        &self.0
    }
}

impl<M: PublicModulus, E> One<M, E> {
    pub fn clone_into(&self, out: Storage<M>) -> Self {
        Self(self.0.clone_into(out))
    }
}










pub(crate) fn elem_exp_vartime<M>(
    out: Storage<M>,
    base: Elem<M, R>,
    exponent: NonZeroU64,
    m: &Modulus<M>,
) -> Elem<M, R> {
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    let exponent = exponent.get();
    let mut acc = base.clone_into(out);
    let mut bit = 1 << (64 - 1 - exponent.leading_zeros());
    debug_assert!((exponent & bit) != 0);
    while bit > 1 {
        bit >>= 1;
        acc = elem_squared(acc, m);
        if (exponent & bit) != 0 {
            acc = elem_mul(&base, acc, m);
        }
    }
    acc
}

pub fn elem_exp_consttime<N, P>(
    out: Storage<P>,
    base: &Elem<N>,
    oneRRR: &One<P, RRR>,
    exponent: &PrivateExponent,
    p: &Modulus<P>,
    other_prime_len_bits: BitLength,
) -> Result<Elem<P, Unencoded>, LimbSliceError> {
    
    
    elem_exp_consttime_inner::<N, P, { ELEM_EXP_CONSTTIME_MAX_MODULUS_LIMBS * STORAGE_ENTRIES }>(
        out,
        base,
        oneRRR,
        exponent,
        p,
        other_prime_len_bits,
    )
}



const ELEM_EXP_CONSTTIME_MAX_MODULUS_LIMBS: usize = 2048 / LIMB_BITS;
const _LIMBS_PER_CHUNK_DIVIDES_ELEM_EXP_CONSTTIME_MAX_MODULUS_LIMBS: () =
    assert!(ELEM_EXP_CONSTTIME_MAX_MODULUS_LIMBS % limbs512::LIMBS_PER_CHUNK == 0);
const WINDOW_BITS: u32 = 5;
const TABLE_ENTRIES: usize = 1 << WINDOW_BITS;
const STORAGE_ENTRIES: usize = TABLE_ENTRIES + if cfg!(target_arch = "x86_64") { 3 } else { 0 };

#[cfg(not(target_arch = "x86_64"))]
fn elem_exp_consttime_inner<N, M, const STORAGE_LIMBS: usize>(
    out: Storage<M>,
    base_mod_n: &Elem<N>,
    oneRRR: &One<M, RRR>,
    exponent: &PrivateExponent,
    m: &Modulus<M>,
    other_prime_len_bits: BitLength,
) -> Result<Elem<M, Unencoded>, LimbSliceError> {
    use crate::{bssl, limb::Window};

    let base_rinverse: Elem<M, RInverse> = elem_reduced(out, base_mod_n, m, other_prime_len_bits);

    let num_limbs = m.limbs().len();
    let m_chunked: AsChunks<Limb, { limbs512::LIMBS_PER_CHUNK }> = match slice::as_chunks(m.limbs())
    {
        (m, []) => m,
        _ => {
            return Err(LimbSliceError::len_mismatch(LenMismatchError::new(
                num_limbs,
            )))
        }
    };
    let cpe = m_chunked.len(); 

    
    
    assert!(STORAGE_LIMBS % (STORAGE_ENTRIES * limbs512::LIMBS_PER_CHUNK) == 0); 
    let mut table = limbs512::AlignedStorage::<STORAGE_LIMBS>::zeroed();
    let mut table = table
        .aligned_chunks_mut(TABLE_ENTRIES, cpe)
        .map_err(LimbSliceError::len_mismatch)?;

    
    let table = table.as_flattened_mut();

    fn gather<M>(table: &[Limb], acc: &mut Elem<M, R>, i: Window) {
        prefixed_extern! {
            fn LIMBS_select_512_32(
                r: *mut Limb,
                table: *const Limb,
                num_limbs: c::size_t,
                i: Window,
            ) -> bssl::Result;
        }
        Result::from(unsafe {
            LIMBS_select_512_32(acc.limbs.as_mut_ptr(), table.as_ptr(), acc.limbs.len(), i)
        })
        .unwrap();
    }

    fn power<M>(
        table: &[Limb],
        mut acc: Elem<M, R>,
        m: &Modulus<M>,
        i: Window,
        mut tmp: Elem<M, R>,
    ) -> (Elem<M, R>, Elem<M, R>) {
        for _ in 0..WINDOW_BITS {
            acc = elem_squared(acc, m);
        }
        gather(table, &mut tmp, i);
        let acc = elem_mul(&tmp, acc, m);
        (acc, tmp)
    }

    fn entry(table: &[Limb], i: usize, num_limbs: usize) -> &[Limb] {
        &table[(i * num_limbs)..][..num_limbs]
    }
    fn entry_mut(table: &mut [Limb], i: usize, num_limbs: usize) -> &mut [Limb] {
        &mut table[(i * num_limbs)..][..num_limbs]
    }

    
    m.oneR(entry_mut(table, 0, num_limbs));

    
    limbs_mul_mont(
        (
            entry_mut(table, 1, num_limbs),
            base_rinverse.limbs.as_ref(),
            oneRRR.as_ref().limbs.as_ref(),
        ),
        m.limbs(),
        m.n0(),
        m.cpu_features(),
    )?;
    for i in 2..TABLE_ENTRIES {
        let (src1, src2) = if i % 2 == 0 {
            (i / 2, i / 2)
        } else {
            (i - 1, 1)
        };
        let (previous, rest) = table.split_at_mut(num_limbs * i);
        let src1 = entry(previous, src1, num_limbs);
        let src2 = entry(previous, src2, num_limbs);
        let dst = entry_mut(rest, 0, num_limbs);
        limbs_mul_mont((dst, src1, src2), m.limbs(), m.n0(), m.cpu_features())?;
    }

    let mut acc = Elem {
        limbs: base_rinverse.limbs,
        encoding: PhantomData,
    };
    let tmp = m.alloc_zero();
    let tmp = Elem {
        limbs: tmp.limbs,
        encoding: PhantomData,
    };
    let (acc, _) = limb::fold_5_bit_windows(
        exponent.limbs(),
        |initial_window| {
            gather(&table, &mut acc, initial_window);
            (acc, tmp)
        },
        |(acc, tmp), window| power(&table, acc, m, window, tmp),
    );

    Ok(acc.into_unencoded(m))
}

#[cfg(target_arch = "x86_64")]
fn elem_exp_consttime_inner<N, M, const STORAGE_LIMBS: usize>(
    out: Storage<M>,
    base_mod_n: &Elem<N>,
    oneRRR: &One<M, RRR>,
    exponent: &PrivateExponent,
    m: &Modulus<M>,
    other_prime_len_bits: BitLength,
) -> Result<Elem<M, Unencoded>, LimbSliceError> {
    use super::limbs::x86_64::mont::{
        gather5, mul_mont5, mul_mont_gather5_amm, power5_amm, scatter5, sqr_mont5,
    };
    use crate::{
        cpu::{
            intel::{Adx, Bmi2},
            GetFeature as _,
        },
        limb::{LeakyWindow, Window},
        polyfill::slice::AsChunksMut,
    };

    let n0 = m.n0();

    let cpu2 = m.cpu_features().get_feature();
    let cpu3 = m.cpu_features().get_feature();

    if base_mod_n.limbs.len() != m.limbs().len() * 2 {
        return Err(LimbSliceError::len_mismatch(LenMismatchError::new(
            base_mod_n.limbs.len(),
        )));
    }

    let m_original: AsChunks<Limb, 8> = match slice::as_chunks(m.limbs()) {
        (m, []) => m,
        _ => return Err(LimbSliceError::len_mismatch(LenMismatchError::new(8))),
    };
    let cpe = m_original.len(); 

    let oneRRR = &oneRRR.as_ref().limbs;
    let oneRRR = match slice::as_chunks(oneRRR) {
        (c, []) => c,
        _ => {
            return Err(LimbSliceError::len_mismatch(LenMismatchError::new(
                oneRRR.len(),
            )))
        }
    };

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    const MOD_EXP_CTIME_ALIGN: usize = 64;
    
    const _TABLE_ENTRIES_IS_32: () = assert!(TABLE_ENTRIES == 32);
    const _STORAGE_ENTRIES_HAS_3_EXTRA: () = assert!(STORAGE_ENTRIES == TABLE_ENTRIES + 3);

    assert!(STORAGE_LIMBS % (STORAGE_ENTRIES * limbs512::LIMBS_PER_CHUNK) == 0); 
    let mut table = limbs512::AlignedStorage::<STORAGE_LIMBS>::zeroed();
    let mut table = table
        .aligned_chunks_mut(STORAGE_ENTRIES, cpe)
        .map_err(LimbSliceError::len_mismatch)?;
    let (mut table, mut state) = table.split_at_mut(TABLE_ENTRIES * cpe);
    assert_eq!((table.as_ptr() as usize) % MOD_EXP_CTIME_ALIGN, 0);

    
    let (mut acc, mut rest) = state.split_at_mut(cpe);
    let (mut base_cached, mut m_cached) = rest.split_at_mut(cpe);

    
    m_cached
        .as_flattened_mut()
        .copy_from_slice(m_original.as_flattened());
    let m_cached = m_cached.as_ref();

    let out: Elem<M, RInverse> = elem_reduced(out, base_mod_n, m, other_prime_len_bits);
    let base_rinverse = match slice::as_chunks(&out.limbs) {
        (c, []) => c,
        _ => {
            return Err(LimbSliceError::len_mismatch(LenMismatchError::new(
                out.limbs.len(),
            )))
        }
    };

    
    mul_mont5(
        base_cached.as_mut(),
        base_rinverse,
        oneRRR,
        m_cached,
        n0,
        cpu2,
    )?;
    let base_cached = base_cached.as_ref();
    let mut out = Storage::from(out); 

    
    
    fn scatter_powers_of_2(
        mut table: AsChunksMut<Limb, 8>,
        mut acc: AsChunksMut<Limb, 8>,
        m_cached: AsChunks<Limb, 8>,
        n0: &N0,
        mut i: LeakyWindow,
        cpu: Option<(Adx, Bmi2)>,
    ) -> Result<(), LimbSliceError> {
        loop {
            scatter5(acc.as_ref(), table.as_mut(), i)?;
            i *= 2;
            if i >= TABLE_ENTRIES as LeakyWindow {
                break;
            }
            sqr_mont5(acc.as_mut(), m_cached, n0, cpu)?;
        }
        Ok(())
    }

    

    
    m.oneR(acc.as_flattened_mut());
    scatter5(acc.as_ref(), table.as_mut(), 0)?;

    
    acc.as_flattened_mut()
        .copy_from_slice(base_cached.as_flattened());

    
    scatter_powers_of_2(table.as_mut(), acc.as_mut(), m_cached, n0, 1, cpu2)?;
    
    
    for i in (3..(TABLE_ENTRIES as LeakyWindow)).step_by(2) {
        let power = Window::from(i - 1);
        assert!(power < 32); 
        unsafe {
            mul_mont_gather5_amm(
                acc.as_mut(),
                base_cached,
                table.as_ref(),
                m_cached,
                n0,
                power,
                cpu3,
            )
        }?;
        scatter_powers_of_2(table.as_mut(), acc.as_mut(), m_cached, n0, i, cpu2)?;
    }

    let table = table.as_ref();

    let acc = limb::fold_5_bit_windows(
        exponent.limbs(),
        |initial_window| {
            unsafe { gather5(acc.as_mut(), table, initial_window) }
                .unwrap_or_else(unwrap_impossible_limb_slice_error);
            acc
        },
        |mut acc, window| {
            unsafe { power5_amm(acc.as_mut(), table, m_cached, n0, window, cpu3) }
                .unwrap_or_else(unwrap_impossible_limb_slice_error);
            acc
        },
    );

    
    out.limbs.copy_from_slice(acc.as_flattened());
    Ok(from_montgomery_amm(out, m))
}


pub fn verify_inverses_consttime<M>(
    a: &Elem<M, R>,
    b: Elem<M, Unencoded>,
    m: &Modulus<M>,
) -> Result<(), error::Unspecified> {
    let r = elem_mul(a, b, m);
    limb::verify_limbs_equal_1_leak_bit(&r.limbs)
}

#[inline]
pub fn elem_verify_equal_consttime<M, E>(
    a: &Elem<M, E>,
    b: &Elem<M, E>,
) -> Result<(), error::Unspecified> {
    let equal = limb::limbs_equal_limbs_consttime(&a.limbs, &b.limbs)
        .unwrap_or_else(unwrap_impossible_len_mismatch_error);
    if !equal.leak() {
        return Err(error::Unspecified);
    }
    Ok(())
}

#[cold]
#[inline(never)]
fn unwrap_impossible_len_mismatch_error<T>(LenMismatchError { .. }: LenMismatchError) -> T {
    unreachable!()
}

#[cold]
#[inline(never)]
fn unwrap_impossible_limb_slice_error(err: LimbSliceError) {
    match err {
        LimbSliceError::LenMismatch(_) => unreachable!(),
        LimbSliceError::TooShort(_) => unreachable!(),
        LimbSliceError::TooLong(_) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu;
    use crate::testutil as test;

    
    struct M {}

    impl PublicModulus for M {}

    #[test]
    fn test_elem_exp_consttime() {
        let cpu_features = cpu::features();
        test::run(
            test_vector_file!("../../crypto/fipsmodule/bn/test/mod_exp_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                let m = consume_modulus::<M>(test_case, "M");
                let m = m.modulus(cpu_features);
                let expected_result = consume_elem(test_case, "ModExp", &m);
                let base = consume_elem(test_case, "A", &m);
                let e = {
                    let bytes = test_case.consume_bytes("E");
                    PrivateExponent::from_be_bytes_for_test_only(untrusted::Input::from(&bytes), &m)
                        .expect("valid exponent")
                };

                let oneRR = One::newRR(m.alloc_zero(), &m);
                let oneRRR = One::newRRR(oneRR, &m);

                
                
                
                
                struct N {}
                let other_modulus_len_bits = m.len_bits();
                let base: Elem<N> = {
                    let mut limbs = BoxedLimbs::zero(base.limbs.len() * 2);
                    limbs[..base.limbs.len()].copy_from_slice(&base.limbs);
                    Elem {
                        limbs,
                        encoding: PhantomData,
                    }
                };

                let too_big = m.limbs().len() > ELEM_EXP_CONSTTIME_MAX_MODULUS_LIMBS;
                let actual_result = if !too_big {
                    elem_exp_consttime(
                        m.alloc_zero(),
                        &base,
                        &oneRRR,
                        &e,
                        &m,
                        other_modulus_len_bits,
                    )
                } else {
                    let actual_result = elem_exp_consttime(
                        m.alloc_zero(),
                        &base,
                        &oneRRR,
                        &e,
                        &m,
                        other_modulus_len_bits,
                    );
                    
                    assert!(actual_result.is_err());
                    
                    elem_exp_consttime_inner::<_, _, { (4096 / LIMB_BITS) * STORAGE_ENTRIES }>(
                        m.alloc_zero(),
                        &base,
                        &oneRRR,
                        &e,
                        &m,
                        other_modulus_len_bits,
                    )
                };
                match actual_result {
                    Ok(r) => assert_elem_eq(&r, &expected_result),
                    Err(LimbSliceError::LenMismatch { .. }) => panic!(),
                    Err(LimbSliceError::TooLong { .. }) => panic!(),
                    Err(LimbSliceError::TooShort { .. }) => panic!(),
                };

                Ok(())
            },
        )
    }

    
    
    
    
    #[test]
    fn test_elem_mul() {
        let cpu_features = cpu::features();
        test::run(
            test_vector_file!("../../crypto/fipsmodule/bn/test/mod_mul_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                let m = consume_modulus::<M>(test_case, "M");
                let m = m.modulus(cpu_features);
                let expected_result = consume_elem(test_case, "ModMul", &m);
                let a = consume_elem(test_case, "A", &m);
                let b = consume_elem(test_case, "B", &m);

                let b = into_encoded(m.alloc_zero(), b, &m);
                let a = into_encoded(m.alloc_zero(), a, &m);
                let actual_result = elem_mul(&a, b, &m);
                let actual_result = actual_result.into_unencoded(&m);
                assert_elem_eq(&actual_result, &expected_result);

                Ok(())
            },
        )
    }

    #[test]
    fn test_elem_squared() {
        let cpu_features = cpu::features();
        test::run(
            test_vector_file!("bigint_elem_squared_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                let m = consume_modulus::<M>(test_case, "M");
                let m = m.modulus(cpu_features);
                let expected_result = consume_elem(test_case, "ModSquare", &m);
                let a = consume_elem(test_case, "A", &m);

                let a = into_encoded(m.alloc_zero(), a, &m);
                let actual_result = elem_squared(a, &m);
                let actual_result = actual_result.into_unencoded(&m);
                assert_elem_eq(&actual_result, &expected_result);

                Ok(())
            },
        )
    }

    #[test]
    fn test_elem_reduced() {
        let cpu_features = cpu::features();
        test::run(
            test_vector_file!("bigint_elem_reduced_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                struct M {}

                let m_ = consume_modulus::<M>(test_case, "M");
                let m = m_.modulus(cpu_features);
                let expected_result = consume_elem(test_case, "R", &m);
                let a =
                    consume_elem_unchecked::<M>(test_case, "A", expected_result.limbs.len() * 2);
                let other_modulus_len_bits = m_.len_bits();

                let actual_result = elem_reduced(m.alloc_zero(), &a, &m, other_modulus_len_bits);
                let oneRR = One::newRR(m.alloc_zero(), &m);
                let actual_result = elem_mul(oneRR.as_ref(), actual_result, &m);
                assert_elem_eq(&actual_result, &expected_result);

                Ok(())
            },
        )
    }

    #[test]
    fn test_elem_reduced_once() {
        let cpu_features = cpu::features();
        test::run(
            test_vector_file!("bigint_elem_reduced_once_tests.txt"),
            |section, test_case| {
                assert_eq!(section, "");

                struct M {}
                struct O {}
                let m = consume_modulus::<M>(test_case, "m");
                let m = m.modulus(cpu_features);
                let a = consume_elem_unchecked::<O>(test_case, "a", m.limbs().len());
                let expected_result = consume_elem::<M>(test_case, "r", &m);
                let other_modulus_len_bits = m.len_bits();

                let actual_result =
                    elem_reduced_once(m.alloc_zero(), &a, &m, other_modulus_len_bits);
                assert_elem_eq(&actual_result, &expected_result);

                Ok(())
            },
        )
    }

    fn consume_elem<M>(
        test_case: &mut test::TestCase,
        name: &str,
        m: &Modulus<M>,
    ) -> Elem<M, Unencoded> {
        let value = test_case.consume_bytes(name);
        Elem::from_be_bytes_padded(untrusted::Input::from(&value), m).unwrap()
    }

    fn consume_elem_unchecked<M>(
        test_case: &mut test::TestCase,
        name: &str,
        num_limbs: usize,
    ) -> Elem<M, Unencoded> {
        let bytes = test_case.consume_bytes(name);
        let mut limbs = BoxedLimbs::zero(num_limbs);
        limb::parse_big_endian_and_pad_consttime(untrusted::Input::from(&bytes), &mut limbs)
            .unwrap();
        Elem {
            limbs,
            encoding: PhantomData,
        }
    }

    fn consume_modulus<M>(test_case: &mut test::TestCase, name: &str) -> OwnedModulus<M> {
        let value = test_case.consume_bytes(name);
        OwnedModulus::from(
            OwnedModulusValue::from_be_bytes(untrusted::Input::from(&value)).unwrap(),
        )
    }

    fn assert_elem_eq<M, E>(a: &Elem<M, E>, b: &Elem<M, E>) {
        if elem_verify_equal_consttime(a, b).is_err() {
            panic!("{:x?} != {:x?}", &*a.limbs, &*b.limbs);
        }
    }

    fn into_encoded<M>(out: Storage<M>, a: Elem<M, Unencoded>, m: &Modulus<M>) -> Elem<M, R> {
        let oneRR = One::newRR(out, m);
        elem_mul(oneRR.as_ref(), a, m)
    }
}

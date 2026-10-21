#include "../../limbs/limbs.h"

#include "ecp_nistz384.h"
#include "../bn/internal.h"
#include "../../internal.h"

#include "../../limbs/limbs.inl"

typedef Limb Elem[P384_LIMBS];
typedef Limb ScalarMont[P384_LIMBS];
typedef Limb Scalar[P384_LIMBS];

static const BN_ULONG Q[P384_LIMBS] = {
#if defined(OPENSSL_64_BIT)
  0xffffffff, 0xffffffff00000000, 0xfffffffffffffffe, 0xffffffffffffffff,
  0xffffffffffffffff, 0xffffffffffffffff
#else
  0xffffffff, 0, 0, 0xffffffff, 0xfffffffe, 0xffffffff, 0xffffffff, 0xffffffff,
  0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff
#endif
};

static const BN_ULONG N[P384_LIMBS] = {
#if defined(OPENSSL_64_BIT)
  0xecec196accc52973, 0x581a0db248b0a77a, 0xc7634d81f4372ddf, 0xffffffffffffffff,
  0xffffffffffffffff, 0xffffffffffffffff
#else
  0xccc52973, 0xecec196a, 0x48b0a77a, 0x581a0db2, 0xf4372ddf, 0xc7634d81,
  0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff
#endif
};

static const BN_ULONG ONE[P384_LIMBS] = {
#if defined(OPENSSL_64_BIT)
  0xffffffff00000001, 0xffffffff, 1, 0, 0
#else
  1, 0xffffffff, 0xffffffff, 0, 1, 0, 0, 0, 0, 0
#endif
};

static const Elem Q_PLUS_1_SHR_1 = {
#if defined(OPENSSL_64_BIT)
  0x80000000, 0x7fffffff80000000, 0xffffffffffffffff, 0xffffffffffffffff,
  0xffffffffffffffff, 0x7fffffffffffffff
#else
  0x80000000, 0, 0x80000000, 0x7fffffff, 0xffffffff, 0xffffffff, 0xffffffff,
  0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0x7fffffff
#endif
};

static const BN_ULONG Q_N0[] = {
  BN_MONT_CTX_N0(1, 1)
};

static const BN_ULONG N_N0[] = {
  BN_MONT_CTX_N0(0x6ed46089, 0xe88fdc45)
};

#if defined(_MSC_VER) && !defined(__clang__) && defined(OPENSSL_X86)
#define INLINE_IF_POSSIBLE __forceinline
#else
#define INLINE_IF_POSSIBLE inline
#endif

static inline Limb is_equal(const Elem a, const Elem b) {
  return LIMBS_equal(a, b, P384_LIMBS);
}

static inline Limb is_zero(const BN_ULONG a[P384_LIMBS]) {
  return LIMBS_are_zero(a, P384_LIMBS);
}

static inline void copy_conditional(Elem r, const Elem a,
                                                const Limb condition) {
  for (size_t i = 0; i < P384_LIMBS; ++i) {
    r[i] = constant_time_select_w(condition, a[i], r[i]);
  }
}

static inline void elem_add(Elem r, const Elem a, const Elem b) {
  LIMBS_add_mod(r, a, b, Q, P384_LIMBS);
}

static inline void elem_sub(Elem r, const Elem a, const Elem b) {
  LIMBS_sub_mod(r, a, b, Q, P384_LIMBS);
}

static void elem_div_by_2(Elem r, const Elem a) {

  Limb is_odd = constant_time_is_nonzero_w(a[0] & 1);

  Limb carry = a[P384_LIMBS - 1] & 1;
  r[P384_LIMBS - 1] = a[P384_LIMBS - 1] >> 1;
  for (size_t i = 1; i < P384_LIMBS; ++i) {
    Limb new_carry = a[P384_LIMBS - i - 1];
    r[P384_LIMBS - i - 1] =
        (a[P384_LIMBS - i - 1] >> 1) | (carry << (LIMB_BITS - 1));
    carry = new_carry;
  }

  Elem adjusted;
  BN_ULONG carry2 = limbs_add(adjusted, r, Q_PLUS_1_SHR_1, P384_LIMBS);
  dev_assert_secret(carry2 == 0);
  (void)carry2;
  copy_conditional(r, adjusted, is_odd);
}

static inline void elem_mul_mont(Elem r, const Elem a, const Elem b) {

  bn_mul_mont_small(r, a, b, Q, Q_N0, P384_LIMBS);
}

static inline void elem_mul_by_2(Elem r, const Elem a) {
  LIMBS_shl_mod(r, a, Q, P384_LIMBS);
}

static INLINE_IF_POSSIBLE void elem_mul_by_3(Elem r, const Elem a) {

  Elem doubled;
  elem_add(doubled, a, a);
  elem_add(r, doubled, a);
}

static inline void elem_sqr_mont(Elem r, const Elem a) {

  elem_mul_mont(r, a, a);
}

void p384_elem_sub(Elem r, const Elem a, const Elem b) {
  elem_sub(r, a, b);
}

void p384_elem_div_by_2(Elem r, const Elem a) {
  elem_div_by_2(r, a);
}

void p384_elem_mul_mont(Elem r, const Elem a, const Elem b) {
  elem_mul_mont(r, a, b);
}

void p384_elem_neg(Elem r, const Elem a) {
  Limb is_zero = LIMBS_are_zero(a, P384_LIMBS);
  Carry borrow = limbs_sub(r, Q, a, P384_LIMBS);
  dev_assert_secret(borrow == 0);
  (void)borrow;
  for (size_t i = 0; i < P384_LIMBS; ++i) {
    r[i] = constant_time_select_w(is_zero, 0, r[i]);
  }
}

void p384_scalar_mul_mont(ScalarMont r, const ScalarMont a,
                              const ScalarMont b) {

  bn_mul_mont_small(r, a, b, N, N_N0, P384_LIMBS);
}

static void p384_point_select_w5(P384_POINT *out,
                                     const P384_POINT table[16], size_t index) {
  Elem x; limbs_zero(x, P384_LIMBS);
  Elem y; limbs_zero(y, P384_LIMBS);
  Elem z; limbs_zero(z, P384_LIMBS);

  for (size_t i = 0; i < 16; ++i) {
    crypto_word_t equal = constant_time_eq_w(index, (crypto_word_t)i + 1);
    for (size_t j = 0; j < P384_LIMBS; ++j) {
      x[j] = constant_time_select_w(equal, table[i].X[j], x[j]);
      y[j] = constant_time_select_w(equal, table[i].Y[j], y[j]);
      z[j] = constant_time_select_w(equal, table[i].Z[j], z[j]);
    }
  }

  limbs_copy(out->X, x, P384_LIMBS);
  limbs_copy(out->Y, y, P384_LIMBS);
  limbs_copy(out->Z, z, P384_LIMBS);
}

#include "ecp_nistz384.inl"

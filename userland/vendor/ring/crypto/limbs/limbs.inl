#include "limbs.h"
#include "ring-core/check.h"

#if defined(_MSC_VER) && !defined(__clang__)
#pragma warning(push, 3)
#include <intrin.h>
#pragma warning(pop)

#if _MSC_FULL_VER < 190023918
#error "MSVC 2015 Update 2 or later is required."
#endif
typedef uint8_t Carry;
#if LIMB_BITS == 64
#pragma intrinsic(_addcarry_u64, _subborrow_u64)
#define RING_CORE_ADDCARRY_INTRINSIC _addcarry_u64
#define RING_CORE_SUBBORROW_INTRINSIC _subborrow_u64
#elif LIMB_BITS == 32
#pragma intrinsic(_addcarry_u32, _subborrow_u32)
#define RING_CORE_ADDCARRY_INTRINSIC _addcarry_u32
#define RING_CORE_SUBBORROW_INTRINSIC _subborrow_u32
typedef uint64_t DoubleLimb;
#endif
#else
typedef Limb Carry;
#if LIMB_BITS == 64
typedef __uint128_t DoubleLimb;
#elif LIMB_BITS == 32
typedef uint64_t DoubleLimb;
#endif
#endif

static inline Carry limb_adc(Limb *r, Limb a, Limb b, Carry carry_in) {
  dev_assert_secret(carry_in == 0 || carry_in == 1);
  Carry ret;
#if defined(RING_CORE_ADDCARRY_INTRINSIC)
  ret = RING_CORE_ADDCARRY_INTRINSIC(carry_in, a, b, r);
#else
  DoubleLimb x = (DoubleLimb)a + b + carry_in;
  *r = (Limb)x;
  ret = (Carry)(x >> LIMB_BITS);
#endif
  dev_assert_secret(ret == 0 || ret == 1);
  return ret;
}

static inline Carry limb_add(Limb *r, Limb a, Limb b) {
  Carry ret;
#if defined(RING_CORE_ADDCARRY_INTRINSIC)
  ret = RING_CORE_ADDCARRY_INTRINSIC(0, a, b, r);
#else
  DoubleLimb x = (DoubleLimb)a + b;
  *r = (Limb)x;
  ret = (Carry)(x >> LIMB_BITS);
#endif
  dev_assert_secret(ret == 0 || ret == 1);
  return ret;
}

static inline Carry limb_sbb(Limb *r, Limb a, Limb b, Carry borrow_in) {
  dev_assert_secret(borrow_in == 0 || borrow_in == 1);
  Carry ret;
#if defined(RING_CORE_SUBBORROW_INTRINSIC)
  ret = RING_CORE_SUBBORROW_INTRINSIC(borrow_in, a, b, r);
#else
  DoubleLimb x = (DoubleLimb)a - b - borrow_in;
  *r = (Limb)x;
  ret = (Carry)((x >> LIMB_BITS) & 1);
#endif
  dev_assert_secret(ret == 0 || ret == 1);
  return ret;
}

static inline Carry limb_sub(Limb *r, Limb a, Limb b) {
  Carry ret;
#if defined(RING_CORE_SUBBORROW_INTRINSIC)
  ret = RING_CORE_SUBBORROW_INTRINSIC(0, a, b, r);
#else
  DoubleLimb x = (DoubleLimb)a - b;
  *r = (Limb)x;
  ret = (Carry)((x >> LIMB_BITS) & 1);
#endif
  dev_assert_secret(ret == 0 || ret == 1);
  return ret;
}

static inline Carry limbs_add(Limb r[], const Limb a[], const Limb b[],
                              size_t num_limbs) {
  debug_assert_nonsecret(num_limbs >= 1);
  Carry carry = limb_add(&r[0], a[0], b[0]);
  for (size_t i = 1; i < num_limbs; ++i) {
    carry = limb_adc(&r[i], a[i], b[i], carry);
  }
  return carry;
}

static inline Carry limbs_sub(Limb r[], const Limb a[], const Limb b[],
                              size_t num_limbs) {
  debug_assert_nonsecret(num_limbs >= 1);
  Carry borrow = limb_sub(&r[0], a[0], b[0]);
  for (size_t i = 1; i < num_limbs; ++i) {
    borrow = limb_sbb(&r[i], a[i], b[i], borrow);
  }
  return borrow;
}

static inline void limbs_copy(Limb r[], const Limb a[], size_t num_limbs) {
  for (size_t i = 0; i < num_limbs; ++i) {
    r[i] = a[i];
  }
}

static inline void limbs_select(Limb r[], const Limb table[],
                                size_t num_limbs, size_t num_entries,
                                crypto_word_t index) {
  for (size_t i = 0; i < num_limbs; ++i) {
    r[i] = 0;
  }

  for (size_t e = 0; e < num_entries; ++e) {
    Limb equal = constant_time_eq_w(index, e);
    for (size_t i = 0; i < num_limbs; ++i) {
      r[i] = constant_time_select_w(equal, table[(e * num_limbs) + i], r[i]);
    }
  }
}

static inline void limbs_zero(Limb r[], size_t num_limbs) {
  for (size_t i = 0; i < num_limbs; ++i) {
    r[i] = 0;
  }
}

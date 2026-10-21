













#include "limbs.h"

#include "../internal.h"
#include "../fipsmodule/bn/internal.h"
#include "limbs.inl"







Limb LIMB_is_zero(const Limb a) {
  return constant_time_is_zero_w(a);
}



Limb LIMBS_are_zero(const Limb a[], size_t num_limbs) {
  Limb all = 0;
  for (size_t i = 0; i < num_limbs; ++i) {
    all |= a[i];
  }
  return LIMB_is_zero(all);
}


Limb LIMBS_equal(const Limb a[], const Limb b[], size_t num_limbs) {
  Limb eq = CONSTTIME_TRUE_W;
  for (size_t i = 0; i < num_limbs; ++i) {
    eq = constant_time_select_w(eq, constant_time_eq_w(a[i], b[i]), eq);
  }
  return eq;
}


Limb LIMBS_less_than(const Limb a[], const Limb b[], size_t num_limbs) {
  debug_assert_nonsecret(num_limbs >= 1);
  


  Limb dummy;
  Carry borrow = limb_sub(&dummy, a[0], b[0]);
  for (size_t i = 1; i < num_limbs; ++i) {
    borrow = limb_sbb(&dummy, a[i], b[i], borrow);
  }
  return constant_time_is_nonzero_w(borrow);
}


void LIMBS_reduce_once(Limb r[], const Limb m[], size_t num_limbs) {
  debug_assert_nonsecret(num_limbs >= 1);
  




  Limb lt = LIMBS_less_than(r, m, num_limbs);
  Carry borrow =
      limb_sub(&r[0], r[0], constant_time_select_w(lt, 0, m[0]));
  for (size_t i = 1; i < num_limbs; ++i) {
    


    borrow =
        limb_sbb(&r[i], r[i], constant_time_select_w(lt, 0, m[i]), borrow);
  }
  dev_assert_secret(borrow == 0);
}

void LIMBS_add_mod(Limb r[], const Limb a[], const Limb b[], const Limb m[],
                   size_t num_limbs) {
  Limb overflow1 =
      constant_time_is_nonzero_w(limbs_add(r, a, b, num_limbs));
  Limb overflow2 = ~LIMBS_less_than(r, m, num_limbs);
  Limb overflow = overflow1 | overflow2;
  Carry borrow = limb_sub(&r[0], r[0], m[0] & overflow);
  for (size_t i = 1; i < num_limbs; ++i) {
    borrow = limb_sbb(&r[i], r[i], m[i] & overflow, borrow);
  }
}

void LIMBS_sub_mod(Limb r[], const Limb a[], const Limb b[], const Limb m[],
                   size_t num_limbs) {
  Limb underflow =
      constant_time_is_nonzero_w(limbs_sub(r, a, b, num_limbs));
  Carry carry = limb_add(&r[0], r[0], m[0] & underflow);
  for (size_t i = 1; i < num_limbs; ++i) {
    carry = limb_adc(&r[i], r[i], m[i] & underflow, carry);
  }
}

void LIMBS_shl_mod(Limb r[], const Limb a[], const Limb m[], size_t num_limbs) {
  Limb overflow1 =
      constant_time_is_nonzero_w(a[num_limbs - 1] & LIMB_HIGH_BIT);
  Limb carry = 0;
  for (size_t i = 0; i < num_limbs; ++i) {
    Limb limb = a[i];
    Limb new_carry = limb >> (LIMB_BITS - 1);
    r[i] = (limb << 1) | carry;
    carry = new_carry;
  }
  Limb overflow2 = ~LIMBS_less_than(r, m, num_limbs);
  Limb overflow = overflow1 | overflow2;
  Carry borrow = limb_sub(&r[0], r[0], m[0] & overflow);
  for (size_t i = 1; i < num_limbs; ++i) {
    borrow = limb_sbb(&r[i], r[i], m[i] & overflow, borrow);
  }
}

int LIMBS_select_512_32(Limb r[], const Limb table[], size_t num_limbs,
                        crypto_word_t index) {
  if (num_limbs % (512 / LIMB_BITS) != 0) {
    return 0;
  }
  limbs_select(r, table, num_limbs, 32, index);
  return 1;
}

static const Limb FIVE_BITS_MASK = 0x1f;

crypto_word_t LIMBS_window5_split_window(Limb lower_limb, Limb higher_limb, size_t index_within_word) {
  Limb high_bits = (higher_limb << (LIMB_BITS - index_within_word))
    & FIVE_BITS_MASK;
  
  
  
  Limb low_bits = lower_limb >> index_within_word;
  return low_bits | high_bits;
}

crypto_word_t LIMBS_window5_unsplit_window(Limb limb, size_t index_within_word) {
  return (limb >> index_within_word) & FIVE_BITS_MASK;
}

Limb LIMB_shr(Limb a, size_t shift) {
  return a >> shift;
}

Limb limbs_mul_add_limb(Limb r[], const Limb a[], Limb b, size_t num_limbs) {
  Limb carried = 0;
  for (size_t i = 0; i < num_limbs; ++i) {
    Limb lo;
    Limb hi;
    bn_umult_lohi(&lo, &hi, a[i], b);
    Limb tmp;
    Carry c = limb_add(&tmp, lo, carried);
    c = limb_adc(&carried, hi, 0, c);
    dev_assert_secret(c == 0);
    c = limb_add(&r[i], r[i], tmp);
    c = limb_adc(&carried, carried, 0, c);
    
    dev_assert_secret(c == 0);
  }
  return carried;
}

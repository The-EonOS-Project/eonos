#include "internal.h"
#include "../../internal.h"

#include "../../limbs/limbs.h"
#include "../../limbs/limbs.inl"

OPENSSL_STATIC_ASSERT(BN_MONT_CTX_N0_LIMBS == 1 || BN_MONT_CTX_N0_LIMBS == 2,
  "BN_MONT_CTX_N0_LIMBS value is invalid");
OPENSSL_STATIC_ASSERT(
  sizeof(BN_ULONG) * BN_MONT_CTX_N0_LIMBS == sizeof(uint64_t),
  "uint64_t is insufficient precision for n0");

int bn_from_montgomery_in_place(BN_ULONG r[], size_t num_r, BN_ULONG a[],
                                    size_t num_a, const BN_ULONG n[],
                                    size_t num_n,
                                    const BN_ULONG n0_[BN_MONT_CTX_N0_LIMBS]) {
  if (num_n == 0 || num_r != num_n || num_a != 2 * num_n) {
    return 0;
  }

  BN_ULONG n0 = n0_[0];
  BN_ULONG carry = 0;
  for (size_t i = 0; i < num_n; i++) {
    BN_ULONG v = limbs_mul_add_limb(a + i, n, a[i] * n0, num_n);
    v += carry + a[i + num_n];
    carry |= (v != a[i + num_n]);
    carry &= (v <= a[i + num_n]);
    a[i + num_n] = v;
  }

  a += num_n;

  BN_ULONG v = limbs_sub(r, a, n, num_n) - carry;

  v = 0u - v;
  for (size_t i = 0; i < num_n; i++) {
    r[i] = constant_time_select_w(v, a[i], r[i]);
    a[i] = 0;
  }
  return 1;
}

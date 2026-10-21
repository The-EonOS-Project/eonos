#include "./p256_shared.h"

#include "../../limbs/limbs.h"

#if !defined(OPENSSL_USE_NISTZ256)

typedef Limb ScalarMont[P256_LIMBS];
typedef Limb Scalar[P256_LIMBS];

#include "../bn/internal.h"

static const BN_ULONG N[P256_LIMBS] = {
#if defined(OPENSSL_64_BIT)
  0xf3b9cac2fc632551, 0xbce6faada7179e84, 0xffffffffffffffff, 0xffffffff00000000
#else
  0xfc632551, 0xf3b9cac2, 0xa7179e84, 0xbce6faad, 0xffffffff, 0xffffffff, 0,
  0xffffffff
#endif
};

static const BN_ULONG N_N0[] = {
  BN_MONT_CTX_N0(0xccd1c8aa, 0xee00bc4f)
};

void p256_scalar_mul_mont(ScalarMont r, const ScalarMont a,
                              const ScalarMont b) {

  bn_mul_mont_small(r, a, b, N, N_N0, P256_LIMBS);
}

void p256_scalar_sqr_rep_mont(ScalarMont r, const ScalarMont a, Limb rep) {
  dev_assert_secret(rep >= 1);
  p256_scalar_mul_mont(r, a, a);
  for (Limb i = 1; i < rep; ++i) {
    p256_scalar_mul_mont(r, r, r);
  }
}

#endif

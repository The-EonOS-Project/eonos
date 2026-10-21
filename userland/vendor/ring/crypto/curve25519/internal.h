













#ifndef OPENSSL_HEADER_CURVE25519_INTERNAL_H
#define OPENSSL_HEADER_CURVE25519_INTERNAL_H

#include <ring-core/base.h>

#include "../internal.h"


#if defined(OPENSSL_ARM) && !defined(OPENSSL_NO_ASM) && !defined(OPENSSL_APPLE)
#define BORINGSSL_X25519_NEON


void x25519_NEON(uint8_t out[32], const uint8_t scalar[32],
                 const uint8_t point[32]);
#endif

#if !defined(OPENSSL_NO_ASM) && !defined(OPENSSL_SMALL) && \
    defined(__GNUC__) && defined(__x86_64__) && !defined(OPENSSL_WINDOWS)
#define BORINGSSL_FE25519_ADX



void __attribute__((sysv_abi))
fiat_curve25519_adx_mul(uint64_t out[4], const uint64_t in1[4],
                        const uint64_t in2[4]);



void __attribute__((sysv_abi))
fiat_curve25519_adx_square(uint64_t out[4], const uint64_t in[4]);


void x25519_scalar_mult_adx(uint8_t out[32], const uint8_t scalar[32],
                            const uint8_t point[32]);
void x25519_ge_scalarmult_base_adx(uint8_t h[4][32], const uint8_t a[32]);
#endif

#if defined(OPENSSL_64_BIT)





typedef uint64_t fe_limb_t;
#define FE_NUM_LIMBS 5
#else





typedef uint32_t fe_limb_t;
#define FE_NUM_LIMBS 10
#endif




typedef struct fe { fe_limb_t v[FE_NUM_LIMBS]; } fe;



typedef struct fe_loose { fe_limb_t v[FE_NUM_LIMBS]; } fe_loose;

static inline void fe_limbs_copy(fe_limb_t r[], const fe_limb_t a[]) {
  for (size_t i = 0; i < FE_NUM_LIMBS; ++i) {
    r[i] = a[i];
  }
}














typedef struct {
  fe X;
  fe Y;
  fe Z;
} ge_p2;



typedef struct {
  fe X;
  fe Y;
  fe Z;
  fe T;
} ge_p3;

typedef struct {
  fe_loose X;
  fe_loose Y;
  fe_loose Z;
  fe_loose T;
} ge_p1p1;

typedef struct {
  fe_loose yplusx;
  fe_loose yminusx;
  fe_loose xy2d;
} ge_precomp;

typedef struct {
  fe_loose YplusX;
  fe_loose YminusX;
  fe_loose Z;
  fe_loose T2d;
} ge_cached;

extern const uint8_t k25519Precomp[32][8][3][32];

#endif  

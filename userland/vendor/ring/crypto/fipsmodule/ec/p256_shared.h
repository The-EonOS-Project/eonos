#ifndef OPENSSL_HEADER_EC_P256_SHARED_H
#define OPENSSL_HEADER_EC_P256_SHARED_H

#include "ring-core/base.h"

#include "../bn/internal.h"

#if !defined(OPENSSL_NO_ASM) && \
    (defined(OPENSSL_X86_64) || defined(OPENSSL_AARCH64)) && \
    !defined(OPENSSL_SMALL)
# define OPENSSL_USE_NISTZ256
#endif

#define P256_LIMBS (256 / BN_BITS2)

typedef struct {
  BN_ULONG X[P256_LIMBS];
  BN_ULONG Y[P256_LIMBS];
  BN_ULONG Z[P256_LIMBS];
} P256_POINT;

typedef unsigned char P256_SCALAR_BYTES[33];

static inline void p256_scalar_bytes_from_limbs(
    P256_SCALAR_BYTES bytes_out, const BN_ULONG limbs[P256_LIMBS]) {
  OPENSSL_memcpy(bytes_out, limbs, 32);
  bytes_out[32] = 0;
}

#endif 

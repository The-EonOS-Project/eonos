#ifndef OPENSSL_HEADER_BN_INTERNAL_H
#define OPENSSL_HEADER_BN_INTERNAL_H

#include <ring-core/base.h>

#if defined(OPENSSL_X86_64) && defined(_MSC_VER) && !defined(__clang__)
#pragma warning(push, 3)
#include <intrin.h>
#pragma warning(pop)
#pragma intrinsic(_umul128)
#endif

#include "../../internal.h"

typedef crypto_word_t BN_ULONG;

#if defined(OPENSSL_64_BIT)

#if defined(BORINGSSL_HAS_UINT128)

#define BN_ULLONG uint128_t
#endif

#define BN_BITS2 64
#define BN_MONT_CTX_N0_LIMBS 1
#define BN_MONT_CTX_N0(hi, lo) TOBN(hi, lo), 0
#define TOBN(hi, lo) ((BN_ULONG)(hi) << 32 | (lo))

#elif defined(OPENSSL_32_BIT)

#define BN_ULLONG uint64_t
#define BN_BITS2 32

#define BN_MONT_CTX_N0_LIMBS 2
#define BN_MONT_CTX_N0(hi, lo) TOBN(hi, lo)
#define TOBN(hi, lo) (lo), (hi)

#else
#error "Must define either OPENSSL_32_BIT or OPENSSL_64_BIT"
#endif

OPENSSL_STATIC_ASSERT(sizeof(int) == sizeof(size_t) ||
                      (sizeof(int) == 4 && sizeof(size_t) == 8),
                      "int and size_t ABI mismatch");
#if defined(OPENSSL_X86_64)
void bn_mul_mont_nohw(BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
                      const BN_ULONG *np, const BN_ULONG *n0, size_t num);
static inline void bn_mul_mont_small(
    BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
    const BN_ULONG *np, const BN_ULONG *n0, size_t num) {
    bn_mul_mont_nohw(rp, ap, bp, np, n0, num);
}
#elif defined(OPENSSL_AARCH64)
void bn_mul_mont_nohw(BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
                      const BN_ULONG *np, const BN_ULONG *n0, size_t num);
static inline void bn_mul_mont_small(
    BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
    const BN_ULONG *np, const BN_ULONG *n0, size_t num) {

    bn_mul_mont_nohw(rp, ap, bp, np, n0, num);
}
#elif defined(OPENSSL_ARM)
void bn_mul8x_mont_neon(BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
                        const BN_ULONG *np, const BN_ULONG *n0, size_t num);
void bn_mul_mont_nohw(BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
                      const BN_ULONG *np, const BN_ULONG *n0, size_t num);
static inline void bn_mul_mont_small(
    BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
    const BN_ULONG *np, const BN_ULONG *n0, size_t num) {

    if (num == 8) {

        if (neon_available) {
            bn_mul8x_mont_neon(rp, ap, bp, np, n0, num);
            return;
        }
    }
    bn_mul_mont_nohw(rp, ap, bp, np, n0, num);
}
#else
void bn_mul_mont(BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
                 const BN_ULONG *np, const BN_ULONG *n0, size_t num);
static inline void bn_mul_mont_small(
    BN_ULONG *rp, const BN_ULONG *ap, const BN_ULONG *bp,
    const BN_ULONG *np, const BN_ULONG *n0, size_t num) {
    bn_mul_mont(rp, ap, bp, np, n0, num);
}
#endif

static inline void bn_umult_lohi(BN_ULONG *low_out, BN_ULONG *high_out,
                                 BN_ULONG a, BN_ULONG b) {
#if defined(OPENSSL_X86_64) && defined(_MSC_VER) && !defined(__clang__)
  *low_out = _umul128(a, b, high_out);
#else
  BN_ULLONG result = (BN_ULLONG)a * b;
  *low_out = (BN_ULONG)result;
  *high_out = (BN_ULONG)(result >> BN_BITS2);
#endif
}

#endif  

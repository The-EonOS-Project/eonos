#ifndef OPENSSL_HEADER_EC_P256_X86_64_H
#define OPENSSL_HEADER_EC_P256_X86_64_H

#include <ring-core/base.h>

#include "p256_shared.h"

#include "../bn/internal.h"

#if defined(OPENSSL_USE_NISTZ256)

void ecp_nistz256_neg(BN_ULONG res[P256_LIMBS], const BN_ULONG a[P256_LIMBS]);

#if defined(OPENSSL_X86_64)
void ecp_nistz256_mul_mont_nohw(BN_ULONG res[P256_LIMBS],
                                const BN_ULONG a[P256_LIMBS],
                                const BN_ULONG b[P256_LIMBS]);
void ecp_nistz256_mul_mont_adx(BN_ULONG res[P256_LIMBS],
                               const BN_ULONG a[P256_LIMBS],
                               const BN_ULONG b[P256_LIMBS]);
#else
void ecp_nistz256_mul_mont(BN_ULONG res[P256_LIMBS],
                           const BN_ULONG a[P256_LIMBS],
                           const BN_ULONG b[P256_LIMBS]);
#endif

#if defined(OPENSSL_X86_64)
void ecp_nistz256_sqr_mont_nohw(BN_ULONG res[P256_LIMBS],
                                const BN_ULONG a[P256_LIMBS]);
void ecp_nistz256_sqr_mont_adx(BN_ULONG res[P256_LIMBS],
                               const BN_ULONG a[P256_LIMBS]);
#else
void ecp_nistz256_sqr_mont(BN_ULONG res[P256_LIMBS],
                           const BN_ULONG a[P256_LIMBS]);
#endif

#if defined(OPENSSL_X86_64)
void ecp_nistz256_ord_mul_mont_nohw(BN_ULONG res[P256_LIMBS],
                                    const BN_ULONG a[P256_LIMBS],
                                    const BN_ULONG b[P256_LIMBS]);
void ecp_nistz256_ord_mul_mont_adx(BN_ULONG res[P256_LIMBS],
                                   const BN_ULONG a[P256_LIMBS],
                                   const BN_ULONG b[P256_LIMBS]);
#else
void ecp_nistz256_ord_mul_mont(BN_ULONG res[P256_LIMBS],
                               const BN_ULONG a[P256_LIMBS],
                               const BN_ULONG b[P256_LIMBS]);
#endif

#if defined(OPENSSL_X86_64)
void ecp_nistz256_ord_sqr_mont_nohw(BN_ULONG res[P256_LIMBS],
                                    const BN_ULONG a[P256_LIMBS], BN_ULONG rep);
void ecp_nistz256_ord_sqr_mont_adx(BN_ULONG res[P256_LIMBS],
                                   const BN_ULONG a[P256_LIMBS], BN_ULONG rep);
#else
void ecp_nistz256_ord_sqr_mont(BN_ULONG res[P256_LIMBS],
                               const BN_ULONG a[P256_LIMBS], BN_ULONG rep);
#endif

typedef struct {
  BN_ULONG X[P256_LIMBS];
  BN_ULONG Y[P256_LIMBS];
} P256_POINT_AFFINE;

#if defined(OPENSSL_X86_64)
void ecp_nistz256_select_w5_nohw(P256_POINT *val, const P256_POINT in_t[16],
                                 int index);
void ecp_nistz256_select_w5_avx2(P256_POINT *val, const P256_POINT in_t[16],
                                 int index);
#else
void ecp_nistz256_select_w5(P256_POINT *val, const P256_POINT in_t[16],
                            int index);
#endif

#if defined(OPENSSL_X86_64)
void ecp_nistz256_select_w7_nohw(P256_POINT_AFFINE *val,
                                 const P256_POINT_AFFINE in_t[64], int index);
void ecp_nistz256_select_w7_avx2(P256_POINT_AFFINE *val,
                                 const P256_POINT_AFFINE in_t[64], int index);
#else
void ecp_nistz256_select_w7(P256_POINT_AFFINE *val,
                            const P256_POINT_AFFINE in_t[64], int index);
#endif

#if defined(OPENSSL_X86_64)
void ecp_nistz256_point_double_nohw(P256_POINT *r, const P256_POINT *a);
void ecp_nistz256_point_double_adx(P256_POINT *r, const P256_POINT *a);
#else
void ecp_nistz256_point_double(P256_POINT *r, const P256_POINT *a);
#endif

#if defined(OPENSSL_X86_64)
void ecp_nistz256_point_add_nohw(P256_POINT *r, const P256_POINT *a,
                                 const P256_POINT *b);
void ecp_nistz256_point_add_adx(P256_POINT *r, const P256_POINT *a,
                                const P256_POINT *b);
#else
void ecp_nistz256_point_add(P256_POINT *r, const P256_POINT *a,
                            const P256_POINT *b);
#endif

#if defined(OPENSSL_X86_64)
void ecp_nistz256_point_add_affine_adx(P256_POINT *r, const P256_POINT *a,
                                       const P256_POINT_AFFINE *b);
void ecp_nistz256_point_add_affine_nohw(P256_POINT *r, const P256_POINT *a,
                                        const P256_POINT_AFFINE *b);
#else
void ecp_nistz256_point_add_affine(P256_POINT *r, const P256_POINT *a,
                                   const P256_POINT_AFFINE *b);
#endif

#endif 

#endif  

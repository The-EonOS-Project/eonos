#ifndef RING_LIMBS_H
#define RING_LIMBS_H

#include <ring-core/base.h>

#include "../internal.h"

typedef crypto_word_t Limb;

#define LIMB_BITS CRYPTO_WORD_BITS
#define LIMB_HIGH_BIT ((Limb)(1) << (LIMB_BITS - 1))

Limb LIMBS_are_zero(const Limb a[], size_t num_limbs);
Limb LIMBS_equal(const Limb a[], const Limb b[], size_t num_limbs);
void LIMBS_reduce_once(Limb r[], const Limb m[], size_t num_limbs);
void LIMBS_add_mod(Limb r[], const Limb a[], const Limb b[], const Limb m[],
                   size_t num_limbs);
void LIMBS_sub_mod(Limb r[], const Limb a[], const Limb b[], const Limb m[],
                   size_t num_limbs);
void LIMBS_shl_mod(Limb r[], const Limb a[], const Limb m[], size_t num_limbs);
Limb limbs_mul_add_limb(Limb r[], const Limb a[], Limb b, size_t num_limbs);

#endif 

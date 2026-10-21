













#ifndef OPENSSL_HEADER_EC_ECP_NISTZ_H
#define OPENSSL_HEADER_EC_ECP_NISTZ_H

#include <ring-core/base.h>

#include "../../limbs/limbs.h"

#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wconversion"
#pragma GCC diagnostic ignored "-Wsign-conversion"
#endif






























































































































































































































static inline void booth_recode(crypto_word_t *is_negative, crypto_word_t *digit,
                                crypto_word_t in, crypto_word_t w) {
  debug_assert_nonsecret(w >= 2);
  debug_assert_nonsecret(w <= 7);

  
  
  crypto_word_t s = ~((in >> w) - 1);
  crypto_word_t d;
  d = ((crypto_word_t)1u << (w + 1)) - in - 1;
  d = (d & s) | (in & ~s);
  d = (d >> 1) + (d & 1);

  *is_negative = constant_time_is_nonzero_w(s & 1);
  *digit = d;
}

#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

void little_endian_bytes_from_scalar(uint8_t str[], size_t str_len,
                                     const Limb scalar[],
                                     size_t num_limbs);

#endif 

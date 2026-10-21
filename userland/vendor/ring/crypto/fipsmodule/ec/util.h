#include <ring-core/base.h>

#include "../../internal.h"

#if defined(__GNUC__)
#pragma GCC diagnostic ignored "-Wconversion"
#pragma GCC diagnostic ignored "-Wsign-conversion"
#endif

static inline void recode_scalar_bits(crypto_word_t *sign, crypto_word_t *digit,
                                      crypto_word_t in) {
  crypto_word_t s, d;

  s = ~((in >> 5) - 1); 

  d = (1 << 6) - in - 1;
  d = (d & s) | (in & ~s);
  d = (d >> 1) + (d & 1);

  *sign = s & 1;
  *digit = d;
}

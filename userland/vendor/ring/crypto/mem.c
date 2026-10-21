#include <ring-core/mem.h>
#include "internal.h"

int CRYPTO_memcmp(const void *in_a, const void *in_b, size_t len) {
  const aliasing_uint8_t *a = in_a;
  const aliasing_uint8_t *b = in_b;
  uint8_t x = 0;

  for (size_t i = 0; i < len; i++) {
    x |= a[i] ^ b[i];
  }

  return x;
}

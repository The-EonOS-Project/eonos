#include "ecp_nistz.h"

#if defined(__GNUC__)
#pragma GCC diagnostic ignored "-Wconversion"
#endif

void little_endian_bytes_from_scalar(uint8_t str[], size_t str_len,
                                         const Limb scalar[],
                                         size_t num_limbs) {
  debug_assert_nonsecret(str_len == (num_limbs * sizeof(Limb)) + 1);

  size_t i;
  for (i = 0; i < num_limbs * sizeof(Limb); i += sizeof(Limb)) {
    Limb d = scalar[i / sizeof(Limb)];

    str[i + 0] = d & 0xff;
    str[i + 1] = (d >> 8) & 0xff;
    str[i + 2] = (d >> 16) & 0xff;
    str[i + 3] = (d >>= 24) & 0xff;
    if (sizeof(Limb) == 8) {
      d >>= 8;
      str[i + 4] = d & 0xff;
      str[i + 5] = (d >> 8) & 0xff;
      str[i + 6] = (d >> 16) & 0xff;
      str[i + 7] = (d >> 24) & 0xff;
    }
  }
  for (; i < str_len; i++) {
    str[i] = 0;
  }
}

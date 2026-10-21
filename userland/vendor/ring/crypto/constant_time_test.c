#include "internal.h"

int bssl_constant_time_test_main(void);

static int test_binary_op_w(crypto_word_t (*op)(crypto_word_t a, crypto_word_t b),
                            crypto_word_t a, crypto_word_t b, int is_true) {
  crypto_word_t c = op(a, b);
  if (is_true && c != CONSTTIME_TRUE_W) {
    return 1;
  } else if (!is_true && c != CONSTTIME_FALSE_W) {
    return 1;
  }
  return 0;
}

static int test_is_zero_w(crypto_word_t a) {
  crypto_word_t c = constant_time_is_zero_w(a);
  if (a == 0 && c != CONSTTIME_TRUE_W) {
    return 1;
  } else if (a != 0 && c != CONSTTIME_FALSE_W) {
    return 1;
  }

  c = constant_time_is_nonzero_w(a);
  if (a == 0 && c != CONSTTIME_FALSE_W) {
    return 1;
  } else if (a != 0 && c != CONSTTIME_TRUE_W) {
    return 1;
  }

  return 0;
}

static int test_select_w(crypto_word_t a, crypto_word_t b) {
  crypto_word_t selected = constant_time_select_w(CONSTTIME_TRUE_W, a, b);
  if (selected != a) {
    return 1;
  }
  selected = constant_time_select_w(CONSTTIME_FALSE_W, a, b);
  if (selected != b) {
    return 1;
  }
  return 0;
}

static crypto_word_t test_values_w[] = {
    0,
    1,
    1024,
    12345,
    32000,
#if defined(OPENSSL_64_BIT)
    0xffffffff / 2 - 1,
    0xffffffff / 2,
    0xffffffff / 2 + 1,
    0xffffffff - 1,
    0xffffffff,
#endif
    SIZE_MAX / 2 - 1,
    SIZE_MAX / 2,
    SIZE_MAX / 2 + 1,
    SIZE_MAX - 1,
    SIZE_MAX
};

int bssl_constant_time_test_main(void) {
  int num_failed = 0;

  for (size_t i = 0;
       i < sizeof(test_values_w) / sizeof(test_values_w[0]); ++i) {
    crypto_word_t a = test_values_w[i];
    num_failed += test_is_zero_w(a);
    for (size_t j = 0;
         j < sizeof(test_values_w) / sizeof(test_values_w[0]); ++j) {
      crypto_word_t b = test_values_w[j];
      num_failed += test_binary_op_w(&constant_time_eq_w, a, b, a == b);
      num_failed += test_binary_op_w(&constant_time_eq_w, b, a, b == a);
      num_failed += test_select_w(a, b);
    }
  }

  return num_failed == 0;
}

void bssl_constant_time_test_conditional_memcpy(uint8_t dst[256], const uint8_t src[256],
                                                crypto_word_t b) {
    constant_time_conditional_memcpy(dst, src, 256, b);
 }

void bssl_constant_time_test_conditional_memxor(uint8_t dst[256],
                                               const uint8_t src[256],
                                               crypto_word_t b) {
  constant_time_conditional_memxor(dst, src, 256, b);
}

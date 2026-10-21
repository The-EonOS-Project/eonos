#include <ring-core/base.h>

#include "../../limbs/limbs.h"
#include "../../limbs/limbs.inl"

#include "p256_shared.h"

#include "../../internal.h"
#include "./util.h"

#if !defined(OPENSSL_USE_NISTZ256)

#if defined(_MSC_VER) && !defined(__clang__)

#pragma warning(disable: 4242)

#pragma warning(disable: 4244)

#pragma warning(disable: 4267)
#endif

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wconversion"
#pragma GCC diagnostic ignored "-Wsign-conversion"
#endif

#if defined(__GNUC__) && !defined(__clang__)
#pragma GCC diagnostic ignored "-Winline"
#endif

#if defined(BORINGSSL_HAS_UINT128)
#if defined(__GNUC__)
#pragma GCC diagnostic ignored "-Wpedantic"
#endif
#include "../../../third_party/fiat/p256_64.h"
#elif defined(OPENSSL_64_BIT)
#include "../../../third_party/fiat/p256_64_msvc.h"
#else
#include "../../../third_party/fiat/p256_32.h"
#endif

#if defined(OPENSSL_64_BIT)
#define FIAT_P256_NLIMBS 4
typedef uint64_t fiat_p256_limb_t;
typedef uint64_t fiat_p256_felem[FIAT_P256_NLIMBS];
static const fiat_p256_felem fiat_p256_one = {0x1, 0xffffffff00000000,
                                              0xffffffffffffffff, 0xfffffffe};
#else  
#define FIAT_P256_NLIMBS 8
typedef uint32_t fiat_p256_limb_t;
typedef uint32_t fiat_p256_felem[FIAT_P256_NLIMBS];
static const fiat_p256_felem fiat_p256_one = {
    0x1, 0x0, 0x0, 0xffffffff, 0xffffffff, 0xffffffff, 0xfffffffe, 0x0};
#endif  

static fiat_p256_limb_t fiat_p256_nz(
    const fiat_p256_limb_t in1[FIAT_P256_NLIMBS]) {
  fiat_p256_limb_t ret;
  fiat_p256_nonzero(&ret, in1);
  return ret;
}

static void fiat_p256_copy(fiat_p256_limb_t out[FIAT_P256_NLIMBS],
                           const fiat_p256_limb_t in1[FIAT_P256_NLIMBS]) {
  for (size_t i = 0; i < FIAT_P256_NLIMBS; i++) {
    out[i] = in1[i];
  }
}

static void fiat_p256_cmovznz(fiat_p256_limb_t out[FIAT_P256_NLIMBS],
                              fiat_p256_limb_t t,
                              const fiat_p256_limb_t z[FIAT_P256_NLIMBS],
                              const fiat_p256_limb_t nz[FIAT_P256_NLIMBS]) {
  fiat_p256_selectznz(out, !!t, z, nz);
}

static void fiat_p256_from_words(fiat_p256_felem out,
                                 const Limb in[32 / sizeof(BN_ULONG)]) {

  OPENSSL_memcpy(out, in, 32);
}

static void fiat_p256_to_words(Limb out[32 / sizeof(BN_ULONG)], const fiat_p256_felem in) {

  OPENSSL_memcpy(out, in, 32);
}

static void fiat_p256_point_double(fiat_p256_felem x_out, fiat_p256_felem y_out,
                                   fiat_p256_felem z_out,
                                   const fiat_p256_felem x_in,
                                   const fiat_p256_felem y_in,
                                   const fiat_p256_felem z_in) {
  fiat_p256_felem delta, gamma, beta, ftmp, ftmp2, tmptmp, alpha, fourbeta;

  fiat_p256_square(delta, z_in);

  fiat_p256_square(gamma, y_in);

  fiat_p256_mul(beta, x_in, gamma);

  fiat_p256_sub(ftmp, x_in, delta);
  fiat_p256_add(ftmp2, x_in, delta);

  fiat_p256_add(tmptmp, ftmp2, ftmp2);
  fiat_p256_add(ftmp2, ftmp2, tmptmp);
  fiat_p256_mul(alpha, ftmp, ftmp2);

  fiat_p256_square(x_out, alpha);
  fiat_p256_add(fourbeta, beta, beta);
  fiat_p256_add(fourbeta, fourbeta, fourbeta);
  fiat_p256_add(tmptmp, fourbeta, fourbeta);
  fiat_p256_sub(x_out, x_out, tmptmp);

  fiat_p256_add(delta, gamma, delta);
  fiat_p256_add(ftmp, y_in, z_in);
  fiat_p256_square(z_out, ftmp);
  fiat_p256_sub(z_out, z_out, delta);

  fiat_p256_sub(y_out, fourbeta, x_out);
  fiat_p256_add(gamma, gamma, gamma);
  fiat_p256_square(gamma, gamma);
  fiat_p256_mul(y_out, alpha, y_out);
  fiat_p256_add(gamma, gamma, gamma);
  fiat_p256_sub(y_out, y_out, gamma);
}

static void fiat_p256_point_add(fiat_p256_felem x3, fiat_p256_felem y3,
                                fiat_p256_felem z3, const fiat_p256_felem x1,
                                const fiat_p256_felem y1,
                                const fiat_p256_felem z1, const int mixed,
                                const fiat_p256_felem x2,
                                const fiat_p256_felem y2,
                                const fiat_p256_felem z2) {
  fiat_p256_felem x_out, y_out, z_out;
  fiat_p256_limb_t z1nz = fiat_p256_nz(z1);
  fiat_p256_limb_t z2nz = fiat_p256_nz(z2);

  fiat_p256_felem z1z1;
  fiat_p256_square(z1z1, z1);

  fiat_p256_felem u1, s1, two_z1z2;
  if (!mixed) {

    fiat_p256_felem z2z2;
    fiat_p256_square(z2z2, z2);

    fiat_p256_mul(u1, x1, z2z2);

    fiat_p256_add(two_z1z2, z1, z2);
    fiat_p256_square(two_z1z2, two_z1z2);
    fiat_p256_sub(two_z1z2, two_z1z2, z1z1);
    fiat_p256_sub(two_z1z2, two_z1z2, z2z2);

    fiat_p256_mul(s1, z2, z2z2);
    fiat_p256_mul(s1, s1, y1);
  } else {

    fiat_p256_copy(u1, x1);

    fiat_p256_add(two_z1z2, z1, z1);

    fiat_p256_copy(s1, y1);
  }

  fiat_p256_felem u2;
  fiat_p256_mul(u2, x2, z1z1);

  fiat_p256_felem h;
  fiat_p256_sub(h, u2, u1);

  fiat_p256_limb_t xneq = fiat_p256_nz(h);

  fiat_p256_mul(z_out, h, two_z1z2);

  fiat_p256_felem z1z1z1;
  fiat_p256_mul(z1z1z1, z1, z1z1);

  fiat_p256_felem s2;
  fiat_p256_mul(s2, y2, z1z1z1);

  fiat_p256_felem r;
  fiat_p256_sub(r, s2, s1);
  fiat_p256_add(r, r, r);

  fiat_p256_limb_t yneq = fiat_p256_nz(r);

  fiat_p256_limb_t is_nontrivial_double = constant_time_is_zero_w(xneq | yneq) &
                                          ~constant_time_is_zero_w(z1nz) &
                                          ~constant_time_is_zero_w(z2nz);
  if (constant_time_declassify_w(is_nontrivial_double)) {
    fiat_p256_point_double(x3, y3, z3, x1, y1, z1);
    return;
  }

  fiat_p256_felem i;
  fiat_p256_add(i, h, h);
  fiat_p256_square(i, i);

  fiat_p256_felem j;
  fiat_p256_mul(j, h, i);

  fiat_p256_felem v;
  fiat_p256_mul(v, u1, i);

  fiat_p256_square(x_out, r);
  fiat_p256_sub(x_out, x_out, j);
  fiat_p256_sub(x_out, x_out, v);
  fiat_p256_sub(x_out, x_out, v);

  fiat_p256_sub(y_out, v, x_out);
  fiat_p256_mul(y_out, y_out, r);
  fiat_p256_felem s1j;
  fiat_p256_mul(s1j, s1, j);
  fiat_p256_sub(y_out, y_out, s1j);
  fiat_p256_sub(y_out, y_out, s1j);

  fiat_p256_cmovznz(x_out, z1nz, x2, x_out);
  fiat_p256_cmovznz(x3, z2nz, x1, x_out);
  fiat_p256_cmovznz(y_out, z1nz, y2, y_out);
  fiat_p256_cmovznz(y3, z2nz, y1, y_out);
  fiat_p256_cmovznz(z_out, z1nz, z2, z_out);
  fiat_p256_cmovznz(z3, z2nz, z1, z_out);
}

#include "./p256_table.h"

static void fiat_p256_select_point_affine(
    const fiat_p256_limb_t idx, size_t size,
    const fiat_p256_felem pre_comp[ ][2], fiat_p256_felem out[3]) {
  OPENSSL_memset(out, 0, sizeof(fiat_p256_felem) * 3);
  for (size_t i = 0; i < size; i++) {
    fiat_p256_limb_t mismatch = i ^ (idx - 1);
    fiat_p256_cmovznz(out[0], mismatch, pre_comp[i][0], out[0]);
    fiat_p256_cmovznz(out[1], mismatch, pre_comp[i][1], out[1]);
  }
  fiat_p256_cmovznz(out[2], idx, out[2], fiat_p256_one);
}

static void fiat_p256_select_point(const fiat_p256_limb_t idx, size_t size,
                                   const fiat_p256_felem pre_comp[ ][3],
                                   fiat_p256_felem out[3]) {
  OPENSSL_memset(out, 0, sizeof(fiat_p256_felem) * 3);
  for (size_t i = 0; i < size; i++) {
    fiat_p256_limb_t mismatch = i ^ idx;
    fiat_p256_cmovznz(out[0], mismatch, pre_comp[i][0], out[0]);
    fiat_p256_cmovznz(out[1], mismatch, pre_comp[i][1], out[1]);
    fiat_p256_cmovznz(out[2], mismatch, pre_comp[i][2], out[2]);
  }
}

static crypto_word_t fiat_p256_get_bit(const Limb in[P256_LIMBS], int i) {
  if (i < 0 || i >= 256) {
    return 0;
  }
#if defined(OPENSSL_64_BIT)
  OPENSSL_STATIC_ASSERT(sizeof(Limb) == 8, "BN_ULONG was not 64-bit");
  return (in[i >> 6] >> (i & 63)) & 1;
#else
  OPENSSL_STATIC_ASSERT(sizeof(Limb) == 4, "BN_ULONG was not 32-bit");
  return (in[i >> 5] >> (i & 31)) & 1;
#endif
}

void p256_point_mul(Limb r[3][P256_LIMBS], const Limb scalar[P256_LIMBS],
                    const Limb p_x[P256_LIMBS], const Limb p_y[P256_LIMBS]) {
  debug_assert_nonsecret(r != NULL);
  debug_assert_nonsecret(scalar != NULL);
  debug_assert_nonsecret(p_x != NULL);
  debug_assert_nonsecret(p_y != NULL);

  fiat_p256_felem p_pre_comp[17][3];
  OPENSSL_memset(&p_pre_comp, 0, sizeof(p_pre_comp));

  fiat_p256_from_words(p_pre_comp[1][0], p_x);
  fiat_p256_from_words(p_pre_comp[1][1], p_y);
  fiat_p256_copy(p_pre_comp[1][2], fiat_p256_one);

  for (size_t j = 2; j <= 16; ++j) {
    if (j & 1) {
      fiat_p256_point_add(p_pre_comp[j][0], p_pre_comp[j][1], p_pre_comp[j][2],
                          p_pre_comp[1][0], p_pre_comp[1][1], p_pre_comp[1][2],
                          0, p_pre_comp[j - 1][0], p_pre_comp[j - 1][1],
                          p_pre_comp[j - 1][2]);
    } else {
      fiat_p256_point_double(p_pre_comp[j][0], p_pre_comp[j][1],
                             p_pre_comp[j][2], p_pre_comp[j / 2][0],
                             p_pre_comp[j / 2][1], p_pre_comp[j / 2][2]);
    }
  }

  fiat_p256_felem nq[3] = {{0}, {0}, {0}}, ftmp, tmp[3];

  int skip = 1;  
  for (size_t i = 255; i < 256; i--) {

    if (!skip) {
      fiat_p256_point_double(nq[0], nq[1], nq[2], nq[0], nq[1], nq[2]);
    }

    if (i % 5 == 0) {
      crypto_word_t bits = fiat_p256_get_bit(scalar, i + 4) << 5;
      bits |= fiat_p256_get_bit(scalar, i + 3) << 4;
      bits |= fiat_p256_get_bit(scalar, i + 2) << 3;
      bits |= fiat_p256_get_bit(scalar, i + 1) << 2;
      bits |= fiat_p256_get_bit(scalar, i) << 1;
      bits |= fiat_p256_get_bit(scalar, i - 1);
      crypto_word_t sign, digit;
      recode_scalar_bits(&sign, &digit, bits);

      fiat_p256_select_point((fiat_p256_limb_t)digit, 17,
        RING_CORE_POINTLESS_ARRAY_CONST_CAST((const fiat_p256_felem(*)[3]))p_pre_comp,
        tmp);
      fiat_p256_opp(ftmp, tmp[1]);  
      fiat_p256_cmovznz(tmp[1], (fiat_p256_limb_t)sign, tmp[1], ftmp);

      if (!skip) {
        fiat_p256_point_add(nq[0], nq[1], nq[2], nq[0], nq[1], nq[2],
                            0 , tmp[0], tmp[1], tmp[2]);
      } else {
        fiat_p256_copy(nq[0], tmp[0]);
        fiat_p256_copy(nq[1], tmp[1]);
        fiat_p256_copy(nq[2], tmp[2]);
        skip = 0;
      }
    }
  }

  fiat_p256_to_words(r[0], nq[0]);
  fiat_p256_to_words(r[1], nq[1]);
  fiat_p256_to_words(r[2], nq[2]);
}

void p256_point_mul_base(Limb r[3][P256_LIMBS], const Limb scalar[P256_LIMBS]) {

  fiat_p256_felem nq[3] = {{0}, {0}, {0}}, tmp[3];

  int skip = 1;  
  for (size_t i = 31; i < 32; i--) {
    if (!skip) {
      fiat_p256_point_double(nq[0], nq[1], nq[2], nq[0], nq[1], nq[2]);
    }

    crypto_word_t bits = fiat_p256_get_bit(scalar, i + 224) << 3;
    bits |= fiat_p256_get_bit(scalar, i + 160) << 2;
    bits |= fiat_p256_get_bit(scalar, i + 96) << 1;
    bits |= fiat_p256_get_bit(scalar, i + 32);

    fiat_p256_select_point_affine((fiat_p256_limb_t)bits, 15,
                                  fiat_p256_g_pre_comp[1], tmp);

    if (!skip) {
      fiat_p256_point_add(nq[0], nq[1], nq[2], nq[0], nq[1], nq[2],
                          1 , tmp[0], tmp[1], tmp[2]);
    } else {
      fiat_p256_copy(nq[0], tmp[0]);
      fiat_p256_copy(nq[1], tmp[1]);
      fiat_p256_copy(nq[2], tmp[2]);
      skip = 0;
    }

    bits = fiat_p256_get_bit(scalar, i + 192) << 3;
    bits |= fiat_p256_get_bit(scalar, i + 128) << 2;
    bits |= fiat_p256_get_bit(scalar, i + 64) << 1;
    bits |= fiat_p256_get_bit(scalar, i);

    fiat_p256_select_point_affine((fiat_p256_limb_t)bits, 15,
                                  fiat_p256_g_pre_comp[0], tmp);
    fiat_p256_point_add(nq[0], nq[1], nq[2], nq[0], nq[1], nq[2], 1 ,
                        tmp[0], tmp[1], tmp[2]);
  }

  fiat_p256_to_words(r[0], nq[0]);
  fiat_p256_to_words(r[1], nq[1]);
  fiat_p256_to_words(r[2], nq[2]);
}

void p256_mul_mont(Limb r[P256_LIMBS], const Limb a[P256_LIMBS],
                   const Limb b[P256_LIMBS]) {
  fiat_p256_felem a_, b_;
  fiat_p256_from_words(a_, a);
  fiat_p256_from_words(b_, b);
  fiat_p256_mul(a_, a_, b_);
  fiat_p256_to_words(r, a_);
}

void p256_sqr_mont(Limb r[P256_LIMBS], const Limb a[P256_LIMBS]) {
  fiat_p256_felem x;
  fiat_p256_from_words(x, a);
  fiat_p256_square(x, x);
  fiat_p256_to_words(r, x);
}

void p256_point_add(Limb r[3][P256_LIMBS], const Limb a[3][P256_LIMBS],
                    const Limb b[3][P256_LIMBS]) {
  fiat_p256_felem x1, y1, z1, x2, y2, z2;
  fiat_p256_from_words(x1, a[0]);
  fiat_p256_from_words(y1, a[1]);
  fiat_p256_from_words(z1, a[2]);
  fiat_p256_from_words(x2, b[0]);
  fiat_p256_from_words(y2, b[1]);
  fiat_p256_from_words(z2, b[2]);
  fiat_p256_point_add(x1, y1, z1, x1, y1, z1, 0 , x2, y2,
                      z2);
  fiat_p256_to_words(r[0], x1);
  fiat_p256_to_words(r[1], y1);
  fiat_p256_to_words(r[2], z1);
}

void p256_point_double(Limb r[3][P256_LIMBS], const Limb a[3][P256_LIMBS]) {
  fiat_p256_felem x, y, z;
  fiat_p256_from_words(x, a[0]);
  fiat_p256_from_words(y, a[1]);
  fiat_p256_from_words(z, a[2]);
  fiat_p256_point_double(x, y, z, x, y, z);
  fiat_p256_to_words(r[0], x);
  fiat_p256_to_words(r[1], y);
  fiat_p256_to_words(r[2], z);
}

void p256_point_add_affine(Limb r[3][P256_LIMBS], const Limb a[3][P256_LIMBS],
                           const Limb b[2][P256_LIMBS]) {
  fiat_p256_felem x1, y1, z1, x2, y2;
  fiat_p256_from_words(x1, a[0]);
  fiat_p256_from_words(y1, a[1]);
  fiat_p256_from_words(z1, a[2]);
  fiat_p256_from_words(x2, b[0]);
  fiat_p256_from_words(y2, b[1]);

  fiat_p256_felem z2 = {0};
  fiat_p256_cmovznz(z2, fiat_p256_nz(x2) & fiat_p256_nz(y2), z2, fiat_p256_one);

  fiat_p256_point_add(x1, y1, z1, x1, y1, z1, 1 , x2, y2, z2);

  fiat_p256_to_words(r[0], x1);
  fiat_p256_to_words(r[1], y1);
  fiat_p256_to_words(r[2], z1);
}

#endif

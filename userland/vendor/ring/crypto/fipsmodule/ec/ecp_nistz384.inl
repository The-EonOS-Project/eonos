#include "ecp_nistz.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wsign-conversion"
#endif

static void nistz384_point_double(P384_POINT *r, const P384_POINT *a) {
  BN_ULONG S[P384_LIMBS];
  BN_ULONG M[P384_LIMBS];
  BN_ULONG Zsqr[P384_LIMBS];
  BN_ULONG tmp0[P384_LIMBS];

  const BN_ULONG *in_x = a->X;
  const BN_ULONG *in_y = a->Y;
  const BN_ULONG *in_z = a->Z;

  BN_ULONG *res_x = r->X;
  BN_ULONG *res_y = r->Y;
  BN_ULONG *res_z = r->Z;

  elem_mul_by_2(S, in_y);

  elem_sqr_mont(Zsqr, in_z);

  elem_sqr_mont(S, S);

  elem_mul_mont(res_z, in_z, in_y);
  elem_mul_by_2(res_z, res_z);

  elem_add(M, in_x, Zsqr);
  elem_sub(Zsqr, in_x, Zsqr);

  elem_sqr_mont(res_y, S);
  elem_div_by_2(res_y, res_y);

  elem_mul_mont(M, M, Zsqr);
  elem_mul_by_3(M, M);

  elem_mul_mont(S, S, in_x);
  elem_mul_by_2(tmp0, S);

  elem_sqr_mont(res_x, M);

  elem_sub(res_x, res_x, tmp0);
  elem_sub(S, S, res_x);

  elem_mul_mont(S, S, M);
  elem_sub(res_y, S, res_y);
}

static void nistz384_point_add(P384_POINT *r, const P384_POINT *a,
                               const P384_POINT *b) {
  BN_ULONG U2[P384_LIMBS], S2[P384_LIMBS];
  BN_ULONG U1[P384_LIMBS], S1[P384_LIMBS];
  BN_ULONG Z1sqr[P384_LIMBS];
  BN_ULONG Z2sqr[P384_LIMBS];
  BN_ULONG H[P384_LIMBS], R[P384_LIMBS];
  BN_ULONG Hsqr[P384_LIMBS];
  BN_ULONG Rsqr[P384_LIMBS];
  BN_ULONG Hcub[P384_LIMBS];

  BN_ULONG res_x[P384_LIMBS];
  BN_ULONG res_y[P384_LIMBS];
  BN_ULONG res_z[P384_LIMBS];

  const BN_ULONG *in1_x = a->X;
  const BN_ULONG *in1_y = a->Y;
  const BN_ULONG *in1_z = a->Z;

  const BN_ULONG *in2_x = b->X;
  const BN_ULONG *in2_y = b->Y;
  const BN_ULONG *in2_z = b->Z;

  BN_ULONG in1infty = is_zero(a->Z);
  BN_ULONG in2infty = is_zero(b->Z);

  elem_sqr_mont(Z2sqr, in2_z); 
  elem_sqr_mont(Z1sqr, in1_z); 

  elem_mul_mont(S1, Z2sqr, in2_z); 
  elem_mul_mont(S2, Z1sqr, in1_z); 

  elem_mul_mont(S1, S1, in1_y); 
  elem_mul_mont(S2, S2, in2_y); 
  elem_sub(R, S2, S1);          

  elem_mul_mont(U1, in1_x, Z2sqr); 
  elem_mul_mont(U2, in2_x, Z1sqr); 
  elem_sub(H, U2, U1);             

  BN_ULONG is_exceptional = is_equal(U1, U2) & ~in1infty & ~in2infty;
  if (is_exceptional) {
    if (is_equal(S1, S2)) {
      nistz384_point_double(r, a);
    } else {
      limbs_zero(r->X, P384_LIMBS);
      limbs_zero(r->Y, P384_LIMBS);
      limbs_zero(r->Z, P384_LIMBS);
    }
    return;
  }

  elem_sqr_mont(Rsqr, R);             
  elem_mul_mont(res_z, H, in1_z);     
  elem_sqr_mont(Hsqr, H);             
  elem_mul_mont(res_z, res_z, in2_z); 
  elem_mul_mont(Hcub, Hsqr, H);       

  elem_mul_mont(U2, U1, Hsqr); 
  elem_mul_by_2(Hsqr, U2);     

  elem_sub(res_x, Rsqr, Hsqr);
  elem_sub(res_x, res_x, Hcub);

  elem_sub(res_y, U2, res_x);

  elem_mul_mont(S2, S1, Hcub);
  elem_mul_mont(res_y, R, res_y);
  elem_sub(res_y, res_y, S2);

  copy_conditional(res_x, in2_x, in1infty);
  copy_conditional(res_y, in2_y, in1infty);
  copy_conditional(res_z, in2_z, in1infty);

  copy_conditional(res_x, in1_x, in2infty);
  copy_conditional(res_y, in1_y, in2infty);
  copy_conditional(res_z, in1_z, in2infty);

  limbs_copy(r->X, res_x, P384_LIMBS);
  limbs_copy(r->Y, res_y, P384_LIMBS);
  limbs_copy(r->Z, res_z, P384_LIMBS);
}

static void add_precomputed_w5(P384_POINT *r, crypto_word_t wvalue,
                               const P384_POINT table[16]) {
  crypto_word_t recoded_is_negative;
  crypto_word_t recoded;
  booth_recode(&recoded_is_negative, &recoded, wvalue, 5);

  alignas(64) P384_POINT h;
  p384_point_select_w5(&h, table, recoded);

  alignas(64) BN_ULONG tmp[P384_LIMBS];
  p384_elem_neg(tmp, h.Y);
  copy_conditional(h.Y, tmp, recoded_is_negative);

  nistz384_point_add(r, r, &h);
}

static void nistz384_point_mul(P384_POINT *r,
                               const BN_ULONG p_scalar[P384_LIMBS],
                               const Limb p_x[P384_LIMBS],
                               const Limb p_y[P384_LIMBS]) {
  static const size_t kWindowSize = 5;
  static const crypto_word_t kMask = (1 << (5  + 1)) - 1;

  uint8_t p_str[(P384_LIMBS * sizeof(Limb)) + 1];
  little_endian_bytes_from_scalar(p_str, sizeof(p_str) / sizeof(p_str[0]),
                                  p_scalar, P384_LIMBS);

  alignas(64) P384_POINT table[16];

  P384_POINT *row = table;

  limbs_copy(row[1 - 1].X, p_x, P384_LIMBS);
  limbs_copy(row[1 - 1].Y, p_y, P384_LIMBS);
  limbs_copy(row[1 - 1].Z, ONE, P384_LIMBS);

  nistz384_point_double(&row[2 - 1], &row[1 - 1]);
  nistz384_point_add(&row[3 - 1], &row[2 - 1], &row[1 - 1]);
  nistz384_point_double(&row[4 - 1], &row[2 - 1]);
  nistz384_point_double(&row[6 - 1], &row[3 - 1]);
  nistz384_point_double(&row[8 - 1], &row[4 - 1]);
  nistz384_point_double(&row[12 - 1], &row[6 - 1]);
  nistz384_point_add(&row[5 - 1], &row[4 - 1], &row[1 - 1]);
  nistz384_point_add(&row[7 - 1], &row[6 - 1], &row[1 - 1]);
  nistz384_point_add(&row[9 - 1], &row[8 - 1], &row[1 - 1]);
  nistz384_point_add(&row[13 - 1], &row[12 - 1], &row[1 - 1]);
  nistz384_point_double(&row[14 - 1], &row[7 - 1]);
  nistz384_point_double(&row[10 - 1], &row[5 - 1]);
  nistz384_point_add(&row[15 - 1], &row[14 - 1], &row[1 - 1]);
  nistz384_point_add(&row[11 - 1], &row[10 - 1], &row[1 - 1]);
  nistz384_point_double(&row[16 - 1], &row[8 - 1]);

  static const size_t START_INDEX = 384 - 4;
  size_t index = START_INDEX;

  BN_ULONG recoded_is_negative;
  crypto_word_t recoded;

  crypto_word_t wvalue = p_str[(index - 1) / 8];
  wvalue = (wvalue >> ((index - 1) % 8)) & kMask;

  booth_recode(&recoded_is_negative, &recoded, wvalue, 5);
  dev_assert_secret(!recoded_is_negative);

  p384_point_select_w5(r, table, recoded);

  while (index >= kWindowSize) {
    if (index != START_INDEX) {
      size_t off = (index - 1) / 8;

      wvalue = p_str[off] | p_str[off + 1] << 8;
      wvalue = (wvalue >> ((index - 1) % 8)) & kMask;
      add_precomputed_w5(r, wvalue, table);
    }

    index -= kWindowSize;

    nistz384_point_double(r, r);
    nistz384_point_double(r, r);
    nistz384_point_double(r, r);
    nistz384_point_double(r, r);
    nistz384_point_double(r, r);
  }

  wvalue = p_str[0];
  wvalue = (wvalue << 1) & kMask;
  add_precomputed_w5(r, wvalue, table);
}

void p384_point_double(Limb r[3][P384_LIMBS], const Limb a[3][P384_LIMBS])
{
  P384_POINT t;
  limbs_copy(t.X, a[0], P384_LIMBS);
  limbs_copy(t.Y, a[1], P384_LIMBS);
  limbs_copy(t.Z, a[2], P384_LIMBS);
  nistz384_point_double(&t, &t);
  limbs_copy(r[0], t.X, P384_LIMBS);
  limbs_copy(r[1], t.Y, P384_LIMBS);
  limbs_copy(r[2], t.Z, P384_LIMBS);
}

void p384_point_add(Limb r[3][P384_LIMBS],
                    const Limb a[3][P384_LIMBS],
                    const Limb b[3][P384_LIMBS])
{
  P384_POINT t1;
  limbs_copy(t1.X, a[0], P384_LIMBS);
  limbs_copy(t1.Y, a[1], P384_LIMBS);
  limbs_copy(t1.Z, a[2], P384_LIMBS);

  P384_POINT t2;
  limbs_copy(t2.X, b[0], P384_LIMBS);
  limbs_copy(t2.Y, b[1], P384_LIMBS);
  limbs_copy(t2.Z, b[2], P384_LIMBS);

  nistz384_point_add(&t1, &t1, &t2);

  limbs_copy(r[0], t1.X, P384_LIMBS);
  limbs_copy(r[1], t1.Y, P384_LIMBS);
  limbs_copy(r[2], t1.Z, P384_LIMBS);
}

void p384_point_mul(Limb r[3][P384_LIMBS], const BN_ULONG p_scalar[P384_LIMBS],
                    const Limb p_x[P384_LIMBS], const Limb p_y[P384_LIMBS]) {
  alignas(64) P384_POINT acc;
  nistz384_point_mul(&acc, p_scalar, p_x, p_y);
  limbs_copy(r[0], acc.X, P384_LIMBS);
  limbs_copy(r[1], acc.Y, P384_LIMBS);
  limbs_copy(r[2], acc.Z, P384_LIMBS);
}

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#endif

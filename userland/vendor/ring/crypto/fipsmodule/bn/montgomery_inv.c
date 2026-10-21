













#include "internal.h"
#include "../../internal.h"


OPENSSL_STATIC_ASSERT(BN_MONT_CTX_N0_LIMBS == 1 || BN_MONT_CTX_N0_LIMBS == 2,
                      "BN_MONT_CTX_N0_LIMBS value is invalid");
OPENSSL_STATIC_ASSERT(sizeof(BN_ULONG) * BN_MONT_CTX_N0_LIMBS == sizeof(uint64_t),
                      "uint64_t is insufficient precision for n0");


#define LG_LITTLE_R (BN_MONT_CTX_N0_LIMBS * BN_BITS2)























uint64_t bn_neg_inv_mod_r_u64(uint64_t n) {
  dev_assert_secret(n % 2 == 1);

  
  static const uint64_t alpha = UINT64_C(1) << (LG_LITTLE_R - 1);

  const uint64_t beta = n;

  uint64_t u = 1;
  uint64_t v = 0;

  
  
  for (size_t i = 0; i < LG_LITTLE_R; ++i) {
#if BN_BITS2 == 64 && defined(BN_ULLONG)
    dev_assert_secret((BN_ULLONG)(1) << (LG_LITTLE_R - i) ==
           ((BN_ULLONG)u * 2 * alpha) - ((BN_ULLONG)v * beta));
#endif

    
    

    uint64_t u_is_odd = UINT64_C(0) - (u & 1);  

    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    uint64_t beta_if_u_is_odd = beta & u_is_odd;  
    u = ((u ^ beta_if_u_is_odd) >> 1) + (u & beta_if_u_is_odd);

    uint64_t alpha_if_u_is_odd = alpha & u_is_odd; 
    v = (v >> 1) + alpha_if_u_is_odd;
  }

  
#if BN_BITS2 == 64 && defined(BN_ULLONG)
  declassify_assert(1 == ((BN_ULLONG)u * 2 * alpha) - ((BN_ULLONG)v * beta));
#endif

  return v;
}

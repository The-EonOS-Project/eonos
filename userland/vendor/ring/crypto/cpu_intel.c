













#include <ring-core/base.h>


#if !defined(OPENSSL_NO_ASM) && (defined(OPENSSL_X86) || defined(OPENSSL_X86_64))

#if defined(_MSC_VER) && !defined(__clang__)
#pragma warning(push, 3)
#include <immintrin.h>
#include <intrin.h>
#pragma warning(pop)
#endif

#include "internal.h"





static void OPENSSL_cpuid(uint32_t *out_eax, uint32_t *out_ebx,
                          uint32_t *out_ecx, uint32_t *out_edx, uint32_t leaf) {
#if defined(_MSC_VER) && !defined(__clang__)
  int tmp[4];
  __cpuid(tmp, (int)leaf);
  *out_eax = (uint32_t)tmp[0];
  *out_ebx = (uint32_t)tmp[1];
  *out_ecx = (uint32_t)tmp[2];
  *out_edx = (uint32_t)tmp[3];
#elif defined(__pic__) && defined(OPENSSL_32_BIT)
  
  
  __asm__ volatile (
    "xor %%ecx, %%ecx\n"
    "mov %%ebx, %%edi\n"
    "cpuid\n"
    "xchg %%edi, %%ebx\n"
    : "=a"(*out_eax), "=D"(*out_ebx), "=c"(*out_ecx), "=d"(*out_edx)
    : "a"(leaf)
  );
#else
  __asm__ volatile (
    "xor %%ecx, %%ecx\n"
    "cpuid\n"
    : "=a"(*out_eax), "=b"(*out_ebx), "=c"(*out_ecx), "=d"(*out_edx)
    : "a"(leaf)
  );
#endif
}





static uint64_t OPENSSL_xgetbv(uint32_t xcr) {
#if defined(_MSC_VER) && !defined(__clang__)
  return (uint64_t)_xgetbv(xcr);
#else
  uint32_t eax, edx;
  __asm__ volatile ("xgetbv" : "=a"(eax), "=d"(edx) : "c"(xcr));
  return (((uint64_t)edx) << 32) | eax;
#endif
}

void OPENSSL_cpuid_setup(uint32_t OPENSSL_ia32cap_P[4]) {
  
  uint32_t eax, ebx, ecx, edx;
  OPENSSL_cpuid(&eax, &ebx, &ecx, &edx, 0);

  uint32_t num_ids = eax;

  int is_intel = ebx == 0x756e6547  &&
                 edx == 0x49656e69  &&
                 ecx == 0x6c65746e ;

  uint32_t extended_features[2] = {0};
  if (num_ids >= 7) {
    OPENSSL_cpuid(&eax, &ebx, &ecx, &edx, 7);
    extended_features[0] = ebx;
    extended_features[1] = ecx;
  }

  OPENSSL_cpuid(&eax, &ebx, &ecx, &edx, 1);

  const uint32_t base_family = (eax >> 8) & 15;
  const uint32_t base_model = (eax >> 4) & 15;

  uint32_t family = base_family;
  uint32_t model = base_model;
  if (base_family == 15) {
    const uint32_t ext_family = (eax >> 20) & 255;
    family += ext_family;
  }
  if (base_family == 6 || base_family == 15) {
    const uint32_t ext_model = (eax >> 16) & 15;
    model |= ext_model << 4;
  }

  
  if (is_intel) {
    edx |= (1u << 30);
  } else {
    edx &= ~(1u << 30);
  }

  uint64_t xcr0 = 0;
  if (ecx & (1u << 27)) {
    
    xcr0 = OPENSSL_xgetbv(0);
  }
  
  if ((xcr0 & 6) != 6) {
    
    ecx &= ~(1u << 28);  
    ecx &= ~(1u << 12);  
    ecx &= ~(1u << 11);  
    extended_features[0] &= ~(1u << 5);   
    extended_features[1] &= ~(1u << 9);   
    extended_features[1] &= ~(1u << 10);  
  }
  
  
  
  if ((xcr0 & 0xe6) != 0xe6) {
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    extended_features[0] &= ~(1u << 16);  
    extended_features[0] &= ~(1u << 17);  
    extended_features[0] &= ~(1u << 21);  
    extended_features[0] &= ~(1u << 26);  
    extended_features[0] &= ~(1u << 27);  
    extended_features[0] &= ~(1u << 28);  
    extended_features[0] &= ~(1u << 30);  
    extended_features[0] &= ~(1u << 31);  
    extended_features[1] &= ~(1u << 1);   
    extended_features[1] &= ~(1u << 6);   
    extended_features[1] &= ~(1u << 11);  
    extended_features[1] &= ~(1u << 12);  
    extended_features[1] &= ~(1u << 14);  
  }

  
  
  
  
  
  
  
  
  
  
  
  
  
  if (is_intel && family == 6 &&
      (model == 85 ||    
       model == 106 ||   
       model == 108 ||   
       model == 125 ||   
       model == 126 ||   
       model == 140 ||   
       model == 141)) {  
    extended_features[0] |= 1u << 14;
  } else {
    extended_features[0] &= ~(1u << 14);
  }

  OPENSSL_ia32cap_P[0] = edx;
  OPENSSL_ia32cap_P[1] = ecx;
  OPENSSL_ia32cap_P[2] = extended_features[0];
  OPENSSL_ia32cap_P[3] = extended_features[1];
}

#endif  

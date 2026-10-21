#ifndef OPENSSL_HEADER_ASM_BASE_H
#define OPENSSL_HEADER_ASM_BASE_H

#include <ring-core/target.h>

#if defined(__ASSEMBLER__)

#include <ring_core_generated/prefix_symbols_asm.h>

#if defined(__ELF__)

.pushsection .note.GNU-stack, "", %progbits
.popsection
#endif

#if defined(__CET__) && defined(OPENSSL_X86_64)

#include <cet.h>
#else
#define _CET_ENDBR
#endif

#if defined(OPENSSL_ARM) || defined(OPENSSL_AARCH64)

#if !defined(__ARM_ARCH)
#error "ARM assembler must define __ARM_ARCH"
#endif

#define __ARM_MAX_ARCH__ 8

#if defined(__ARM_FEATURE_BTI_DEFAULT) && __ARM_FEATURE_BTI_DEFAULT == 1
#define GNU_PROPERTY_AARCH64_BTI (1 << 0)   
#define AARCH64_VALID_CALL_TARGET hint #34  
#else
#define GNU_PROPERTY_AARCH64_BTI 0  
#define AARCH64_VALID_CALL_TARGET
#endif

#if defined(__ARM_FEATURE_PAC_DEFAULT) && \
    (__ARM_FEATURE_PAC_DEFAULT & 1) == 1  
#define GNU_PROPERTY_AARCH64_POINTER_AUTH \
  (1 << 1)                                       
#define AARCH64_SIGN_LINK_REGISTER hint #25      
#define AARCH64_VALIDATE_LINK_REGISTER hint #29  
#elif defined(__ARM_FEATURE_PAC_DEFAULT) && \
    (__ARM_FEATURE_PAC_DEFAULT & 2) == 2  
#define GNU_PROPERTY_AARCH64_POINTER_AUTH \
  (1 << 1)                                       
#define AARCH64_SIGN_LINK_REGISTER hint #27      
#define AARCH64_VALIDATE_LINK_REGISTER hint #31  
#else
#define GNU_PROPERTY_AARCH64_POINTER_AUTH 0  
#if GNU_PROPERTY_AARCH64_BTI != 0
#define AARCH64_SIGN_LINK_REGISTER AARCH64_VALID_CALL_TARGET
#else
#define AARCH64_SIGN_LINK_REGISTER
#endif
#define AARCH64_VALIDATE_LINK_REGISTER
#endif

#if GNU_PROPERTY_AARCH64_POINTER_AUTH != 0 || GNU_PROPERTY_AARCH64_BTI != 0
.pushsection .note.gnu.property, "a";
.balign 8;
.long 4;
.long 0x10;
.long 0x5;
.asciz "GNU";
.long 0xc0000000; 
.long 4;
.long (GNU_PROPERTY_AARCH64_POINTER_AUTH | GNU_PROPERTY_AARCH64_BTI);
.long 0;
.popsection;
#endif
#endif  

#endif  

#endif  

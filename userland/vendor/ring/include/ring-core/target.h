#ifndef OPENSSL_HEADER_TARGET_H
#define OPENSSL_HEADER_TARGET_H

#if defined(__x86_64) || defined(_M_AMD64) || defined(_M_X64)
#define OPENSSL_64_BIT
#define OPENSSL_X86_64
#elif defined(__x86) || defined(__i386) || defined(__i386__) || defined(_M_IX86)
#define OPENSSL_32_BIT
#define OPENSSL_X86
#elif defined(__AARCH64EL__) || defined(_M_ARM64)
#define OPENSSL_64_BIT
#define OPENSSL_AARCH64
#elif defined(__ARMEL__) || defined(_M_ARM)
#define OPENSSL_32_BIT
#define OPENSSL_ARM

#elif !defined(__BYTE_ORDER__)
#error "Cannot determine endianness because __BYTE_ORDER__ is not defined"

#elif !(defined(__ORDER_LITTLE_ENDIAN__) && (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)) && \
      !(defined(__ORDER_BIG_ENDIAN__) && (__BYTE_ORDER__ == __ORDER_BIG_ENDIAN__))
#error "Unsupported endianness"
#elif defined(__LP64__)
#define OPENSSL_64_BIT
#elif defined(__ILP32__)
#define OPENSSL_32_BIT

#elif defined(__MIPSEL__) || defined(__MIPSEB__) || defined(__PPC__) || defined(__powerpc__) || defined(__csky__) || defined(__XTENSA__)
#define OPENSSL_32_BIT
#else
#error "Unknown target CPU"
#endif

#if defined(__APPLE__)
#define OPENSSL_APPLE
#endif

#if defined(_WIN32)
#define OPENSSL_WINDOWS
#endif

#if defined(__has_feature)
#if __has_feature(address_sanitizer)
#define OPENSSL_ASAN
#endif
#if __has_feature(thread_sanitizer)
#define OPENSSL_TSAN
#endif
#if __has_feature(memory_sanitizer)
#define OPENSSL_MSAN
#define OPENSSL_ASM_INCOMPATIBLE
#endif
#if __has_feature(hwaddress_sanitizer)
#define OPENSSL_HWASAN
#endif
#endif

#if defined(OPENSSL_APPLE) && defined(OPENSSL_ARM)
#define OPENSSL_ASM_INCOMPATIBLE
#endif

#if defined(OPENSSL_ASM_INCOMPATIBLE)
#undef OPENSSL_ASM_INCOMPATIBLE
#if !defined(OPENSSL_NO_ASM)
#define OPENSSL_NO_ASM
#endif
#endif  

#if !defined(OPENSSL_X86_64) && !defined(OPENSSL_AARCH64)
#define OPENSSL_SMALL
#endif

#endif  

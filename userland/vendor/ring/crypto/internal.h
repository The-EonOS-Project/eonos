













#ifndef OPENSSL_HEADER_CRYPTO_INTERNAL_H
#define OPENSSL_HEADER_CRYPTO_INTERNAL_H

#include <ring-core/base.h> 

#include "ring-core/check.h"

#if defined(__clang__)


#pragma GCC diagnostic ignored "-Wmissing-prototypes"
#endif

#if defined(__GNUC__) && \
    (__GNUC__ * 10000 + __GNUC_MINOR__ * 100 + __GNUC_PATCHLEVEL__) < 40800



#define alignas(x) __attribute__ ((aligned (x)))
#elif defined(_MSC_VER) && !defined(__clang__)
#define alignas(x) __declspec(align(x))
#else
#include <stdalign.h>
#endif

#if defined(__clang__) || defined(__GNUC__)
#define RING_NOINLINE __attribute__((noinline))
#elif defined(_MSC_VER)
#define RING_NOINLINE __declspec(noinline)
#else
#define RING_NOINLINE
#endif




#if defined(__clang__) || defined(_MSC_VER)
#define RING_CORE_POINTLESS_ARRAY_CONST_CAST(cast)
#else
#define RING_CORE_POINTLESS_ARRAY_CONST_CAST(cast) cast
#endif



typedef unsigned char aliasing_uint8_t;

#if (!defined(_MSC_VER) || defined(__clang__)) && defined(OPENSSL_64_BIT)
#define BORINGSSL_HAS_UINT128
typedef __int128_t int128_t;
typedef __uint128_t uint128_t;
#endif




#if defined(OPENSSL_X86) || defined(OPENSSL_X86_64)
# if defined(_MSC_VER) && !defined(__clang__)
#  if defined(_M_AMD64) || defined(_M_X64) || (defined(_M_IX86_FP) && _M_IX86_FP >= 2)
#   define OPENSSL_SSE2
#  else
#   error "SSE2 is required."
#  endif
# elif !defined(__SSE2__)
#  error "SSE2 is required."
# endif
#endif








#if defined(OPENSSL_SSE2) && defined(OPENSSL_NO_SSE2_FOR_TESTING)
#undef OPENSSL_SSE2
#endif




static inline int buffers_alias(const void *a, size_t a_bytes,
                                const void *b, size_t b_bytes) {
  
  
  
  
  uintptr_t a_u = (uintptr_t)a;
  uintptr_t b_u = (uintptr_t)b;
  return a_u + a_bytes > b_u && b_u + b_bytes > a_u;
}



















#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wconversion"
#pragma GCC diagnostic ignored "-Wsign-conversion"
#endif
#if defined(_MSC_VER) && !defined(__clang__)
#pragma warning(push)

#pragma warning(disable: 4242)

#pragma warning(disable: 4244)
#endif







#if defined(OPENSSL_64_BIT)
typedef uint64_t crypto_word_t;
#define CRYPTO_WORD_BITS (64u)
#elif defined(OPENSSL_32_BIT)
typedef uint32_t crypto_word_t;
#define CRYPTO_WORD_BITS (32u)
#else
#error "Must define either OPENSSL_32_BIT or OPENSSL_64_BIT"
#endif

#define CONSTTIME_TRUE_W ~((crypto_word_t)0)
#define CONSTTIME_FALSE_W ((crypto_word_t)0)








static inline crypto_word_t value_barrier_w(crypto_word_t a) {
#if defined(__GNUC__) || defined(__clang__)
  __asm__("" : "+r"(a) : );
#endif
  return a;
}


static inline uint32_t value_barrier_u32(uint32_t a) {
#if defined(__GNUC__) || defined(__clang__)
  __asm__("" : "+r"(a) : );
#endif
  return a;
}






static inline crypto_word_t constant_time_msb_w(crypto_word_t a) {
  return 0u - (a >> (sizeof(a) * 8 - 1));
}


static inline crypto_word_t constant_time_is_zero_w(crypto_word_t a) {
  
  
  
  
  
  
  
  
  
  
  
  return constant_time_msb_w(~a & (a - 1));
}

static inline crypto_word_t constant_time_is_nonzero_w(crypto_word_t a) {
  return ~constant_time_is_zero_w(a);
}


static inline crypto_word_t constant_time_eq_w(crypto_word_t a,
                                               crypto_word_t b) {
  return constant_time_is_zero_w(a ^ b);
}




static inline crypto_word_t constant_time_select_w(crypto_word_t mask,
                                                   crypto_word_t a,
                                                   crypto_word_t b) {
  
  
  
  
  
  mask = value_barrier_w(mask);
  return (mask & a) | (~mask & b);
}



static inline uint8_t constant_time_select_8(crypto_word_t mask, uint8_t a,
                                             uint8_t b) {
  
  
  
  
  
  
  uint8_t m = value_barrier_w(mask);
  return (m & a) | (~m & b);
}




static inline void constant_time_conditional_memcpy(void *dst, const void *src,
                                                    const size_t n,
                                                    const crypto_word_t mask) {
  debug_assert_nonsecret(!buffers_alias(dst, n, src, n));
  uint8_t *out = (uint8_t *)dst;
  const uint8_t *in = (const uint8_t *)src;
  for (size_t i = 0; i < n; i++) {
    out[i] = constant_time_select_8(mask, in[i], out[i]);
  }
}




static inline void constant_time_conditional_memxor(void *dst, const void *src,
                                                    size_t n,
                                                    const crypto_word_t mask) {
  debug_assert_nonsecret(!buffers_alias(dst, n, src, n));
  aliasing_uint8_t *out = dst;
  const aliasing_uint8_t *in = src;
#if defined(__GNUC__) && !defined(__clang__)
  
  typedef aliasing_uint8_t v32u8 __attribute__((vector_size(32), aligned(1), may_alias));
  size_t n_vec = n&~(size_t)31;
  v32u8 masks = ((aliasing_uint8_t)mask-(v32u8){}); 
  for (size_t i = 0; i < n_vec; i += 32) {
    *(v32u8*)&out[i] ^= masks & *(v32u8 const*)&in[i];
  }
  out += n_vec;
  n -= n_vec;
#endif
  for (size_t i = 0; i < n; i++) {
    out[i] ^= value_barrier_w(mask) & in[i];
  }
}

#if defined(BORINGSSL_CONSTANT_TIME_VALIDATION)





#define CONSTTIME_SECRET(ptr, len) VALGRIND_MAKE_MEM_UNDEFINED(ptr, len)




#define CONSTTIME_DECLASSIFY(ptr, len) VALGRIND_MAKE_MEM_DEFINED(ptr, len)

#else

#define CONSTTIME_SECRET(ptr, len)
#define CONSTTIME_DECLASSIFY(ptr, len)

#endif  

static inline crypto_word_t constant_time_declassify_w(crypto_word_t v) {
  
  
  
  
  
  
  
  
  
  
  
  CONSTTIME_DECLASSIFY(&v, sizeof(v));
  return value_barrier_w(v);
}

static inline int constant_time_declassify_int(int v) {
  OPENSSL_STATIC_ASSERT(sizeof(uint32_t) == sizeof(int),
                "int is not the same size as uint32_t");
  
  CONSTTIME_DECLASSIFY(&v, sizeof(v));
  return value_barrier_u32((uint32_t)v);
}

#if defined(_MSC_VER) && !defined(__clang__)

#pragma warning(pop)
#endif
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#endif





#define declassify_assert(expr) dev_assert_secret(constant_time_declassify_int(expr))



#if defined(__GNUC__) && __GNUC__ >= 2
static inline uint32_t CRYPTO_bswap4(uint32_t x) {
  return __builtin_bswap32(x);
}

static inline uint64_t CRYPTO_bswap8(uint64_t x) {
  return __builtin_bswap64(x);
}
#elif defined(_MSC_VER)
#pragma warning(push, 3)
#include <stdlib.h>
#pragma warning(pop)
#pragma intrinsic(_byteswap_ulong)
static inline uint32_t CRYPTO_bswap4(uint32_t x) {
  return _byteswap_ulong(x);
}
#endif

#if !defined(RING_CORE_NOSTDLIBINC)
#include <string.h>
#endif

static inline void *OPENSSL_memcpy(void *dst, const void *src, size_t n) {
#if !defined(RING_CORE_NOSTDLIBINC)
  if (n == 0) {
    return dst;
  }
  return memcpy(dst, src, n);
#else
  aliasing_uint8_t *d = dst;
  const aliasing_uint8_t *s = src;
  for (size_t i = 0; i < n; ++i) {
    d[i] = s[i];
  }
  return dst;
#endif
}

static inline void *OPENSSL_memset(void *dst, int c, size_t n) {
#if !defined(RING_CORE_NOSTDLIBINC)
  if (n == 0) {
    return dst;
  }
  return memset(dst, c, n);
#else
  aliasing_uint8_t *d = dst;
  for (size_t i = 0; i < n; ++i) {
    d[i] = (aliasing_uint8_t)c;
  }
  return dst;
#endif
}








#if defined(__BYTE_ORDER__) && defined(__ORDER_BIG_ENDIAN__)
#if __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define RING_BIG_ENDIAN
#endif
#endif

static inline uint32_t CRYPTO_load_u32_le(const void *in) {
  uint32_t v;
  OPENSSL_memcpy(&v, in, sizeof(v));
#if defined(RING_BIG_ENDIAN)
  return CRYPTO_bswap4(v);
#else
  return v;
#endif
}

static inline void CRYPTO_store_u32_le(void *out, uint32_t v) {
#if defined(RING_BIG_ENDIAN)
  v = CRYPTO_bswap4(v);
#endif
  OPENSSL_memcpy(out, &v, sizeof(v));
}

static inline uint32_t CRYPTO_load_u32_be(const void *in) {
  uint32_t v;
  OPENSSL_memcpy(&v, in, sizeof(v));
#if !defined(RING_BIG_ENDIAN)
  return CRYPTO_bswap4(v);
#else
  return v;
#endif
}

static inline void CRYPTO_store_u32_be(void *out, uint32_t v) {
#if !defined(RING_BIG_ENDIAN)
  v = CRYPTO_bswap4(v);
#endif
  OPENSSL_memcpy(out, &v, sizeof(v));
}



#if defined(OPENSSL_X86) || defined(OPENSSL_X86_64)


















#if defined(OPENSSL_X86_64)
extern uint32_t avx2_available;
extern uint32_t adx_bmi2_available;
#endif
#endif


#if defined(OPENSSL_ARM)
extern alignas(4) uint32_t neon_available;
#endif  

#endif  

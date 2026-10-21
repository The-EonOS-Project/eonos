













#ifndef OPENSSL_HEADER_BASE_H
#define OPENSSL_HEADER_BASE_H




#if defined(_MSC_VER) && !defined(__clang__)
#pragma warning(push, 3)
#endif

#include <stddef.h>
#include <stdint.h>

#if defined(_MSC_VER) && !defined(__clang__)
#pragma warning(pop)
#endif

#if defined(__APPLE__)
#include <TargetConditionals.h>
#endif

#include <ring-core/target.h>  

#include <ring_core_generated/prefix_symbols.h>

#include <ring-core/type_check.h>

#if defined(__APPLE__)


#if defined(TARGET_OS_OSX) && TARGET_OS_OSX
#define OPENSSL_MACOS
#endif
#if defined(TARGET_OS_IPHONE) && TARGET_OS_IPHONE
#define OPENSSL_IOS
#endif
#endif




#define OPENSSL_EXPORT



OPENSSL_STATIC_ASSERT(sizeof(int32_t) == sizeof(int), "int isn't 32 bits.");
OPENSSL_STATIC_ASSERT(sizeof(uint32_t) == sizeof(unsigned int), "unsigned int isn't 32 bits.");
OPENSSL_STATIC_ASSERT(sizeof(size_t) == sizeof(uintptr_t), "uintptr_t and size_t differ.");
OPENSSL_STATIC_ASSERT(sizeof(size_t) <= sizeof(uint64_t), "size_t is larger than uint64_t.");
OPENSSL_STATIC_ASSERT(sizeof(size_t) >= sizeof(uint32_t), "size_t is smaller than uint32_t.");

#endif  

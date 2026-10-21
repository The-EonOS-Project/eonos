













#ifndef OPENSSL_HEADER_TYPE_CHECK_H
#define OPENSSL_HEADER_TYPE_CHECK_H

#include <ring-core/base.h>


#if defined(__cplusplus) || (defined(_MSC_VER) && !defined(__clang__))

#define OPENSSL_STATIC_ASSERT(cond, msg) static_assert(cond, msg)
#else




#define OPENSSL_STATIC_ASSERT(cond, msg) _Static_assert(cond, msg)
#endif

#endif  

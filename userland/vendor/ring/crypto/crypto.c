













#include <ring-core/base.h>






#if defined(OPENSSL_WINDOWS)
#define HIDDEN
#else
#define HIDDEN __attribute__((visibility("hidden")))
#endif

#if defined(OPENSSL_X86_64)

HIDDEN uint32_t avx2_available = 0;
HIDDEN uint32_t adx_bmi2_available = 0;
#elif defined(OPENSSL_ARM)
HIDDEN uint32_t neon_available = 0;
#endif

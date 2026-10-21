#ifndef RING_CHECK_H
#define RING_CHECK_H

#if !defined(RING_CORE_NOSTDLIBINC)
# include <assert.h>
# define debug_assert_nonsecret(x) assert(x)
#else
# if !defined(NDEBUG)
#  define debug_assert_nonsecret(x) ((x) ? ((void)0) : __builtin_trap())
# else
#  define debug_assert_nonsecret(x) ((void)0)
# endif
#endif

#if 0 

#else
# define dev_assert_secret(x) ((void)0)
#endif

#endif 

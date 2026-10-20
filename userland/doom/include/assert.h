#ifndef EONOS_DOOM_ASSERT_H
#define EONOS_DOOM_ASSERT_H
#include <stdio.h>
#include <stdlib.h>
#define assert(condition) ((condition) ? (void)0 : (fprintf(stderr, "assertion failed: %s (%s:%d)\n", #condition, __FILE__, __LINE__), abort()))
#endif

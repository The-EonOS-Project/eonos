#ifndef EONOS_DOOM_SYS_STAT_H
#define EONOS_DOOM_SYS_STAT_H
#include <sys/types.h>
struct stat { off_t st_size; unsigned int st_mode; };
int stat(const char *path, struct stat *result);
int mkdir(const char *path, mode_t mode);
#endif

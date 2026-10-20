#ifndef EONOS_DOOM_UNISTD_H
#define EONOS_DOOM_UNISTD_H
#include <stddef.h>
#include <sys/types.h>
ssize_t read(int fd, void *buffer, size_t count);
ssize_t write(int fd, const void *buffer, size_t count);
int close(int fd);
int access(const char *path, int mode);
int unlink(const char *path);
unsigned int sleep(unsigned int seconds);
int usleep(unsigned int microseconds);
int isatty(int fd);
#endif

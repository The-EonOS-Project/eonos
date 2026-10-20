#ifndef EONOS_DOOM_STDIO_H
#define EONOS_DOOM_STDIO_H
#include <stddef.h>
#include <stdarg.h>
typedef struct EonosFile FILE;
#define EOF (-1)
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2
#define BUFSIZ 1024
struct EonosFile { int fd; long position; int error; int eof; int owned; };
extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;
FILE *fopen(const char *path, const char *mode);
int fclose(FILE *stream);
size_t fread(void *buffer, size_t size, size_t count, FILE *stream);
size_t fwrite(const void *buffer, size_t size, size_t count, FILE *stream);
int fseek(FILE *stream, long offset, int whence);
long ftell(FILE *stream);
void rewind(FILE *stream);
int fflush(FILE *stream);
int ferror(FILE *stream);
int feof(FILE *stream);
void clearerr(FILE *stream);
int fileno(FILE *stream);
int fgetc(FILE *stream);
int getc(FILE *stream);
int getchar(void);
char *fgets(char *buffer, int size, FILE *stream);
int fputc(int byte, FILE *stream);
int putchar(int byte);
int puts(const char *text);
int vfprintf(FILE *stream, const char *format, va_list args);
int fprintf(FILE *stream, const char *format, ...);
int vprintf(const char *format, va_list args);
int printf(const char *format, ...);
int vsnprintf(char *buffer, size_t size, const char *format, va_list args);
int snprintf(char *buffer, size_t size, const char *format, ...);
int vsprintf(char *buffer, const char *format, va_list args);
int sprintf(char *buffer, const char *format, ...);
int sscanf(const char *input, const char *format, ...);
void perror(const char *text);
int remove(const char *path);
int rename(const char *old_path, const char *new_path);
#endif

/* Minimal freestanding C runtime for upstream doomgeneric on EonOS.
 * File and process services cross into the Rust runtime through eonos_*.
 */
#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <math.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <string.h>
#include <unistd.h>

int errno;

extern void *eonos_doom_alloc(size_t size);
extern void eonos_doom_free(void *pointer);
extern long eonos_doom_open(const char *path, int flags);
extern long eonos_doom_read(int fd, void *buffer, size_t count);
extern long eonos_doom_write(int fd, const void *buffer, size_t count);
extern long eonos_doom_seek(int fd, long offset, int whence);
extern long eonos_doom_close(int fd);
extern void eonos_doom_exit(int status) __attribute__((noreturn));

static FILE std_in = { 0, 0, 0, 0, 0 };
static FILE std_out = { 1, 0, 0, 0, 0 };
static FILE std_err = { 2, 0, 0, 0, 0 };
FILE *stdin = &std_in;
FILE *stdout = &std_out;
FILE *stderr = &std_err;

static void (*exit_functions[32])(void);
static size_t exit_function_count;

void *malloc(size_t size)
{
    size_t total;
    size_t *header;
    if (size == 0) size = 1;
    if (size > (size_t)-1 - 2 * sizeof(size_t)) return NULL;
    total = size + 2 * sizeof(size_t);
    header = (size_t *)eonos_doom_alloc(total);
    if (header == NULL) return NULL;
    *header = size;
    return header + 2;
}

void free(void *pointer)
{
    if (pointer != NULL) eonos_doom_free((size_t *)pointer - 2);
}

void *calloc(size_t count, size_t size)
{
    size_t bytes;
    unsigned char *result;
    size_t i;
    if (size != 0 && count > (size_t)-1 / size) return NULL;
    bytes = count * size;
    result = (unsigned char *)malloc(bytes);
    if (result != NULL) for (i = 0; i < bytes; ++i) result[i] = 0;
    return result;
}

void *realloc(void *pointer, size_t size)
{
    size_t old_size;
    size_t *header;
    void *next;
    if (pointer == NULL) return malloc(size);
    if (size == 0) { free(pointer); return NULL; }
    if (size > (size_t)-1 - 2 * sizeof(size_t)) return NULL;
    header = (size_t *)pointer - 2;
    old_size = *header;
    next = malloc(size);
    if (next == NULL) return NULL;
    memcpy(next, pointer, old_size < size ? old_size : size);
    free(pointer);
    return next;
}

int atexit(void (*function)(void))
{
    if (function == NULL || exit_function_count == 32) return -1;
    exit_functions[exit_function_count++] = function;
    return 0;
}

void exit(int status)
{
    while (exit_function_count != 0) exit_functions[--exit_function_count]();
    eonos_doom_exit(status);
}

void abort(void) { exit(134); }
char *getenv(const char *name) { (void)name; return NULL; }
int system(const char *command) { (void)command; errno = ENOSYS; return -1; }

static int digit_value(unsigned char c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'z') return c - 'a' + 10;
    if (c >= 'A' && c <= 'Z') return c - 'A' + 10;
    return -1;
}

unsigned long long strtoull(const char *text, char **end, int base)
{
    const char *p = text;
    unsigned long long value = 0;
    int negative = 0;
    int digit;
    while (isspace((unsigned char)*p)) ++p;
    if (*p == '+' || *p == '-') { negative = *p == '-'; ++p; }
    if (base == 0) {
        if (p[0] == '0' && (p[1] == 'x' || p[1] == 'X')) { base = 16; p += 2; }
        else base = p[0] == '0' ? 8 : 10;
    } else if (base == 16 && p[0] == '0' && (p[1] == 'x' || p[1] == 'X')) p += 2;
    if (base < 2 || base > 36) { if (end) *end = (char *)text; errno = EINVAL; return 0; }
    while ((digit = digit_value((unsigned char)*p)) >= 0 && digit < base) {
        if (value > (~0ULL - (unsigned)digit) / (unsigned)base) errno = ERANGE;
        else value = value * (unsigned)base + (unsigned)digit;
        ++p;
    }
    if (end) *end = (char *)(p == text ? text : p);
    return negative ? 0ULL - value : value;
}

long long strtoll(const char *text, char **end, int base) { return (long long)strtoull(text, end, base); }
unsigned long strtoul(const char *text, char **end, int base) { return (unsigned long)strtoull(text, end, base); }
long strtol(const char *text, char **end, int base) { return (long)strtoll(text, end, base); }
int atoi(const char *text) { return (int)strtol(text, NULL, 10); }
long atol(const char *text) { return strtol(text, NULL, 10); }
double atof(const char *text) { const char *p=text; double value=0.0,scale=0.1; int negative=0,exponent=0,exp_negative=0; while(isspace((unsigned char)*p))++p; if(*p=='+'||*p=='-'){negative=*p=='-';++p;} while(isdigit((unsigned char)*p)){value=value*10.0+(*p++-'0');} if(*p=='.'){++p;while(isdigit((unsigned char)*p)){value+=(*p++-'0')*scale;scale*=0.1;}} if(*p=='e'||*p=='E'){++p;if(*p=='+'||*p=='-'){exp_negative=*p=='-';++p;}while(isdigit((unsigned char)*p)){if(exponent<100)exponent=exponent*10+(*p-'0');++p;}while(exponent--)value=exp_negative?value/10.0:value*10.0;}return negative?-value:value; }
int abs(int value) { return value < 0 ? -value : value; }
long labs(long value) { return value < 0 ? -value : value; }
static unsigned int random_state = 1;
int rand(void) { random_state = random_state * 1103515245u + 12345u; return (int)((random_state >> 16) & 0x7fff); }
void srand(unsigned int seed) { random_state = seed; }

void qsort(void *base, size_t count, size_t size, int (*compare)(const void *, const void *))
{
    unsigned char *bytes = (unsigned char *)base;
    size_t i, j;
    unsigned char *tmp;
    if (size == 0 || count < 2 || compare == NULL) return;
    tmp = (unsigned char *)malloc(size);
    if (tmp == NULL) return;
    for (i = 1; i < count; ++i) {
        memcpy(tmp, bytes + i * size, size);
        j = i;
        while (j > 0 && compare(bytes + (j - 1) * size, tmp) > 0) {
            memmove(bytes + j * size, bytes + (j - 1) * size, size);
            --j;
        }
        memcpy(bytes + j * size, tmp, size);
    }
    free(tmp);
}

void *memcpy(void *destination, const void *source, size_t count)
{
    unsigned char *d = (unsigned char *)destination;
    const unsigned char *s = (const unsigned char *)source;
    size_t i;
    for (i = 0; i < count; ++i) d[i] = s[i];
    return destination;
}

void *memmove(void *destination, const void *source, size_t count)
{
    unsigned char *d = (unsigned char *)destination;
    const unsigned char *s = (const unsigned char *)source;
    size_t i;
    if (d < s) for (i = 0; i < count; ++i) d[i] = s[i];
    else if (d > s) for (i = count; i != 0; --i) d[i - 1] = s[i - 1];
    return destination;
}

void *memset(void *destination, int value, size_t count)
{
    unsigned char *d = (unsigned char *)destination;
    size_t i;
    for (i = 0; i < count; ++i) d[i] = (unsigned char)value;
    return destination;
}

int memcmp(const void *left, const void *right, size_t count)
{
    const unsigned char *a = (const unsigned char *)left, *b = (const unsigned char *)right;
    size_t i;
    for (i = 0; i < count; ++i) if (a[i] != b[i]) return (int)a[i] - (int)b[i];
    return 0;
}

void *memchr(const void *memory, int value, size_t count)
{
    const unsigned char *p = (const unsigned char *)memory;
    size_t i;
    for (i = 0; i < count; ++i) if (p[i] == (unsigned char)value) return (void *)(p + i);
    return NULL;
}

size_t strlen(const char *text) { size_t n = 0; while (text[n]) ++n; return n; }
size_t strnlen(const char *text, size_t maxlen) { size_t n = 0; while (n < maxlen && text[n]) ++n; return n; }
char *strcpy(char *d, const char *s) { char *r = d; while ((*d++ = *s++) != 0) {} return r; }
char *strncpy(char *d, const char *s, size_t n) { size_t i = 0; for (; i < n && s[i]; ++i) d[i] = s[i]; for (; i < n; ++i) d[i] = 0; return d; }
char *strcat(char *d, const char *s) { strcpy(d + strlen(d), s); return d; }
char *strncat(char *d, const char *s, size_t n) { size_t at = strlen(d), i = 0; while (i < n && s[i]) { d[at + i] = s[i]; ++i; } d[at + i] = 0; return d; }
int strcmp(const char *a, const char *b) { while (*a && *a == *b) { ++a; ++b; } return (unsigned char)*a - (unsigned char)*b; }
int strncmp(const char *a, const char *b, size_t n) { size_t i; for (i = 0; i < n; ++i) { unsigned char x=a[i], y=b[i]; if (x != y || !x) return x-y; } return 0; }
int strcasecmp(const char *a,const char *b) { while(*a && tolower((unsigned char)*a)==tolower((unsigned char)*b)){++a;++b;}return tolower((unsigned char)*a)-tolower((unsigned char)*b); }
int strncasecmp(const char *a,const char *b,size_t n) { size_t i;for(i=0;i<n;++i){int x=tolower((unsigned char)a[i]),y=tolower((unsigned char)b[i]);if(x!=y||!x)return x-y;}return 0; }
char *strchr(const char *s, int c) { do { if (*s == (char)c) return (char *)s; } while (*s++); return NULL; }
char *strrchr(const char *s, int c) { const char *last = NULL; do { if (*s == (char)c) last = s; } while (*s++); return (char *)last; }
char *strstr(const char *s, const char *n) { size_t len = strlen(n); if (!len) return (char *)s; for (; *s; ++s) if (*s == *n && strncmp(s, n, len) == 0) return (char *)s; return NULL; }
static char *token_cursor;
char *strtok(char *s, const char *delims) { char *start; if (!s) s = token_cursor; if (!s) return NULL; while (*s && strchr(delims, *s)) ++s; if (!*s) { token_cursor = NULL; return NULL; } start = s; while (*s && !strchr(delims, *s)) ++s; if (*s) *s++ = 0; token_cursor = s; return start; }
size_t strspn(const char *s, const char *a) { size_t n=0; while (s[n] && strchr(a,s[n])) ++n; return n; }
size_t strcspn(const char *s, const char *r) { size_t n=0; while (s[n] && !strchr(r,s[n])) ++n; return n; }
char *strpbrk(const char *s, const char *a) { for (; *s; ++s) if (strchr(a,*s)) return (char *)s; return NULL; }
char *strerror(int error) { (void)error; return "EonOS system error"; }
char *strdup(const char *text) { size_t n=strlen(text)+1; char *copy=(char *)malloc(n); if(copy)memcpy(copy,text,n); return copy; }

int isalnum(int c) { return isalpha(c) || isdigit(c); }
int isalpha(int c) { return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z'); }
int iscntrl(int c) { return (unsigned char)c < 32 || (unsigned char)c == 127; }
int isdigit(int c) { return c >= '0' && c <= '9'; }
int isgraph(int c) { return c > 32 && c < 127; }
int islower(int c) { return c >= 'a' && c <= 'z'; }
int isprint(int c) { return c >= 32 && c < 127; }
int ispunct(int c) { return isprint(c) && !isspace(c) && !isalnum(c); }
int isspace(int c) { return c == ' ' || (c >= '\t' && c <= '\r'); }
int isupper(int c) { return c >= 'A' && c <= 'Z'; }
int isxdigit(int c) { return isdigit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'); }
int tolower(int c) { return isupper(c) ? c + ('a' - 'A') : c; }
int toupper(int c) { return islower(c) ? c - ('a' - 'A') : c; }

ssize_t read(int fd, void *buffer, size_t count) { long r=eonos_doom_read(fd,buffer,count); if (r<0) { errno=(int)-r; return -1; } return (ssize_t)r; }
ssize_t write(int fd, const void *buffer, size_t count) { long r=eonos_doom_write(fd,buffer,count); if (r<0) { errno=(int)-r; return -1; } return (ssize_t)r; }
int close(int fd) { long r=eonos_doom_close(fd); if (r<0) { errno=(int)-r; return -1; } return 0; }
int access(const char *path, int mode) { (void)mode; long fd=eonos_doom_open(path,O_RDONLY); if (fd<0) { errno=(int)-fd; return -1; } close((int)fd); return 0; }
int unlink(const char *path) { (void)path; errno=EROFS; return -1; }
int stat(const char *path, struct stat *result) { (void)path; (void)result; errno=ENOENT; return -1; }
int mkdir(const char *path, mode_t mode) { (void)path; (void)mode; errno=EROFS; return -1; }
int remove(const char *path) { return unlink(path); }
int rename(const char *old_path,const char *new_path) { (void)old_path;(void)new_path;errno=EROFS;return -1; }
unsigned int sleep(unsigned int seconds) { (void)seconds; errno=ENOSYS; return 0; }
int usleep(unsigned int microseconds) { (void)microseconds; errno=ENOSYS; return -1; }
int isatty(int fd) { return fd >= 0 && fd <= 2; }

FILE *fopen(const char *path, const char *mode)
{
    int flags;
    long fd;
    FILE *stream;
    if (!path || !mode) { errno=EINVAL; return NULL; }
    if (mode[0]=='r') flags=O_RDONLY;
    else if (mode[0]=='w') flags=O_WRONLY|O_CREAT|O_TRUNC;
    else if (mode[0]=='a') flags=O_WRONLY|O_CREAT|O_APPEND;
    else { errno=EINVAL; return NULL; }
    fd=eonos_doom_open(path,flags);
    if (fd<0) { errno=(int)-fd; return NULL; }
    stream=(FILE *)malloc(sizeof(FILE));
    if (!stream) { eonos_doom_close((int)fd); errno=ENOMEM; return NULL; }
    stream->fd=(int)fd; stream->position=0; stream->error=0; stream->eof=0; stream->owned=1;
    return stream;
}

int fclose(FILE *stream)
{
    int result=0;
    if (!stream) { errno=EINVAL; return EOF; }
    if (stream->owned) { if (close(stream->fd)<0) result=EOF; free(stream); }
    return result;
}

size_t fread(void *buffer, size_t size, size_t count, FILE *stream)
{
    size_t total, done=0;
    if (!stream || (!buffer && size && count)) { errno=EINVAL; return 0; }
    if (size && count > (size_t)-1 / size) { stream->error=1; errno=EINVAL; return 0; }
    total=size*count;
    while (done<total) {
        ssize_t n=read(stream->fd,(unsigned char *)buffer+done,total-done);
        if (n<0) { stream->error=1; break; }
        if (n==0) { stream->eof=1; break; }
        done+=(size_t)n; stream->position+=(long)n;
    }
    return size ? done/size : 0;
}

size_t fwrite(const void *buffer, size_t size, size_t count, FILE *stream)
{
    size_t total, done=0;
    if (!stream || (!buffer && size && count)) { errno=EINVAL; return 0; }
    if (size && count > (size_t)-1 / size) { stream->error=1; errno=EINVAL; return 0; }
    total=size*count;
    while (done<total) {
        ssize_t n=write(stream->fd,(const unsigned char *)buffer+done,total-done);
        if (n<0) { stream->error=1; break; }
        if (n==0) { stream->error=1; errno=EIO; break; }
        done+=(size_t)n; stream->position+=(long)n;
    }
    return size ? done/size : 0;
}

int fseek(FILE *stream, long offset, int whence)
{
    long position;
    if (!stream || whence<SEEK_SET || whence>SEEK_END) { errno=EINVAL; return -1; }
    position=eonos_doom_seek(stream->fd,offset,whence);
    if (position<0) { errno=(int)-position; stream->error=1; return -1; }
    stream->position=position; stream->eof=0; return 0;
}
long ftell(FILE *stream) { if (!stream) { errno=EINVAL; return -1; } return stream->position; }
void rewind(FILE *stream) { if (fseek(stream,0,SEEK_SET)==0) stream->error=0; }
int fflush(FILE *stream) { (void)stream; return 0; }
int ferror(FILE *stream) { return stream ? stream->error : 1; }
int feof(FILE *stream) { return stream ? stream->eof : 0; }
void clearerr(FILE *stream) { if (stream) { stream->error=0; stream->eof=0; } }
int fileno(FILE *stream) { return stream ? stream->fd : -1; }
int fgetc(FILE *stream) { unsigned char c; return fread(&c,1,1,stream)==1 ? c : EOF; }
int getc(FILE *stream) { return fgetc(stream); }
int getchar(void) { return fgetc(stdin); }
char *fgets(char *buffer, int size, FILE *stream) { int i=0,c; if (!buffer || size<=0) return NULL; while (i<size-1 && (c=fgetc(stream))!=EOF) { buffer[i++]=(char)c; if (c=='\n') break; } if (i==0) return NULL; buffer[i]=0; return buffer; }
int fputc(int c, FILE *stream) { unsigned char b=(unsigned char)c; return fwrite(&b,1,1,stream)==1 ? b : EOF; }
int putchar(int c) { return fputc(c,stdout); }
int puts(const char *text) { if (fwrite(text,1,strlen(text),stdout)!=strlen(text) || putchar('\n')==EOF) return EOF; return 0; }

typedef struct { char *buffer; size_t size; size_t count; } FormatSink;
static void emit(FormatSink *sink, char c) { if (sink->size && sink->count+1<sink->size) sink->buffer[sink->count]=c; ++sink->count; }
static void emit_repeat(FormatSink *sink, char c, int amount) { while (amount-->0) emit(sink,c); }
static void emit_string(FormatSink *sink, const char *s, size_t n) { size_t i; for (i=0;i<n;++i) emit(sink,s[i]); }

static int format_to(FormatSink *sink, const char *format, va_list args)
{
    const char *p=format;
    while (*p) {
        int left=0, plus=0, space=0, alternate=0, zero=0, width=0, precision=-1;
        int length=0, negative=0, base=10, upper=0;
        char spec, digits[70], prefix[4];
        size_t ndigits=0, nprefix=0, i;
        unsigned long long value=0;
        if (*p!='%') { emit(sink,*p++); continue; }
        ++p;
        if (*p=='%') { emit(sink,*p++); continue; }
        for (;;) { if (*p=='-') left=1; else if (*p=='+') plus=1; else if (*p==' ') space=1; else if (*p=='#') alternate=1; else if (*p=='0') zero=1; else break; ++p; }
        if (*p=='*') { width=va_arg(args,int); ++p; if (width<0) { left=1; width=-width; } }
        else while (isdigit((unsigned char)*p)) { if (width<1000) width=width*10+(*p-'0'); ++p; }
        if (*p=='.') { precision=0; ++p; if (*p=='*') { precision=va_arg(args,int); ++p; if (precision<0) precision=-1; }
            else while (isdigit((unsigned char)*p)) { if (precision<1000) precision=precision*10+(*p-'0'); ++p; } }
        if (*p=='h') { ++p; length=(*p=='h') ? (++p,1) : 2; }
        else if (*p=='l') { ++p; length=(*p=='l') ? (++p,4) : 3; }
        else if (*p=='z' || *p=='t' || *p=='j') { length=5; ++p; }
        else if (*p=='L') { length=6; ++p; }
        spec=*p ? *p++ : 0;
        if (spec=='s') {
            const char *s=va_arg(args,const char *); size_t n;
            if (!s) s="(null)"; n=strlen(s); if (precision>=0 && n>(size_t)precision) n=(size_t)precision;
            if (!left && width>(int)n) emit_repeat(sink,' ',width-(int)n); emit_string(sink,s,n); if (left && width>(int)n) emit_repeat(sink,' ',width-(int)n); continue;
        }
        if (spec=='c') { char c=(char)va_arg(args,int); if (!left && width>1) emit_repeat(sink,' ',width-1); emit(sink,c); if (left && width>1) emit_repeat(sink,' ',width-1); continue; }
        if (spec=='p') { value=(uintptr_t)va_arg(args,void *); base=16; alternate=1; }
        else if (spec=='d' || spec=='i') {
            long long signed_value;
            if (length==4) signed_value=va_arg(args,long long); else if (length==3) signed_value=va_arg(args,long); else if (length==5) signed_value=(long long)va_arg(args,ptrdiff_t); else signed_value=va_arg(args,int);
            if (signed_value<0) { negative=1; value=(unsigned long long)0-(unsigned long long)signed_value; } else value=(unsigned long long)signed_value;
        } else if (spec=='u' || spec=='x' || spec=='X' || spec=='o') {
            if (spec=='x' || spec=='X') base=16; else if (spec=='o') base=8;
            if (spec=='X') upper=1;
            if (length==4) value=va_arg(args,unsigned long long); else if (length==3) value=va_arg(args,unsigned long); else if (length==5) value=va_arg(args,size_t); else value=va_arg(args,unsigned int);
        } else if (spec=='f' || spec=='F' || spec=='g' || spec=='G' || spec=='e' || spec=='E') {
            double number=va_arg(args,double); int places=precision<0?6:precision; int whole;
            char temp[96]; size_t n=0; double fraction;
            if (number<0) { negative=1; number=-number; }
            if (number>2147483000.0) number=2147483000.0;
            whole=(int)number; fraction=number-(double)whole;
            if (spec=='g'||spec=='G') { while (places>0 && ((int)(fraction*10.0+0.5))==0) { fraction*=10.0; --places; } }
            if (places>6) places=6;
            { char rev[16]; size_t rn=0; unsigned int w=(unsigned int)whole; do { rev[rn++]=(char)('0'+w%10); w/=10; } while(w); while(rn) temp[n++]=rev[--rn]; }
            if (places>0 || alternate) temp[n++]='.';
            for (i=0;i<(size_t)places;++i) { fraction*=10.0; temp[n++]=(char)('0'+(int)fraction%10); fraction-=(int)fraction; }
            if (negative || plus || space) prefix[nprefix++]=negative?'-':(plus?'+':' ');
            if (!left && width>(int)n+(int)nprefix) emit_repeat(sink,zero?'0':' ',width-(int)n-(int)nprefix);
            emit_string(sink,prefix,nprefix); emit_string(sink,temp,n); if (left && width>(int)n+(int)nprefix) emit_repeat(sink,' ',width-(int)n-(int)nprefix); continue;
        } else { emit(sink,'%'); if (spec) emit(sink,spec); continue; }
        if (spec=='p' || alternate) { if (base==16 && value) { prefix[nprefix++]='0'; prefix[nprefix++]=upper?'X':'x'; } else if (base==8 && (ndigits==0 || digits[0]!='0')) prefix[nprefix++]='0'; }
        do { unsigned digit=(unsigned)(value%(unsigned)base); digits[ndigits++]=(char)(digit<10?'0'+digit:(upper?'A':'a')+digit-10); value/=(unsigned)base; } while(value);
        while (precision>(int)ndigits) digits[ndigits++]='0';
        if (negative || plus || space) { for (i=nprefix;i>0;--i) prefix[i]=prefix[i-1]; prefix[0]=negative?'-':(plus?'+':' '); ++nprefix; }
        { int padding=width-(int)nprefix-(int)ndigits; if (!left && (!zero || precision>=0)) emit_repeat(sink,' ',padding); emit_string(sink,prefix,nprefix); if (!left && zero && precision<0) emit_repeat(sink,'0',padding); while(ndigits) emit(sink,digits[--ndigits]); if(left) emit_repeat(sink,' ',padding); }
    }
    if (sink->size) sink->buffer[sink->count<sink->size?sink->count:sink->size-1]=0;
    return sink->count>(size_t)INT_MAX?INT_MAX:(int)sink->count;
}

int vsnprintf(char *buffer, size_t size, const char *format, va_list args)
{
    FormatSink sink={buffer,size,0};
    va_list copy; va_copy(copy,args); int result=format_to(&sink,format,copy); va_end(copy); return result;
}
int sscanf(const char *input,const char *format,...)
{
    va_list args; int assigned=0; const char *s=input,*f=format; va_start(args,format);
    while(*f){if(isspace((unsigned char)*f)){while(isspace((unsigned char)*f))++f;while(isspace((unsigned char)*s))++s;continue;}if(*f!='%'){if(*s!=*f)break;++f;++s;continue;}++f;if(*f=='%'){if(*s!='%')break;++s;++f;continue;}while(*f=='h'||*f=='l'||*f=='z')++f;if(*f=='x'||*f=='X'||*f=='i'||*f=='d'||*f=='u'||*f=='o'){char *end;int base=(*f=='x'||*f=='X')?16:(*f=='o'?8:(*f=='i'?0:10));long value=strtol(s,&end,base);if(end==s)break;if(*f=='u'||*f=='x'||*f=='X'||*f=='o')*va_arg(args,unsigned int *)=(unsigned int)value;else *va_arg(args,int *)=(int)value;s=end;++assigned;++f;continue;}break;}va_end(args);return assigned;
}
int snprintf(char *buffer,size_t size,const char *format,...){va_list args;va_start(args,format);int r=vsnprintf(buffer,size,format,args);va_end(args);return r;}
int vsprintf(char *buffer,const char *format,va_list args){return vsnprintf(buffer,(size_t)-1,format,args);}
int sprintf(char *buffer,const char *format,...){va_list args;va_start(args,format);int r=vsprintf(buffer,format,args);va_end(args);return r;}
int vfprintf(FILE *stream,const char *format,va_list args){char buffer[2048];int n=vsnprintf(buffer,sizeof(buffer),format,args);if(n<0)return -1;size_t amount=(size_t)n<sizeof(buffer)?(size_t)n:sizeof(buffer)-1;return fwrite(buffer,1,amount,stream)==amount?n:-1;}
int fprintf(FILE *stream,const char *format,...){va_list args;va_start(args,format);int r=vfprintf(stream,format,args);va_end(args);return r;}
int vprintf(const char *format,va_list args){return vfprintf(stdout,format,args);}
int printf(const char *format,...){va_list args;va_start(args,format);int r=vfprintf(stdout,format,args);va_end(args);return r;}
void perror(const char *text){if(text&&*text)fprintf(stderr,"%s: ",text);fprintf(stderr,"%s\n",strerror(errno));}

double fabs(double x) { return x < 0 ? -x : x; }
float fabsf(float x) { return x < 0 ? -x : x; }
double floor(double x) { long long i=(long long)x; return (double)i>x ? (double)(i-1) : (double)i; }
double ceil(double x) { long long i=(long long)x; return (double)i<x ? (double)(i+1) : (double)i; }
double sqrt(double x) { double g=x>1?x:1; int i; if (x<=0) return 0; for(i=0;i<32;++i) g=(g+x/g)*0.5; return g; }
double pow(double x,double y) { long long n=(long long)y; double r=1; if((double)n!=y)return 0; if(n<0){n=-n;x=1/x;} while(n){if(n&1)r*=x;x*=x;n>>=1;}return r; }
double fmod(double x,double y) { if(y==0)return 0; return x-(double)(long long)(x/y)*y; }
double sin(double x) { double x2,term,sum; int n; const double tau=6.2831853071795864769; while(x>3.14159265358979323846)x-=tau;while(x< -3.14159265358979323846)x+=tau;x2=x*x;sum=term=x;for(n=1;n<10;++n){term*=-x2/((2*n)*(2*n+1));sum+=term;}return sum; }
double cos(double x) { return sin(x+1.57079632679489661923); }
double atan2(double y,double x) { if(x>0)return y/x; if(x<0)return y/x+(y>=0?3.14159265358979323846:-3.14159265358979323846); return y>0?1.57079632679489661923:(y<0?-1.57079632679489661923:0); }
long lrint(double x) { return (long)(x>=0?x+0.5:x-0.5); }

#include <stdlib.h>
#include <stdio.h>
#include <string.h>

#include <stdarg.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <unistd.h>
#endif

#ifdef ORIGCODE
#include "SDL.h"
#endif

#include "config.h"

#include "deh_str.h"
#include "doomtype.h"
#include "m_argv.h"
#include "m_config.h"
#include "m_misc.h"
#include "i_joystick.h"
#include "i_sound.h"
#include "i_timer.h"
#include "i_video.h"

#include "i_system.h"

#include "w_wad.h"
#include "z_zone.h"

#ifdef __MACOSX__
#include <CoreFoundation/CFUserNotification.h>
#endif

#define DEFAULT_RAM 6 
#define MIN_RAM     6  

typedef struct atexit_listentry_s atexit_listentry_t;

struct atexit_listentry_s
{
    atexit_func_t func;
    boolean run_on_error;
    atexit_listentry_t *next;
};

static atexit_listentry_t *exit_funcs = NULL;

void I_AtExit(atexit_func_t func, boolean run_on_error)
{
    atexit_listentry_t *entry;

    entry = malloc(sizeof(*entry));

    entry->func = func;
    entry->run_on_error = run_on_error;
    entry->next = exit_funcs;
    exit_funcs = entry;
}

void I_Tactile(int on, int off, int total)
{
}

static byte *AutoAllocMemory(int *size, int default_ram, int min_ram)
{
    byte *zonemem;

    zonemem = NULL;

    while (zonemem == NULL)
    {

        if (default_ram < min_ram)
        {
            I_Error("Unable to allocate %i MiB of RAM for zone", default_ram);
        }

        *size = default_ram * 1024 * 1024;

        zonemem = malloc(*size);

        if (zonemem == NULL)
        {
            default_ram -= 1;
        }
    }

    return zonemem;
}

byte *I_ZoneBase (int *size)
{
    byte *zonemem;
    int min_ram, default_ram;
    int p;

    p = M_CheckParmWithArgs("-mb", 1);

    if (p > 0)
    {
        default_ram = atoi(myargv[p+1]);
        min_ram = default_ram;
    }
    else
    {
        default_ram = DEFAULT_RAM;
        min_ram = MIN_RAM;
    }

    zonemem = AutoAllocMemory(size, default_ram, min_ram);

    printf("zone memory: %p, %x allocated for zone\n", 
           zonemem, *size);

    return zonemem;
}

void I_PrintBanner(char *msg)
{
    int i;
    int spaces = 35 - (strlen(msg) / 2);

    for (i=0; i<spaces; ++i)
        putchar(' ');

    puts(msg);
}

void I_PrintDivider(void)
{
    int i;

    for (i=0; i<75; ++i)
    {
        putchar('=');
    }

    putchar('\n');
}

void I_PrintStartupBanner(char *gamedescription)
{
    I_PrintDivider();
    I_PrintBanner(gamedescription);
    I_PrintDivider();

    printf(
    " " PACKAGE_NAME " is free software, covered by the GNU General Public\n"
    " License.  There is NO warranty; not even for MERCHANTABILITY or FITNESS\n"
    " FOR A PARTICULAR PURPOSE. You are welcome to change and distribute\n"
    " copies under certain conditions. See the source for more information.\n");

    I_PrintDivider();
}

boolean I_ConsoleStdout(void)
{
#ifdef _WIN32

    return 0;
#else
#if ORIGCODE
    return isatty(fileno(stdout));
#else
	return 0;
#endif
#endif
}

void I_Quit (void)
{
    atexit_listentry_t *entry;

    entry = exit_funcs; 

    while (entry != NULL)
    {
        entry->func();
        entry = entry->next;
    }

#if ORIGCODE
    SDL_Quit();

    exit(0);
#endif
}

#if !defined(_WIN32) && !defined(__MACOSX__) && !defined(__DJGPP__) && !defined(EONOS)
#define ZENITY_BINARY "/usr/bin/zenity"

static int ZenityAvailable(void)
{
    return system(ZENITY_BINARY " --help >/dev/null 2>&1") == 0;
}

static char *EscapeShellString(char *string)
{
    char *result;
    char *r, *s;

    result = malloc(strlen(string) * 2 + 3);
    r = result;

    *r = '"';
    ++r;

    for (s = string; *s != '\0'; ++s)
    {

        if (strchr("$`\\!", *s) != NULL)
        {
            *r = '\\';
            ++r;
        }

        *r = *s;
        ++r;
    }

    *r = '"';
    ++r;
    *r = '\0';

    return result;
}

static int ZenityErrorBox(char *message)
{
    int result;
    char *escaped_message;
    char *errorboxpath;
    static size_t errorboxpath_size;

    if (!ZenityAvailable())
    {
        return 0;
    }

    escaped_message = EscapeShellString(message);

    errorboxpath_size = strlen(ZENITY_BINARY) + strlen(escaped_message) + 19;
    errorboxpath = malloc(errorboxpath_size);
    M_snprintf(errorboxpath, errorboxpath_size, "%s --error --text=%s",
               ZENITY_BINARY, escaped_message);

    result = system(errorboxpath);

    free(errorboxpath);
    free(escaped_message);

    return result;
}

#endif 

static boolean already_quitting = false;

void I_Error (char *error, ...)
{
    char msgbuf[512];
    va_list argptr;
    atexit_listentry_t *entry;
    boolean exit_gui_popup;

    if (already_quitting)
    {
        fprintf(stderr, "Warning: recursive call to I_Error detected.\n");
#if ORIGCODE
        exit(-1);
#endif
    }
    else
    {
        already_quitting = true;
    }

    va_start(argptr, error);

    vfprintf(stderr, error, argptr);
    fprintf(stderr, "\n\n");
    va_end(argptr);
    fflush(stderr);

    va_start(argptr, error);
    memset(msgbuf, 0, sizeof(msgbuf));
    M_vsnprintf(msgbuf, sizeof(msgbuf), error, argptr);
    va_end(argptr);

    entry = exit_funcs;

    while (entry != NULL)
    {
        if (entry->run_on_error)
        {
            entry->func();
        }

        entry = entry->next;
    }

    exit_gui_popup = !M_ParmExists("-nogui");

    if (exit_gui_popup && !I_ConsoleStdout())
#ifdef _WIN32
    {
        wchar_t wmsgbuf[512];

        MultiByteToWideChar(CP_ACP, 0,
                            msgbuf, strlen(msgbuf) + 1,
                            wmsgbuf, sizeof(wmsgbuf));

        MessageBoxW(NULL, wmsgbuf, L"", MB_OK);
    }
#elif defined(__MACOSX__)
    {
        CFStringRef message;
	int i;

	for (i = 0; msgbuf[i] != '\0'; ++i)
        {
            if (msgbuf[i] == '\n')
            {
                msgbuf[i] = ' ';
            }
        }

        message = CFStringCreateWithCString(NULL, msgbuf,
                                            kCFStringEncodingUTF8);

        CFUserNotificationDisplayNotice(0,
                                        kCFUserNotificationCautionAlertLevel,
                                        NULL,
                                        NULL,
                                        NULL,
                                        CFSTR(PACKAGE_STRING),
                                        message,
                                        NULL);
    }
#elif defined(EONOS)
    {

        fprintf(stderr, "%s\n", msgbuf);
    }
#elif defined(__DJGPP__)
    {
        printf("%s\n", msgbuf);
        exit(-1);
    }

#else
    {
        ZenityErrorBox(msgbuf);
    }
#endif

#if ORIGCODE
    SDL_Quit();

    exit(-1);
#else
    exit(-1);
#endif
}

#define DOS_MEM_DUMP_SIZE 10

static const unsigned char mem_dump_dos622[DOS_MEM_DUMP_SIZE] = {
  0x57, 0x92, 0x19, 0x00, 0xF4, 0x06, 0x70, 0x00, 0x16, 0x00};
static const unsigned char mem_dump_win98[DOS_MEM_DUMP_SIZE] = {
  0x9E, 0x0F, 0xC9, 0x00, 0x65, 0x04, 0x70, 0x00, 0x16, 0x00};
static const unsigned char mem_dump_dosbox[DOS_MEM_DUMP_SIZE] = {
  0x00, 0x00, 0x00, 0xF1, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00};
static unsigned char mem_dump_custom[DOS_MEM_DUMP_SIZE];

static const unsigned char *dos_mem_dump = mem_dump_dos622;

boolean I_GetMemoryValue(unsigned int offset, void *value, int size)
{
    static boolean firsttime = true;

    if (firsttime)
    {
        int p, i, val;

        firsttime = false;
        i = 0;

        p = M_CheckParmWithArgs("-setmem", 1);

        if (p > 0)
        {
            if (!strcasecmp(myargv[p + 1], "dos622"))
            {
                dos_mem_dump = mem_dump_dos622;
            }
            if (!strcasecmp(myargv[p + 1], "dos71"))
            {
                dos_mem_dump = mem_dump_win98;
            }
            else if (!strcasecmp(myargv[p + 1], "dosbox"))
            {
                dos_mem_dump = mem_dump_dosbox;
            }
            else
            {
                for (i = 0; i < DOS_MEM_DUMP_SIZE; ++i)
                {
                    ++p;

                    if (p >= myargc || myargv[p][0] == '-')
                    {
                        break;
                    }

                    M_StrToInt(myargv[p], &val);
                    mem_dump_custom[i++] = (unsigned char) val;
                }

                dos_mem_dump = mem_dump_custom;
            }
        }
    }

    switch (size)
    {
    case 1:
        *((unsigned char *) value) = dos_mem_dump[offset];
        return true;
    case 2:
        *((unsigned short *) value) = dos_mem_dump[offset]
                                    | (dos_mem_dump[offset + 1] << 8);
        return true;
    case 4:
        *((unsigned int *) value) = dos_mem_dump[offset]
                                  | (dos_mem_dump[offset + 1] << 8)
                                  | (dos_mem_dump[offset + 2] << 16)
                                  | (dos_mem_dump[offset + 3] << 24);
        return true;
    }

    return false;
}


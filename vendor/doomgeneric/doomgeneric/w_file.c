#include <stdio.h>

#include "config.h"

#include "doomtype.h"
#include "m_argv.h"

#include "w_file.h"

extern wad_file_class_t stdc_wad_file;

#ifdef HAVE_MMAP
extern wad_file_class_t posix_wad_file;
#endif 

static wad_file_class_t *wad_file_classes[] = 
{

#ifdef HAVE_MMAP
    &posix_wad_file,
#endif
    &stdc_wad_file,
};

wad_file_t *W_OpenFile(char *path)
{
    wad_file_t *result;
    int i;

    if (!M_CheckParm("-mmap"))
    {
        return stdc_wad_file.OpenFile(path);
    }

    result = NULL;

    for (i = 0; i < arrlen(wad_file_classes); ++i)
    {
        result = wad_file_classes[i]->OpenFile(path);

        if (result != NULL)
        {
            break;
        }
    }

    return result;
}

void W_CloseFile(wad_file_t *wad)
{
    wad->file_class->CloseFile(wad);
}

size_t W_Read(wad_file_t *wad, unsigned int offset,
              void *buffer, size_t buffer_len)
{
    return wad->file_class->Read(wad, offset, buffer, buffer_len);
}


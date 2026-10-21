


















#include "config.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <ctype.h>
#include <SDL.h>
#include <SDL_mixer.h>

#ifdef HAVE_LIBSAMPLERATE
#include <samplerate.h>
#endif

#include "deh_str.h"
#include "i_sound.h"
#include "i_system.h"
#include "i_swap.h"
#include "m_argv.h"
#include "m_misc.h"
#include "w_wad.h"
#include "z_zone.h"

#include "doomtype.h"

#define LOW_PASS_FILTER 1

#define NUM_CHANNELS 16

typedef struct allocated_sound_s allocated_sound_t;

struct allocated_sound_s
{
    sfxinfo_t *sfxinfo;
    Mix_Chunk chunk;
    int use_count;
    allocated_sound_t *prev, *next;
};

static boolean setpanning_workaround = false;

static boolean sound_initialized = false;

static sfxinfo_t *channels_playing[NUM_CHANNELS];

static int mixer_freq;
static Uint16 mixer_format;
static int mixer_channels;
static boolean use_sfx_prefix;
static boolean (*ExpandSoundData)(sfxinfo_t *sfxinfo,
                                  byte *data,
                                  int samplerate,
                                  int length) = NULL;





static allocated_sound_t *allocated_sounds_head = NULL;
static allocated_sound_t *allocated_sounds_tail = NULL;
static int allocated_sounds_size = 0;

int use_libsamplerate = 0;







float libsamplerate_scale = 0.65f;



static void AllocatedSoundLink(allocated_sound_t *snd)
{
    snd->prev = NULL;

    snd->next = allocated_sounds_head;
    allocated_sounds_head = snd;

    if (allocated_sounds_tail == NULL)
    {
        allocated_sounds_tail = snd;
    }
    else
    {
        snd->next->prev = snd;
    }
}



static void AllocatedSoundUnlink(allocated_sound_t *snd)
{
    if (snd->prev == NULL)
    {
        allocated_sounds_head = snd->next;
    }
    else
    {
        snd->prev->next = snd->next;
    }

    if (snd->next == NULL)
    {
        allocated_sounds_tail = snd->prev;
    }
    else
    {
        snd->next->prev = snd->prev;
    }
}

static void FreeAllocatedSound(allocated_sound_t *snd)
{
    

    AllocatedSoundUnlink(snd);

    

    snd->sfxinfo->driver_data = NULL;

    

    allocated_sounds_size -= snd->chunk.alen;

    free(snd);
}





static boolean FindAndFreeSound(void)
{
    allocated_sound_t *snd;

    snd = allocated_sounds_tail;

    while (snd != NULL)
    {
        if (snd->use_count == 0)
        {
            FreeAllocatedSound(snd);
            return true;
        }

        snd = snd->prev;
    }

    

    return false;
}





static void ReserveCacheSpace(size_t len)
{
    if (snd_cachesize <= 0)
    {
        return;
    }

    
    

    while (allocated_sounds_size + len > snd_cachesize)
    {
        

        if (!FindAndFreeSound())
        {
            break;
        }
    }
}



static Mix_Chunk *AllocateSound(sfxinfo_t *sfxinfo, size_t len)
{
    allocated_sound_t *snd;

    

    ReserveCacheSpace(len);

    
    

    do
    {
        snd = malloc(sizeof(allocated_sound_t) + len);

        
        

        if (snd == NULL && !FindAndFreeSound())
        {
            return NULL;
        }

    } while (snd == NULL);

    

    snd->chunk.abuf = (byte *) (snd + 1);
    snd->chunk.alen = len;
    snd->chunk.allocated = 1;
    snd->chunk.volume = MIX_MAX_VOLUME;

    snd->sfxinfo = sfxinfo;
    snd->use_count = 0;

    

    sfxinfo->driver_data = snd;

    

    allocated_sounds_size += len;

    AllocatedSoundLink(snd);

    return &snd->chunk;
}



static void LockAllocatedSound(allocated_sound_t *snd)
{
    

    ++snd->use_count;

    

    
    

    AllocatedSoundUnlink(snd);
    AllocatedSoundLink(snd);
}



static void UnlockAllocatedSound(allocated_sound_t *snd)
{
    if (snd->use_count <= 0)
    {
        I_Error("Sound effect released more times than it was locked...");
    }

    --snd->use_count;

    
}





static void ReleaseSoundOnChannel(int channel)
{
    sfxinfo_t *sfxinfo = channels_playing[channel];

    if (sfxinfo == NULL)
    {
        return;
    }

    channels_playing[channel] = NULL;

    UnlockAllocatedSound(sfxinfo->driver_data);
}

#ifdef HAVE_LIBSAMPLERATE



static int SRC_ConversionMode(void)
{
    switch (use_libsamplerate)
    {
        

        default:
        case 0:
            return -1;

        

        case 1:
            return SRC_LINEAR;
        case 2:
            return SRC_ZERO_ORDER_HOLD;
        case 3:
            return SRC_SINC_FASTEST;
        case 4:
            return SRC_SINC_MEDIUM_QUALITY;
        case 5:
            return SRC_SINC_BEST_QUALITY;
    }
}








static boolean ExpandSoundData_SRC(sfxinfo_t *sfxinfo,
                                   byte *data,
                                   int samplerate,
                                   int length)
{
    SRC_DATA src_data;
    uint32_t i, abuf_index=0, clipped=0;
    uint32_t alen;
    int retn;
    int16_t *expanded;
    Mix_Chunk *chunk;

    src_data.input_frames = length;
    src_data.data_in = malloc(length * sizeof(float));
    src_data.src_ratio = (double)mixer_freq / samplerate;

    
    src_data.output_frames = src_data.src_ratio * length + (mixer_freq / 4);
    src_data.data_out = malloc(src_data.output_frames * sizeof(float));

    assert(src_data.data_in != NULL && src_data.data_out != NULL);

    

    for (i=0; i<length; ++i)
    {
        
        
        
        src_data.data_in[i] = data[i] / 127.5 - 1;
    }

    

    retn = src_simple(&src_data, SRC_ConversionMode(), 1);
    assert(retn == 0);

    

    alen = src_data.output_frames_gen * 4;

    chunk = AllocateSound(sfxinfo, src_data.output_frames_gen * 4);

    if (chunk == NULL)
    {
        return false;
    }

    expanded = (int16_t *) chunk->abuf;

    

    for (i=0; i<src_data.output_frames_gen; ++i)
    {
        
        
        
        
        

        
        
        
        
        
        
        

        
        
        
        
        
        

        float cvtval_f =
            src_data.data_out[i] * libsamplerate_scale * INT16_MAX;
        int32_t cvtval_i = cvtval_f + (cvtval_f < 0 ? -0.5 : 0.5);

        
        if (cvtval_i < -INT16_MAX)
        {
            cvtval_i = -INT16_MAX;
            ++clipped;
        }
        else if (cvtval_i > INT16_MAX)
        {
            cvtval_i = INT16_MAX;
            ++clipped;
        }

        

        expanded[abuf_index++] = cvtval_i;
        expanded[abuf_index++] = cvtval_i;
    }

    free(src_data.data_in);
    free(src_data.data_out);

    if (clipped > 0)
    {
        fprintf(stderr, "Sound '%s': clipped %u samples (%0.2f %%)\n", 
                        sfxinfo->name, clipped,
                        400.0 * clipped / chunk->alen);
    }

    return true;
}

#endif

static boolean ConvertibleRatio(int freq1, int freq2)
{
    int ratio;

    if (freq1 > freq2)
    {
        return ConvertibleRatio(freq2, freq1);
    }
    else if ((freq2 % freq1) != 0)
    {
        

        return false;
    }
    else
    {
        

        ratio = freq2 / freq1;

        while ((ratio & 1) == 0)
        {
            ratio = ratio >> 1;
        }

        return ratio == 1;
    }
}

#ifdef DEBUG_DUMP_WAVS



static void WriteWAV(char *filename, byte *data,
                     uint32_t length, int samplerate)
{
    FILE *wav;
    unsigned int i;
    unsigned short s;

    wav = fopen(filename, "wb");

    

    fwrite("RIFF", 1, 4, wav);
    i = LONG(36 + samplerate);
    fwrite(&i, 4, 1, wav);
    fwrite("WAVE", 1, 4, wav);

    

    fwrite("fmt ", 1, 4, wav);
    i = LONG(16);
    fwrite(&i, 4, 1, wav);           
    s = SHORT(1);
    fwrite(&s, 2, 1, wav);           
    s = SHORT(2);
    fwrite(&s, 2, 1, wav);           
    i = LONG(samplerate);
    fwrite(&i, 4, 1, wav);           
    i = LONG(samplerate * 2 * 2);
    fwrite(&i, 4, 1, wav);           
    s = SHORT(2 * 2);
    fwrite(&s, 2, 1, wav);           
    s = SHORT(16);
    fwrite(&s, 2, 1, wav);           

    

    fwrite("data", 1, 4, wav);
    i = LONG(length);
    fwrite(&i, 4, 1, wav);           
    fwrite(data, 1, length, wav);    

    fclose(wav);
}

#endif




static boolean ExpandSoundData_SDL(sfxinfo_t *sfxinfo,
                                   byte *data,
                                   int samplerate,
                                   int length)
{
    SDL_AudioCVT convertor;
    Mix_Chunk *chunk;
    uint32_t expanded_length;
 
    

    expanded_length = (uint32_t) ((((uint64_t) length) * mixer_freq) / samplerate);

    

    expanded_length *= 4;

    

    chunk = AllocateSound(sfxinfo, expanded_length);

    if (chunk == NULL)
    {
        return false;
    }

    
        Sint16 *expanded = (Sint16 *) chunk->abuf;
        int expand_ratio;
        int i;

        
        
        
        
        

        

        expanded_length = ((uint64_t) length * mixer_freq) / samplerate;
        expand_ratio = (length << 8) / expanded_length;

        for (i=0; i<expanded_length; ++i)
        {
            Sint16 sample;
            int src;

            src = (i * expand_ratio) >> 8;

            sample = data[src] | (data[src] << 8);
            sample -= 32768;

            

            expanded[i * 2] = expanded[i * 2 + 1] = sample;
        }

#ifdef LOW_PASS_FILTER
        
        

        {
            float rc, dt, alpha;

            
            
            
            
            

            
            

            dt = 1.0f / mixer_freq;
            rc = 1.0f / (3.14f * samplerate);
            alpha = dt / (rc + dt);

            

            for (i=2; i<expanded_length * 2; ++i)
            {
                expanded[i] = (Sint16) (alpha * expanded[i]
                                      + (1 - alpha) * expanded[i-2]);
            }
        }
#endif 

    return true;
}




static boolean CacheSFX(sfxinfo_t *sfxinfo)
{
    int lumpnum;
    unsigned int lumplen;
    int samplerate;
    unsigned int length;
    byte *data;

    

    lumpnum = sfxinfo->lumpnum;
    data = W_CacheLumpNum(lumpnum, PU_STATIC);
    lumplen = W_LumpLength(lumpnum);

    

    if (lumplen < 8
     || data[0] != 0x03 || data[1] != 0x00)
    {
        

        return false;
    }

    

    samplerate = (data[3] << 8) | data[2];
    length = (data[7] << 24) | (data[6] << 16) | (data[5] << 8) | data[4];

    
    

    
    
    
    
    

    if (length > lumplen - 8 || length <= 48)
    {
        return false;
    }

    
    

    data += 16;
    length -= 32;

    

    if (!ExpandSoundData(sfxinfo, data + 8, samplerate, length))
    {
        return false;
    }

#ifdef DEBUG_DUMP_WAVS
    {
        char filename[16];

        M_snprintf(filename, sizeof(filename), "%s.wav",
                   DEH_String(S_sfx[sound].name));
        WriteWAV(filename, sound_chunks[sound].abuf,
                 sound_chunks[sound].alen, mixer_freq);
    }
#endif

    
  
    W_ReleaseLumpNum(lumpnum);

    return true;
}

static void GetSfxLumpName(sfxinfo_t *sfx, char *buf, size_t buf_len)
{
    

    if (sfx->link != NULL)
    {
        sfx = sfx->link;
    }

    
    

    if (use_sfx_prefix)
    {
        M_snprintf(buf, buf_len, "ds%s", DEH_String(sfx->name));
    }
    else
    {
        M_StringCopy(buf, DEH_String(sfx->name), buf_len);
    }
}

#ifdef HAVE_LIBSAMPLERATE



static void I_SDL_PrecacheSounds(sfxinfo_t *sounds, int num_sounds)
{
    char namebuf[9];
    int i;

    

    if (use_libsamplerate == 0)
    {
	return;
    }

    printf("I_SDL_PrecacheSounds: Precaching all sound effects..");

    for (i=0; i<num_sounds; ++i)
    {
        if ((i % 6) == 0)
        {
            printf(".");
            fflush(stdout);
        }

        GetSfxLumpName(&sounds[i], namebuf, sizeof(namebuf));

        sounds[i].lumpnum = W_CheckNumForName(namebuf);

        if (sounds[i].lumpnum != -1)
        {
            CacheSFX(&sounds[i]);
        }
    }

    printf("\n");
}

#else

static void I_SDL_PrecacheSounds(sfxinfo_t *sounds, int num_sounds)
{
    
}

#endif



static boolean LockSound(sfxinfo_t *sfxinfo)
{
    

    if (sfxinfo->driver_data == NULL)
    {
        if (!CacheSFX(sfxinfo))
        {
            return false;
        }
    }

    LockAllocatedSound(sfxinfo->driver_data);

    return true;
}






static int I_SDL_GetSfxLumpNum(sfxinfo_t *sfx)
{
    char namebuf[9];

    GetSfxLumpName(sfx, namebuf, sizeof(namebuf));

    return W_GetNumForName(namebuf);
}

static void I_SDL_UpdateSoundParams(int handle, int vol, int sep)
{
    int left, right;

    if (!sound_initialized || handle < 0 || handle >= NUM_CHANNELS)
    {
        return;
    }

    left = ((254 - sep) * vol) / 127;
    right = ((sep) * vol) / 127;

    if (left < 0) left = 0;
    else if ( left > 255) left = 255;
    if (right < 0) right = 0;
    else if (right > 255) right = 255;

    
    
    
    

    if (setpanning_workaround)
    {
        Mix_UnregisterAllEffects(handle);
    }

    Mix_SetPanning(handle, left, right);
}














static int I_SDL_StartSound(sfxinfo_t *sfxinfo, int channel, int vol, int sep)
{
    allocated_sound_t *snd;

    if (!sound_initialized || channel < 0 || channel >= NUM_CHANNELS)
    {
        return -1;
    }

    
    

    ReleaseSoundOnChannel(channel);

    

    if (!LockSound(sfxinfo))
    {
	return -1;
    }

    snd = sfxinfo->driver_data;

    

    Mix_PlayChannelTimed(channel, &snd->chunk, 0, -1);

    channels_playing[channel] = sfxinfo;

    
 
    I_SDL_UpdateSoundParams(channel, vol, sep);

    return channel;
}

static void I_SDL_StopSound(int handle)
{
    if (!sound_initialized || handle < 0 || handle >= NUM_CHANNELS)
    {
        return;
    }

    Mix_HaltChannel(handle);

    
    

    ReleaseSoundOnChannel(handle);
}


static boolean I_SDL_SoundIsPlaying(int handle)
{
    if (!sound_initialized || handle < 0 || handle >= NUM_CHANNELS)
    {
        return false;
    }

    return Mix_Playing(handle);
}





static void I_SDL_UpdateSound(void)
{
    int i;

    

    for (i=0; i<NUM_CHANNELS; ++i)
    {
        if (channels_playing[i] && !I_SDL_SoundIsPlaying(i))
        {
            
            
            
            ReleaseSoundOnChannel(i);
        }
    }
}

static void I_SDL_ShutdownSound(void)
{    
    if (!sound_initialized)
    {
        return;
    }

    Mix_CloseAudio();
    SDL_QuitSubSystem(SDL_INIT_AUDIO);

    sound_initialized = false;
}




static int GetSliceSize(void)
{
    int limit;
    int n;

    limit = (snd_samplerate * snd_maxslicetime_ms) / 1000;

    

    for (n=0;; ++n)
    {
        

        if ((1 << (n + 1)) > limit)
        {
            return (1 << n);
        }
    }

    

    return 1024;
}

static boolean I_SDL_InitSound(boolean _use_sfx_prefix)
{
    int i;

    use_sfx_prefix = _use_sfx_prefix;

    

    for (i=0; i<NUM_CHANNELS; ++i)
    {
        channels_playing[i] = NULL;
    }

    if (SDL_Init(SDL_INIT_AUDIO) < 0)
    {
        fprintf(stderr, "Unable to set up sound.\n");
        return false;
    }

    if (Mix_OpenAudio(snd_samplerate, AUDIO_S16SYS, 2, GetSliceSize()) < 0)
    {
        fprintf(stderr, "Error initialising SDL_mixer: %s\n", Mix_GetError());
        return false;
    }

    ExpandSoundData = ExpandSoundData_SDL;

    Mix_QuerySpec(&mixer_freq, &mixer_format, &mixer_channels);

#ifdef HAVE_LIBSAMPLERATE
    if (use_libsamplerate != 0)
    {
        if (SRC_ConversionMode() < 0)
        {
            I_Error("I_SDL_InitSound: Invalid value for use_libsamplerate: %i",
                    use_libsamplerate);
        }

        ExpandSoundData = ExpandSoundData_SRC;
    }
#else
    if (use_libsamplerate != 0)
    {
        fprintf(stderr, "I_SDL_InitSound: use_libsamplerate=%i, but "
                        "libsamplerate support not compiled in.\n",
                        use_libsamplerate);
    }
#endif

    
    
    
    

    {
        const SDL_version *mixer_version;
        int v;

        mixer_version = Mix_Linked_Version();
        v = SDL_VERSIONNUM(mixer_version->major,
                           mixer_version->minor,
                           mixer_version->patch);

        if (v <= SDL_VERSIONNUM(1, 2, 8))
        {
            setpanning_workaround = true;
            fprintf(stderr, "\n"
              "ATTENTION: You are using an old version of SDL_mixer!\n"
              "           This version has a bug that may cause "
                          "your sound to stutter.\n"
              "           Please upgrade to a newer version!\n"
              "\n");
        }
    }

    Mix_AllocateChannels(NUM_CHANNELS);

    SDL_PauseAudio(0);

    sound_initialized = true;

    return true;
}

static snddevice_t sound_sdl_devices[] = 
{
    SNDDEVICE_SB,
    SNDDEVICE_PAS,
    SNDDEVICE_GUS,
    SNDDEVICE_WAVEBLASTER,
    SNDDEVICE_SOUNDCANVAS,
    SNDDEVICE_AWE32,
};

sound_module_t DG_sound_module = 
{
    sound_sdl_devices,
    arrlen(sound_sdl_devices),
    I_SDL_InitSound,
    I_SDL_ShutdownSound,
    I_SDL_GetSfxLumpNum,
    I_SDL_UpdateSound,
    I_SDL_UpdateSoundParams,
    I_SDL_StartSound,
    I_SDL_StopSound,
    I_SDL_SoundIsPlaying,
    I_SDL_PrecacheSounds,
};


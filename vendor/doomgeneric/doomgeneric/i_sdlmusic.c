


















#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <SDL.h>
#include <SDL_mixer.h>

#include "config.h"
#include "doomtype.h"
#include "memio.h"
#include "mus2mid.h"

#include "deh_str.h"
#include "gusconf.h"
#include "i_sound.h"
#include "i_system.h"
#include "i_swap.h"
#include "m_argv.h"
#include "m_config.h"
#include "m_misc.h"
#include "sha1.h"
#include "w_wad.h"
#include "z_zone.h"

#define MAXMIDLENGTH (96 * 1024)
#define MID_HEADER_MAGIC "MThd"
#define MUS_HEADER_MAGIC "MUS\x1a"

#define FLAC_HEADER "fLaC"
#define OGG_HEADER "OggS"





#define LOOP_START_TAG "LOOP_START"
#define LOOP_END_TAG   "LOOP_END"


#define FLAC_STREAMINFO      0
#define FLAC_VORBIS_COMMENT  4


#define OGG_ID_HEADER        1
#define OGG_COMMENT_HEADER   3











typedef struct
{
    sha1_digest_t hash;
    char *filename;
} subst_music_t;


typedef struct
{
    boolean valid;
    unsigned int samplerate_hz;
    int start_time, end_time;
} file_metadata_t;

static subst_music_t *subst_music = NULL;
static unsigned int subst_music_len = 0;

static const char *subst_config_filenames[] =
{
    "doom1-music.cfg",
    "doom2-music.cfg",
    "tnt-music.cfg",
    "heretic-music.cfg",
    "hexen-music.cfg",
    "strife-music.cfg",
};

static boolean music_initialized = false;




static boolean sdl_was_initialized = false;

static boolean musicpaused = false;
static int current_music_volume;

char *timidity_cfg_path = "";

static char *temp_timidity_cfg = NULL;



static boolean playing_substitute = false;
static file_metadata_t file_metadata;



static unsigned int current_track_pos;


static Mix_Music *current_track_music = NULL;


static boolean current_track_loop;



static unsigned int ParseVorbisTime(unsigned int samplerate_hz, char *value)
{
    char *num_start, *p;
    unsigned int result = 0;
    char c;

    if (strchr(value, ':') == NULL)
    {
	return atoi(value);
    }

    result = 0;
    num_start = value;

    for (p = value; *p != '\0'; ++p)
    {
        if (*p == '.' || *p == ':')
        {
            c = *p; *p = '\0';
            result = result * 60 + atoi(num_start);
            num_start = p + 1;
            *p = c;
        }

        if (*p == '.')
        {
            return result * samplerate_hz
	         + (unsigned int) (atof(p) * samplerate_hz);
        }
    }

    return (result * 60 + atoi(num_start)) * samplerate_hz;
}



static void ParseVorbisComment(file_metadata_t *metadata, char *comment)
{
    char *eq, *key, *value;

    eq = strchr(comment, '=');

    if (eq == NULL)
    {
        return;
    }

    key = comment;
    *eq = '\0';
    value = eq + 1;

    if (!strcmp(key, LOOP_START_TAG))
    {
        metadata->start_time = ParseVorbisTime(metadata->samplerate_hz, value);
    }
    else if (!strcmp(key, LOOP_END_TAG))
    {
        metadata->end_time = ParseVorbisTime(metadata->samplerate_hz, value);
    }
}


static void ParseVorbisComments(file_metadata_t *metadata, FILE *fs)
{
    uint32_t buf;
    unsigned int num_comments, i, comment_len;
    char *comment;

    
    if (metadata->samplerate_hz == 0)
    {
	return;
    }

    
    if (fread(&buf, 4, 1, fs) < 1)
    {
        return;
    }
    if (fseek(fs, LONG(buf), SEEK_CUR) != 0)
    {
	return;
    }

    
    if (fread(&buf, 4, 1, fs) < 1)
    {
        return;
    }
    num_comments = LONG(buf);

    
    for (i = 0; i < num_comments; ++i)
    {
        
        if (fread(&buf, 4, 1, fs) < 1)
	{
            return;
	}

        comment_len = LONG(buf);

        
        comment = calloc(1, comment_len + 1);
        if (comment == NULL
         || fread(comment, 1, comment_len, fs) < comment_len)
        {
            free(comment);
            break;
        }

        
        ParseVorbisComment(metadata, comment);
        free(comment);
    }
}

static void ParseFlacStreaminfo(file_metadata_t *metadata, FILE *fs)
{
    byte buf[34];

    
    if (fread(buf, sizeof(buf), 1, fs) < 1)
    {
        return;
    }

    
    metadata->samplerate_hz = (buf[10] << 12) | (buf[11] << 4)
                            | (buf[12] >> 4);
    
    
    
    
}

static void ParseFlacFile(file_metadata_t *metadata, FILE *fs)
{
    byte header[4];
    unsigned int block_type;
    size_t block_len;
    boolean last_block;

    for (;;)
    {
        long pos = -1;

        
        if (fread(header, 4, 1, fs) < 1)
        {
            return;
        }

        block_type = header[0] & ~0x80;
        last_block = (header[0] & 0x80) != 0;
        block_len = (header[1] << 16) | (header[2] << 8) | header[3];

        pos = ftell(fs);
        if (pos < 0)
        {
            return;
        }

        if (block_type == FLAC_STREAMINFO)
        {
            ParseFlacStreaminfo(metadata, fs);
        }
        else if (block_type == FLAC_VORBIS_COMMENT)
        {
            ParseVorbisComments(metadata, fs);
        }

        if (last_block)
        {
            break;
        }

        
        if (fseek(fs, pos + block_len, SEEK_SET) != 0)
        {
            return;
        }
    }
}

static void ParseOggIdHeader(file_metadata_t *metadata, FILE *fs)
{
    byte buf[21];

    if (fread(buf, sizeof(buf), 1, fs) < 1)
    {
        return;
    }

    metadata->samplerate_hz = (buf[8] << 24) | (buf[7] << 16)
                            | (buf[6] << 8) | buf[5];
}

static void ParseOggFile(file_metadata_t *metadata, FILE *fs)
{
    byte buf[7];
    unsigned int offset;

    
    
    memset(buf, 0, sizeof(buf));

    for (offset = 0; offset < 100 * 1024; ++offset)
    {
	
	
	
        memmove(buf, buf + 1, sizeof(buf) - 1);

        if (fread(&buf[6], 1, 1, fs) < 1)
        {
            return;
        }

        if (!memcmp(buf + 1, "vorbis", 6))
        {
            switch (buf[0])
            {
                case OGG_ID_HEADER:
                    ParseOggIdHeader(metadata, fs);
                    break;
                case OGG_COMMENT_HEADER:
		    ParseVorbisComments(metadata, fs);
                    break;
                default:
                    break;
            }
        }
    }
}

static void ReadLoopPoints(char *filename, file_metadata_t *metadata)
{
    FILE *fs;
    char header[4];

    metadata->valid = false;
    metadata->samplerate_hz = 0;
    metadata->start_time = 0;
    metadata->end_time = -1;

    fs = fopen(filename, "r");

    if (fs == NULL)
    {
        return;
    }

    
    

    if (fread(header, 4, 1, fs) < 1)
    {
        fclose(fs);
        return;
    }

    if (memcmp(header, FLAC_HEADER, 4) == 0)
    {
        ParseFlacFile(metadata, fs);
    }
    else if (memcmp(header, OGG_HEADER, 4) == 0)
    {
        ParseOggFile(metadata, fs);
    }

    fclose(fs);

    
    metadata->valid = metadata->samplerate_hz > 0;
}




static char *GetSubstituteMusicFile(void *data, size_t data_len)
{
    sha1_context_t context;
    sha1_digest_t hash;
    char *filename;
    int i;

    
    if (subst_music_len == 0)
    {
        return NULL;
    }

    SHA1_Init(&context);
    SHA1_Update(&context, data, data_len);
    SHA1_Final(hash, &context);

    
    
    
    

    filename = NULL;

    for (i = 0; i < subst_music_len; ++i)
    {
        if (memcmp(hash, subst_music[i].hash, sizeof(hash)) == 0)
        {
            filename = subst_music[i].filename;

            
            
            
            
            if (M_FileExists(filename))
            {
                break;
            }
        }
    }

    return filename;
}



static void AddSubstituteMusic(subst_music_t *subst)
{
    ++subst_music_len;
    subst_music =
        realloc(subst_music, sizeof(subst_music_t) * subst_music_len);
    memcpy(&subst_music[subst_music_len - 1], subst, sizeof(subst_music_t));
}

static int ParseHexDigit(char c)
{
    c = tolower(c);

    if (c >= '0' && c <= '9')
    {
        return c - '0';
    }
    else if (c >= 'a' && c <= 'f')
    {
        return 10 + (c - 'a');
    }
    else
    {
        return -1;
    }
}

static char *GetFullPath(char *base_filename, char *path)
{
    char *basedir, *result;
    char *p;

    
    
    if (path[0] == DIR_SEPARATOR)
    {
        return strdup(path);
    }

#ifdef _WIN32
    
    if (isalpha(path[0]) && path[1] == ':' && path[2] == DIR_SEPARATOR)
    {
        return strdup(path);
    }
#endif

    
    
    
    path = M_StringReplace(path, "/", DIR_SEPARATOR_S);

    
    
    basedir = strdup(base_filename);
    p = strrchr(basedir, DIR_SEPARATOR);
    if (p != NULL)
    {
        p[1] = '\0';
        result = M_StringJoin(basedir, path, NULL);
    }
    else
    {
        result = strdup(path);
    }
    free(basedir);
    free(path);

    return result;
}




static char *ParseSubstituteLine(char *filename, char *line)
{
    subst_music_t subst;
    char *p;
    int hash_index;

    
    p = strchr(line, '#');
    if (p != NULL)
    {
        while (p > line && isspace(*(p - 1)))
        {
            --p;
        }
        *p = '\0';
    }

    
    for (p = line; *p != '\0' && isspace(*p); ++p);

    
    
    if (*p == '\0')
    {
        return NULL;
    }

    
    hash_index = 0;
    while (*p != '\0' && *p != '=' && !isspace(*p))
    {
        int d1, d2;

        d1 = ParseHexDigit(p[0]);
        d2 = ParseHexDigit(p[1]);

        if (d1 < 0 || d2 < 0)
        {
            return "Invalid hex digit in SHA1 hash";
        }
        else if (hash_index >= sizeof(sha1_digest_t))
        {
            return "SHA1 hash too long";
        }

        subst.hash[hash_index] = (d1 << 4) | d2;
        ++hash_index;

        p += 2;
    }

    if (hash_index != sizeof(sha1_digest_t))
    {
        return "SHA1 hash too short";
    }

    
    for (; *p != '\0' && isspace(*p); ++p);

    if (*p != '=')
    {
        return "Expected '='";
    }

    ++p;

    
    for (; *p != '\0' && isspace(*p); ++p);

    
    while (strlen(p) > 0 && isspace(p[strlen(p) - 1]))
    {
        p[strlen(p) - 1] = '\0';
    }

    if (strlen(p) == 0)
    {
        return "No filename specified for music substitution";
    }

    
    subst.filename = GetFullPath(filename, p);
    AddSubstituteMusic(&subst);

    return NULL;
}



static boolean ReadSubstituteConfig(char *filename)
{
    char line[128];
    FILE *fs;
    char *error;
    int linenum = 1;
    int old_subst_music_len;

    fs = fopen(filename, "r");

    if (fs == NULL)
    {
        return false;
    }

    old_subst_music_len = subst_music_len;

    while (!feof(fs))
    {
        M_StringCopy(line, "", sizeof(line));
        fgets(line, sizeof(line), fs);

        error = ParseSubstituteLine(filename, line);

        if (error != NULL)
        {
            fprintf(stderr, "%s:%i: Error: %s\n", filename, linenum, error);
        }

        ++linenum;
    }

    fclose(fs);

    return true;
}



static void LoadSubstituteConfigs(void)
{
    char *musicdir;
    char *path;
    unsigned int i;

    if (!strcmp(configdir, ""))
    {
        musicdir = strdup("");
    }
    else
    {
        musicdir = M_StringJoin(configdir, "music", DIR_SEPARATOR_S, NULL);
    }

    
    
    
    
    for (i = 0; i < arrlen(subst_config_filenames); ++i)
    {
        path = M_StringJoin(musicdir, subst_config_filenames[i], NULL);
        ReadSubstituteConfig(path);
        free(path);
    }

    free(musicdir);

    if (subst_music_len > 0)
    {
        printf("Loaded %i music substitutions from config files.\n",
               subst_music_len);
    }
}






static boolean IsMusicLump(int lumpnum)
{
    byte *data;
    boolean result;

    if (W_LumpLength(lumpnum) < 4)
    {
        return false;
    }

    data = W_CacheLumpNum(lumpnum, PU_STATIC);

    result = memcmp(data, MUS_HEADER_MAGIC, 4) == 0
          || memcmp(data, MID_HEADER_MAGIC, 4) == 0;

    W_ReleaseLumpNum(lumpnum);

    return result;
}




static void DumpSubstituteConfig(char *filename)
{
    sha1_context_t context;
    sha1_digest_t digest;
    char name[9];
    byte *data;
    FILE *fs;
    int lumpnum, h;

    fs = fopen(filename, "w");

    if (fs == NULL)
    {
        I_Error("Failed to open %s for writing", filename);
        return;
    }

    fprintf(fs, "# Example %s substitute MIDI file.\n\n", PACKAGE_NAME);
    fprintf(fs, "# SHA1 hash                              = filename\n");

    for (lumpnum = 0; lumpnum < numlumps; ++lumpnum)
    {
        strncpy(name, lumpinfo[lumpnum].name, 8);
        name[8] = '\0';

        if (!IsMusicLump(lumpnum))
        {
            continue;
        }

        
        data = W_CacheLumpNum(lumpnum, PU_STATIC);
        SHA1_Init(&context);
        SHA1_Update(&context, data, W_LumpLength(lumpnum));
        SHA1_Final(digest, &context);
        W_ReleaseLumpNum(lumpnum);

        
        for (h = 0; h < sizeof(sha1_digest_t); ++h)
        {
            fprintf(fs, "%02x", digest[h]);
        }

        fprintf(fs, " = %s.ogg\n", name);
    }

    fprintf(fs, "\n");
    fclose(fs);

    printf("Substitute MIDI config file written to %s.\n", filename);
    I_Quit();
}






static boolean WriteWrapperTimidityConfig(char *write_path)
{
    char *p, *path;
    FILE *fstream;

    if (!strcmp(timidity_cfg_path, ""))
    {
        return false;
    }

    fstream = fopen(write_path, "w");

    if (fstream == NULL)
    {
        return false;
    }

    p = strrchr(timidity_cfg_path, DIR_SEPARATOR);
    if (p != NULL)
    {
        path = strdup(timidity_cfg_path);
        path[p - timidity_cfg_path] = '\0';
        fprintf(fstream, "dir %s\n", path);
        free(path);
    }

    fprintf(fstream, "source %s\n", timidity_cfg_path);
    fclose(fstream);

    return true;
}

void I_InitTimidityConfig(void)
{
    char *env_string;
    boolean success;

    temp_timidity_cfg = M_TempFile("timidity.cfg");

    success = WriteWrapperTimidityConfig(temp_timidity_cfg);

    
    

    if (success)
    {
        env_string = M_StringJoin("TIMIDITY_CFG=", temp_timidity_cfg, NULL);
        putenv(env_string);
    }
    else
    {
        free(temp_timidity_cfg);
        temp_timidity_cfg = NULL;
    }
}



static void RemoveTimidityConfig(void)
{
    if (temp_timidity_cfg != NULL)
    {
        remove(temp_timidity_cfg);
        free(temp_timidity_cfg);
    }
}



static void I_SDL_ShutdownMusic(void)
{
    if (music_initialized)
    {
        Mix_HaltMusic();
        music_initialized = false;

        if (sdl_was_initialized)
        {
            Mix_CloseAudio();
            SDL_QuitSubSystem(SDL_INIT_AUDIO);
            sdl_was_initialized = false;
        }
    }
}

static boolean SDLIsInitialized(void)
{
    int freq, channels;
    Uint16 format;

    return Mix_QuerySpec(&freq, &format, &channels) != 0;
}


void TrackPositionCallback(int chan, void *stream, int len, void *udata)
{
    
    current_track_pos += len / 4;
}


static boolean I_SDL_InitMusic(void)
{
    int i;

    
    
    

#ifdef __MACOSX__
    {
        const SDL_version *v = Mix_Linked_Version();

        if (SDL_VERSIONNUM(v->major, v->minor, v->patch)
          < SDL_VERSIONNUM(1, 2, 11))
        {
            printf("\n"
               "                   *** WARNING ***\n"
               "      You are using an old version of SDL_mixer.\n"
               "      Music playback on this version may cause crashes\n"
               "      under OS X and is disabled by default.\n"
               "\n");
        }
    }
#endif

    
    
    
    
    
    

    i = M_CheckParmWithArgs("-dumpsubstconfig", 1);

    if (i > 0)
    {
        DumpSubstituteConfig(myargv[i + 1]);
    }

    
    

    if (SDLIsInitialized())
    {
        music_initialized = true;
    }
    else
    {
        if (SDL_Init(SDL_INIT_AUDIO) < 0)
        {
            fprintf(stderr, "Unable to set up sound.\n");
        }
        else if (Mix_OpenAudio(snd_samplerate, AUDIO_S16SYS, 2, 1024) < 0)
        {
            fprintf(stderr, "Error initializing SDL_mixer: %s\n",
                    Mix_GetError());
            SDL_QuitSubSystem(SDL_INIT_AUDIO);
        }
        else
        {
            SDL_PauseAudio(0);

            sdl_was_initialized = true;
            music_initialized = true;
        }
    }

    
    

    RemoveTimidityConfig();

    
    

    if (strlen(snd_musiccmd) > 0)
    {
        Mix_SetMusicCMD(snd_musiccmd);
    }

    
    Mix_RegisterEffect(MIX_CHANNEL_POST, TrackPositionCallback, NULL, NULL);

    
    if (snd_musicdevice == SNDDEVICE_GENMIDI)
    {
        LoadSubstituteConfigs();
    }

    return music_initialized;
}






static void UpdateMusicVolume(void)
{
    int vol;

    if (musicpaused)
    {
        vol = 0;
    }
    else
    {
        vol = (current_music_volume * MIX_MAX_VOLUME) / 127;
    }

    Mix_VolumeMusic(vol);
}



static void I_SDL_SetMusicVolume(int volume)
{
    
    current_music_volume = volume;

    UpdateMusicVolume();
}



static void I_SDL_PlaySong(void *handle, boolean looping)
{
    int loops;

    if (!music_initialized)
    {
        return;
    }

    if (handle == NULL)
    {
        return;
    }

    current_track_music = (Mix_Music *) handle;
    current_track_loop = looping;

    if (looping)
    {
        loops = -1;
    }
    else
    {
        loops = 1;
    }

    
    
    if (playing_substitute && file_metadata.valid)
    {
        loops = 1;
        SDL_LockAudio();
        current_track_pos = 0;  
        SDL_UnlockAudio();
    }

    Mix_PlayMusic(current_track_music, loops);
}

static void I_SDL_PauseSong(void)
{
    if (!music_initialized)
    {
        return;
    }

    musicpaused = true;

    UpdateMusicVolume();
}

static void I_SDL_ResumeSong(void)
{
    if (!music_initialized)
    {
        return;
    }

    musicpaused = false;

    UpdateMusicVolume();
}

static void I_SDL_StopSong(void)
{
    if (!music_initialized)
    {
        return;
    }

    Mix_HaltMusic();
    playing_substitute = false;
    current_track_music = NULL;
}

static void I_SDL_UnRegisterSong(void *handle)
{
    Mix_Music *music = (Mix_Music *) handle;

    if (!music_initialized)
    {
        return;
    }

    if (handle == NULL)
    {
        return;
    }

    Mix_FreeMusic(music);
}



static boolean IsMid(byte *mem, int len)
{
    return len > 4 && !memcmp(mem, "MThd", 4);
}

static boolean ConvertMus(byte *musdata, int len, char *filename)
{
    MEMFILE *instream;
    MEMFILE *outstream;
    void *outbuf;
    size_t outbuf_len;
    int result;

    instream = mem_fopen_read(musdata, len);
    outstream = mem_fopen_write();

    result = mus2mid(instream, outstream);

    if (result == 0)
    {
        mem_get_buf(outstream, &outbuf, &outbuf_len);

        M_WriteFile(filename, outbuf, outbuf_len);
    }

    mem_fclose(instream);
    mem_fclose(outstream);

    return result;
}

static void *I_SDL_RegisterSong(void *data, int len)
{
    char *filename;
    Mix_Music *music;

    if (!music_initialized)
    {
        return NULL;
    }

    playing_substitute = false;

    
    filename = GetSubstituteMusicFile(data, len);

    if (filename != NULL)
    {
        music = Mix_LoadMUS(filename);

        if (music == NULL)
        {
            
            
            fprintf(stderr, "Failed to load substitute music file: %s: %s\n",
                    filename, Mix_GetError());
        }
        else
        {
            
            
            playing_substitute = true;
            ReadLoopPoints(filename, &file_metadata);
            return music;
        }
    }

    
    

    filename = M_TempFile("doom.mid");

    if (IsMid(data, len) && len < MAXMIDLENGTH)
    {
        M_WriteFile(filename, data, len);
    }
    else
    {
	

        ConvertMus(data, len, filename);
    }

    
    
    

    music = Mix_LoadMUS(filename);

    if (music == NULL)
    {
        

        fprintf(stderr, "Error loading midi: %s\n", Mix_GetError());
    }

    
    
    
    

    if (strlen(snd_musiccmd) == 0)
    {
        remove(filename);
    }

    free(filename);

    return music;
}


static boolean I_SDL_MusicIsPlaying(void)
{
    if (!music_initialized)
    {
        return false;
    }

    return Mix_PlayingMusic();
}


static double GetMusicPosition(void)
{
    unsigned int music_pos;
    int freq;

    Mix_QuerySpec(&freq, NULL, NULL);

    SDL_LockAudio();
    music_pos = current_track_pos;
    SDL_UnlockAudio();

    return (double) music_pos / freq;
}

static void RestartCurrentTrack(void)
{
    double start = (double) file_metadata.start_time
                 / file_metadata.samplerate_hz;

    
    
    if (current_track_loop)
    {
        
        if (current_track_music != NULL)
        {
            Mix_PlayMusic(current_track_music, 1);
        }

        Mix_SetMusicPosition(start);
        SDL_LockAudio();
        current_track_pos = file_metadata.start_time;
        SDL_UnlockAudio();
    }
    else
    {
        Mix_HaltMusic();
        current_track_music = NULL;
        playing_substitute = false;
    }
}



static void I_SDL_PollMusic(void)
{
    if (playing_substitute && file_metadata.valid)
    {
        double end = (double) file_metadata.end_time
                   / file_metadata.samplerate_hz;

        
        if (file_metadata.end_time >= 0 && GetMusicPosition() >= end)
        {
            RestartCurrentTrack();
        }

        
        if (!Mix_PlayingMusic() && current_track_loop)
        {
            RestartCurrentTrack();
        }
    }
}

static snddevice_t music_sdl_devices[] =
{
    SNDDEVICE_PAS,
    SNDDEVICE_GUS,
    SNDDEVICE_WAVEBLASTER,
    SNDDEVICE_SOUNDCANVAS,
    SNDDEVICE_GENMIDI,
    SNDDEVICE_AWE32,
};

music_module_t DG_music_module =
{
    music_sdl_devices,
    arrlen(music_sdl_devices),
    I_SDL_InitMusic,
    I_SDL_ShutdownMusic,
    I_SDL_SetMusicVolume,
    I_SDL_PauseSong,
    I_SDL_ResumeSong,
    I_SDL_RegisterSong,
    I_SDL_UnRegisterSong,
    I_SDL_PlaySong,
    I_SDL_StopSong,
    I_SDL_MusicIsPlaying,
    I_SDL_PollMusic,
};


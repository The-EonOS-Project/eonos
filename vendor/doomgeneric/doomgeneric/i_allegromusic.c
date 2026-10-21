#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

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

#include <allegro/base.h>
#include <allegro/midi.h>
#include <allegro/sound.h>
#include <allegro/system.h>

#define MAXMIDLENGTH (96 * 1024)
#define MID_HEADER_MAGIC "MThd"
#define MUS_HEADER_MAGIC "MUS\x1a"

static boolean music_initialized = false;

static boolean musicpaused = false;
static int current_music_volume;

MIDI *current_track_music = NULL;

static boolean current_track_loop;

static void I_Allegro_ShutdownMusic(void)
{

}

static boolean I_Allegro_InitMusic(void)
{

	music_initialized = true;

	return true;
}

static void I_Allegro_SetMusicVolume(int volume)
{
	int digivol = -1;
	get_volume(&digivol, NULL);

	set_volume(digivol, volume * 2);
}

static void I_Allegro_PlaySong(void *handle, boolean looping)
{
	int retval;

	if (!music_initialized)
	{
		return;
	}

	if (handle == NULL)
	{
		return;
	}

	current_track_music = (MIDI *) handle;
	current_track_loop = looping;

	retval = play_midi(current_track_music, looping);
	if (retval < 0) {
		fprintf(stderr, "Error playing midi: %d \"%s\"\n", retval, allegro_error);
	}
}

static void I_Allegro_PauseSong(void)
{
	if (!music_initialized)
	{
		return;
	}

	musicpaused = true;

	midi_pause();
}

static void I_Allegro_ResumeSong(void)
{
	if (!music_initialized)
	{
		return;
	}

	musicpaused = false;

	midi_resume();
}

static void I_Allegro_StopSong(void)
{
	if (!music_initialized)
	{
		return;
	}

	stop_midi();

	current_track_music = NULL;
}

static void I_Allegro_UnRegisterSong(void *handle)
{
	MIDI *midi = (MIDI *) handle;

	if (!music_initialized)
	{
		return;
	}

	if (handle == NULL)
	{
		return;
	}

	destroy_midi(midi);
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

static void *I_Allegro_RegisterSong(void *data, int len)
{
	char *filename;
	MIDI *music;

	if (!music_initialized)
	{
		return NULL;
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

	music = load_midi(filename);

	if (music == NULL)
	{

		fprintf(stderr, "Error loading midi: %s\n", allegro_error);
	}

	remove(filename);

	free(filename);

	return music;
}

static boolean I_Allegro_MusicIsPlaying(void)
{
	if (!music_initialized)
	{
		return false;
	}

	return (current_track_music != NULL) && (midi_pos > 0);
}

static double GetMusicPosition(void)
{
	return midi_time;
}

static void I_Allegro_PollMusic(void)
{

}

static snddevice_t music_allegro_devices[] =
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
	music_allegro_devices,
	arrlen(music_allegro_devices),
	I_Allegro_InitMusic,
	I_Allegro_ShutdownMusic,
	I_Allegro_SetMusicVolume,
	I_Allegro_PauseSong,
	I_Allegro_ResumeSong,
	I_Allegro_RegisterSong,
	I_Allegro_UnRegisterSong,
	I_Allegro_PlaySong,
	I_Allegro_StopSong,
	I_Allegro_MusicIsPlaying,
	I_Allegro_PollMusic,
};


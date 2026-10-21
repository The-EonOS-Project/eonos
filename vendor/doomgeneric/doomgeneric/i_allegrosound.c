


















#include "config.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <ctype.h>

#include "deh_str.h"
#include "i_sound.h"
#include "i_system.h"
#include "i_swap.h"
#include "m_argv.h"
#include "m_misc.h"
#include "w_wad.h"
#include "z_zone.h"

#include "doomtype.h"

#include <allegro/base.h>
#include <allegro/sound.h>
#include <allegro/system.h>


#define NUM_CHANNELS 16

#define NUM_MIDI_CHANNELS 16


static boolean sound_initialized = false;

static sfxinfo_t *channels_playing[NUM_CHANNELS];

static int allegro_voices[NUM_CHANNELS];

static SAMPLE *dummy_sample = NULL;

static boolean use_sfx_prefix;




int use_libsamplerate = 0;







float libsamplerate_scale = 0.65f;





static boolean CacheSFX(sfxinfo_t *sfxinfo)
{
	int lumpnum;
	unsigned int lumplen;
	int samplerate;
	unsigned int length;
	byte *data;
	SAMPLE *sample;

	

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

	sample = create_sample(8, 0, samplerate, length);
	if (sample == NULL) {
		return false;
	}
	memcpy(sample->data, data, length);

	sfxinfo->driver_data = sample;

	

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


static void I_Allegro_PrecacheSounds(sfxinfo_t *sounds, int num_sounds)
{
	char namebuf[9];
	int i;

	printf("I_Allegro_PrecacheSounds: Precaching all sound effects..");

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







static int I_Allegro_GetSfxLumpNum(sfxinfo_t *sfx)
{
	char namebuf[9];

	GetSfxLumpName(sfx, namebuf, sizeof(namebuf));

	return W_GetNumForName(namebuf);
}

static void I_Allegro_UpdateSoundParams(int handle, int vol, int sep)
{
	int left, right;

	if (!sound_initialized || handle < 0 || handle >= NUM_CHANNELS)
	{
		return;
	}

	if (channels_playing[handle] == NULL) {
		return;
	}

	voice_set_volume(allegro_voices[handle], vol);
	voice_set_pan(allegro_voices[handle], sep);
}














static int I_Allegro_StartSound(sfxinfo_t *sfxinfo, int channel, int vol, int sep)
{
	if (!sound_initialized || channel < 0 || channel >= NUM_CHANNELS)
	{
		return -1;
	}

	
	
	if (channels_playing[channel]) {
		voice_stop(allegro_voices[channel]);
		channels_playing[channel] = NULL;
	}

	
	if (sfxinfo->driver_data == NULL)
	{
		if (!CacheSFX(sfxinfo))
		{
			return -1;
		}
	}
	assert(sfxinfo->driver_data);

	
	reallocate_voice(allegro_voices[channel], sfxinfo->driver_data);
	voice_set_volume(allegro_voices[channel], vol);
	voice_set_pan(allegro_voices[channel], sep);
	voice_start(allegro_voices[channel]);

	channels_playing[channel] = sfxinfo;

	return channel;
}


static void I_Allegro_StopSound(int handle)
{
	if (!sound_initialized || handle < 0 || handle >= NUM_CHANNELS)
	{
		return;
	}

	if (channels_playing[handle] == NULL) {
		return;
	}

	voice_stop(allegro_voices[handle]);
	channels_playing[handle] = NULL;
}


static boolean I_Allegro_SoundIsPlaying(int handle)
{
	int position;

	if (!sound_initialized || handle < 0 || handle >= NUM_CHANNELS)
	{
		return false;
	}

	if (channels_playing[handle] == NULL) {
		return false;
	}

	position = voice_get_position(allegro_voices[handle]);
	if (position < 0) {
		
		return false;
	}

	
	return true;
}





static void I_Allegro_UpdateSound(void)
{
	int i;

	
	for (i = 0; i < NUM_CHANNELS; i++) {
		if (channels_playing[i]) {
			int position = voice_get_position(allegro_voices[i]);
			if (position < 0) {
				
				channels_playing[i] = NULL;
			}
		}
	}
}


static void I_Allegro_ShutdownSound(void)
{
	int i;

	if (!sound_initialized)
	{
		return;
	}

	for (i = 0; i < NUM_CHANNELS; i++) {
		if (channels_playing[i] != NULL) {
			voice_stop(allegro_voices[i]);
			channels_playing[i] = NULL;
		}
		deallocate_voice(allegro_voices[i]);
		allegro_voices[i] = -1;
	}

	destroy_sample(dummy_sample);
	dummy_sample = NULL;

	remove_sound();

	sound_initialized = false;
}


static boolean I_Allegro_InitSound(boolean _use_sfx_prefix)
{
	int i;

	use_sfx_prefix = _use_sfx_prefix;

	

	for (i=0; i<NUM_CHANNELS; ++i)
	{
		channels_playing[i] = NULL;
	}

	reserve_voices(NUM_CHANNELS, NUM_MIDI_CHANNELS);

	i = install_sound(DIGI_AUTODETECT, MIDI_AUTODETECT, "allegro.cfg");
	if (i < 0) {
		printf("install_sound failed: %d \"%s\"\n", i, allegro_error);
		return false;
	}

	printf("Allegro sound initialized\n");

	printf("Mixer quality %d\n",       get_mixer_quality());
	printf("Mixer freq %d\n",          get_mixer_frequency());
	printf("Mixer bits %d\n",          get_mixer_bits());
	printf("Mixer channels %d\n",      get_mixer_channels());
	printf("Mixer voices %d\n",        get_mixer_voices());
	printf("Mixer buffer length %d\n", get_mixer_buffer_length());

	
	dummy_sample = create_sample(8, 0, 11025, 8);
	if (dummy_sample == NULL) {
		printf("Failed to create sound sample: %s\n", allegro_error);
		return false;
	}

	for (i = 0; i < NUM_CHANNELS; i++) {
		allegro_voices[i] = allocate_voice(dummy_sample);
	}

	sound_initialized = true;

	return true;
}


static snddevice_t sound_allegro_devices[] =
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
	sound_allegro_devices,
	arrlen(sound_allegro_devices),
	I_Allegro_InitSound,
	I_Allegro_ShutdownSound,
	I_Allegro_GetSfxLumpNum,
	I_Allegro_UpdateSound,
	I_Allegro_UpdateSoundParams,
	I_Allegro_StartSound,
	I_Allegro_StopSound,
	I_Allegro_SoundIsPlaying,
	I_Allegro_PrecacheSounds,
};


#include <stdio.h>

#include "doomtype.h"
#include "i_swap.h"

#include "memio.h"
#include "mus2mid.h"

#define NUM_CHANNELS 16

#define MIDI_PERCUSSION_CHAN 9
#define MUS_PERCUSSION_CHAN 15

typedef enum
{
    mus_releasekey = 0x00,
    mus_presskey = 0x10,
    mus_pitchwheel = 0x20,
    mus_systemevent = 0x30,
    mus_changecontroller = 0x40,
    mus_scoreend = 0x60
} musevent;

typedef enum
{
    midi_releasekey = 0x80,
    midi_presskey = 0x90,
    midi_aftertouchkey = 0xA0,
    midi_changecontroller = 0xB0,
    midi_changepatch = 0xC0,
    midi_aftertouchchannel = 0xD0,
    midi_pitchwheel = 0xE0
} midievent;

typedef struct 
{
    byte id[4];
    unsigned short scorelength;
    unsigned short scorestart;
    unsigned short primarychannels;
    unsigned short secondarychannels;
    unsigned short instrumentcount;
} musheader;

static const byte midiheader[] =
{
    'M', 'T', 'h', 'd',     
    0x00, 0x00, 0x00, 0x06, 
    0x00, 0x00,             
    0x00, 0x01,             
    0x00, 0x46,             
    'M', 'T', 'r', 'k',        
    0x00, 0x00, 0x00, 0x00  
};

static byte channelvelocities[] =
{
    127, 127, 127, 127, 127, 127, 127, 127,
    127, 127, 127, 127, 127, 127, 127, 127
};

static unsigned int queuedtime = 0;

static unsigned int tracksize;

static const byte controller_map[] =
{
    0x00, 0x20, 0x01, 0x07, 0x0A, 0x0B, 0x5B, 0x5D,
    0x40, 0x43, 0x78, 0x7B, 0x7E, 0x7F, 0x79
};

static int channel_map[NUM_CHANNELS];

static boolean WriteTime(unsigned int time, MEMFILE *midioutput)
{
    unsigned int buffer = time & 0x7F;
    byte writeval;

    while ((time >>= 7) != 0)
    {
        buffer <<= 8;
        buffer |= ((time & 0x7F) | 0x80);
    }

    for (;;)
    {
        writeval = (byte)(buffer & 0xFF);

        if (mem_fwrite(&writeval, 1, 1, midioutput) != 1)
        {
            return true;
        }

        ++tracksize;

        if ((buffer & 0x80) != 0)
        {
            buffer >>= 8;
        }
        else
        {
            queuedtime = 0;
            return false;
        }
    }
}

static boolean WriteEndTrack(MEMFILE *midioutput)
{
    byte endtrack[] = {0xFF, 0x2F, 0x00};

    if (WriteTime(queuedtime, midioutput))
    {
        return true;
    }

    if (mem_fwrite(endtrack, 1, 3, midioutput) != 3)
    {
        return true;
    }

    tracksize += 3;
    return false;
}

static boolean WritePressKey(byte channel, byte key,
                             byte velocity, MEMFILE *midioutput)
{
    byte working = midi_presskey | channel;

    if (WriteTime(queuedtime, midioutput))
    {
        return true;
    }

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = key & 0x7F;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = velocity & 0x7F;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    tracksize += 3;

    return false;
}

static boolean WriteReleaseKey(byte channel, byte key,
                               MEMFILE *midioutput)
{
    byte working = midi_releasekey | channel;

    if (WriteTime(queuedtime, midioutput))
    {
        return true;
    }

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = key & 0x7F;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = 0;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    tracksize += 3;

    return false;
}

static boolean WritePitchWheel(byte channel, short wheel,
                               MEMFILE *midioutput)
{
    byte working = midi_pitchwheel | channel;

    if (WriteTime(queuedtime, midioutput))
    {
        return true;
    }

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = wheel & 0x7F;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = (wheel >> 7) & 0x7F;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    tracksize += 3;
    return false;
}

static boolean WriteChangePatch(byte channel, byte patch,
                                MEMFILE *midioutput)
{
    byte working = midi_changepatch | channel;

    if (WriteTime(queuedtime, midioutput))
    {
        return true;
    }

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = patch & 0x7F;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    tracksize += 2;

    return false;
}

static boolean WriteChangeController_Valued(byte channel,
                                            byte control,
                                            byte value,
                                            MEMFILE *midioutput)
{
    byte working = midi_changecontroller | channel;

    if (WriteTime(queuedtime, midioutput))
    {
        return true;
    }

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = control & 0x7F;

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    working = value; 

    if (working & 0x80)
    {
        working = 0x7F;
    }

    if (mem_fwrite(&working, 1, 1, midioutput) != 1)
    {
        return true;
    }

    tracksize += 3;

    return false;
}

static boolean WriteChangeController_Valueless(byte channel,
                                               byte control,
                                               MEMFILE *midioutput)
{
    return WriteChangeController_Valued(channel, control, 0,
                                             midioutput);
}

static int AllocateMIDIChannel(void)
{
    int result;
    int max;
    int i;

    max = -1;

    for (i=0; i<NUM_CHANNELS; ++i)
    {
        if (channel_map[i] > max)
        {
            max = channel_map[i];
        }
    }

    result = max + 1;

    if (result == MIDI_PERCUSSION_CHAN)
    {
        ++result;
    }

    return result;
}

static int GetMIDIChannel(int mus_channel, MEMFILE *midioutput)
{

    if (mus_channel == MUS_PERCUSSION_CHAN)
    {
        return MIDI_PERCUSSION_CHAN;
    }
    else
    {

        if (channel_map[mus_channel] == -1)
        {
            channel_map[mus_channel] = AllocateMIDIChannel();

            WriteChangeController_Valueless(channel_map[mus_channel], 0x7b,
                                            midioutput);
        }

        return channel_map[mus_channel];
    }
}

static boolean ReadMusHeader(MEMFILE *file, musheader *header)
{
    boolean result;

    result = mem_fread(&header->id, sizeof(byte), 4, file) == 4
          && mem_fread(&header->scorelength, sizeof(short), 1, file) == 1
          && mem_fread(&header->scorestart, sizeof(short), 1, file) == 1
          && mem_fread(&header->primarychannels, sizeof(short), 1, file) == 1
          && mem_fread(&header->secondarychannels, sizeof(short), 1, file) == 1
          && mem_fread(&header->instrumentcount, sizeof(short), 1, file) == 1;

    if (result)
    {
        header->scorelength = SHORT(header->scorelength);
        header->scorestart = SHORT(header->scorestart);
        header->primarychannels = SHORT(header->primarychannels);
        header->secondarychannels = SHORT(header->secondarychannels);
        header->instrumentcount = SHORT(header->instrumentcount);
    }

    return result;
}

boolean mus2mid(MEMFILE *musinput, MEMFILE *midioutput)
{

    musheader musfileheader;

    byte eventdescriptor;
    int channel; 
    musevent event;

    byte key;
    byte controllernumber;
    byte controllervalue;

    byte tracksizebuffer[4];

    int hitscoreend = 0;

    byte working;

    unsigned int timedelay;

    for (channel=0; channel<NUM_CHANNELS; ++channel)
    {
        channel_map[channel] = -1;
    }

    if (!ReadMusHeader(musinput, &musfileheader))
    {
        return true;
    }

#define CHECK_MUS_HEADER
#ifdef CHECK_MUS_HEADER

    if (musfileheader.id[0] != 'M'
     || musfileheader.id[1] != 'U'
     || musfileheader.id[2] != 'S'
     || musfileheader.id[3] != 0x1A)
    {
        return true;
    }
#endif

    if (mem_fseek(musinput, (long)musfileheader.scorestart,
                  MEM_SEEK_SET) != 0)
    {
        return true;
    }

    mem_fwrite(midiheader, 1, sizeof(midiheader), midioutput);
    tracksize = 0;

    while (!hitscoreend)
    {

        while (!hitscoreend)
        {

            if (mem_fread(&eventdescriptor, 1, 1, musinput) != 1)
            {
                return true;
            }

            channel = GetMIDIChannel(eventdescriptor & 0x0F, midioutput);
            event = eventdescriptor & 0x70;

            switch (event)
            {
                case mus_releasekey:
                    if (mem_fread(&key, 1, 1, musinput) != 1)
                    {
                        return true;
                    }

                    if (WriteReleaseKey(channel, key, midioutput))
                    {
                        return true;
                    }

                    break;

                case mus_presskey:
                    if (mem_fread(&key, 1, 1, musinput) != 1)
                    {
                        return true;
                    }

                    if (key & 0x80)
                    {
                        if (mem_fread(&channelvelocities[channel], 1, 1, musinput) != 1)
                        {
                            return true;
                        }

                        channelvelocities[channel] &= 0x7F;
                    }

                    if (WritePressKey(channel, key,
                                      channelvelocities[channel], midioutput))
                    {
                        return true;
                    }

                    break;

                case mus_pitchwheel:
                    if (mem_fread(&key, 1, 1, musinput) != 1)
                    {
                        break;
                    }
                    if (WritePitchWheel(channel, (short)(key * 64), midioutput))
                    {
                        return true;
                    }

                    break;

                case mus_systemevent:
                    if (mem_fread(&controllernumber, 1, 1, musinput) != 1)
                    {
                        return true;
                    }
                    if (controllernumber < 10 || controllernumber > 14)
                    {
                        return true;
                    }

                    if (WriteChangeController_Valueless(channel,
                                                        controller_map[controllernumber],
                                                        midioutput))
                    {
                        return true;
                    }

                    break;

                case mus_changecontroller:
                    if (mem_fread(&controllernumber, 1, 1, musinput) != 1)
                    {
                        return true;
                    }

                    if (mem_fread(&controllervalue, 1, 1, musinput) != 1)
                    {
                        return true;
                    }

                    if (controllernumber == 0)
                    {
                        if (WriteChangePatch(channel, controllervalue,
                                             midioutput))
                        {
                            return true;
                        }
                    }
                    else
                    {
                        if (controllernumber < 1 || controllernumber > 9)
                        {
                            return true;
                        }

                        if (WriteChangeController_Valued(channel,
                                                         controller_map[controllernumber],
                                                         controllervalue,
                                                         midioutput))
                        {
                            return true;
                        }
                    }

                    break;

                case mus_scoreend:
                    hitscoreend = 1;
                    break;

                default:
                    return true;
                    break;
            }

            if (eventdescriptor & 0x80)
            {
                break;
            }
        }

        if (!hitscoreend)
        {
            timedelay = 0;
            for (;;)
            {
                if (mem_fread(&working, 1, 1, musinput) != 1)
                {
                    return true;
                }

                timedelay = timedelay * 128 + (working & 0x7F);
                if ((working & 0x80) == 0)
                {
                    break;
                }
            }
            queuedtime += timedelay;
        }
    }

    if (WriteEndTrack(midioutput))
    {
        return true;
    }

    if (mem_fseek(midioutput, 18, MEM_SEEK_SET))
    {
        return true;
    }

    tracksizebuffer[0] = (tracksize >> 24) & 0xff;
    tracksizebuffer[1] = (tracksize >> 16) & 0xff;
    tracksizebuffer[2] = (tracksize >> 8) & 0xff;
    tracksizebuffer[3] = tracksize & 0xff;

    if (mem_fwrite(tracksizebuffer, 1, 4, midioutput) != 4)
    {
        return true;
    }

    return false;
}

#ifdef STANDALONE

#include "m_misc.h"
#include "z_zone.h"

int main(int argc, char *argv[])
{
    MEMFILE *src, *dst;
    byte *infile;
    long infile_len;
    void *outfile;
    size_t outfile_len;

    if (argc != 3)
    {
        printf("Usage: %s <musfile> <midfile>\n", argv[0]);
        exit(-1);
    }

    Z_Init();

    infile_len = M_ReadFile(argv[1], &infile);

    src = mem_fopen_read(infile, infile_len);
    dst = mem_fopen_write();

    if (mus2mid(src, dst))
    {
        fprintf(stderr, "mus2mid() failed\n");
        exit(-1);
    }

    mem_get_buf(dst, &outfile, &outfile_len);

    M_WriteFile(argv[2], outfile, outfile_len);

    return 0;
}

#endif


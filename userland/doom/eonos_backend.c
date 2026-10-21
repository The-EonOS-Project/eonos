#include <stdint.h>
#include "doomgeneric.h"

extern void eonos_doom_draw_frame(const uint32_t *pixels);
extern void eonos_doom_sleep_ms(uint32_t milliseconds);
extern uint32_t eonos_doom_ticks_ms(void);
extern int eonos_doom_key_event(int *pressed, unsigned char *key);

void DG_Init(void) { }

void DG_DrawFrame(void)
{
    eonos_doom_draw_frame((const uint32_t *)DG_ScreenBuffer);
}

void DG_SleepMs(uint32_t milliseconds)
{
    eonos_doom_sleep_ms(milliseconds);
}

uint32_t DG_GetTicksMs(void)
{
    return eonos_doom_ticks_ms();
}

int DG_GetKey(int *pressed, unsigned char *key)
{
    return eonos_doom_key_event(pressed, key);
}

void DG_SetWindowTitle(const char *title) { (void)title; }

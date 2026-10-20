//! Shared pointer state produced by USB HID boot-mouse reports.

use crate::sync::IrqMutex;

/// Current absolute pointer position, button mask, and accumulated wheel.
/// Button bits 0 through 4 follow the USB HID boot-mouse report layout.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
    pub _padding: [u8; 3],
    pub wheel: i32,
}

static STATE: IrqMutex<MouseState> = IrqMutex::new(MouseState {
    x: 0,
    y: 0,
    buttons: 0,
    _padding: [0; 3],
    wheel: 0,
});

pub fn init() {
    let (width, height) = crate::framebuffer::FRAMEBUFFER
        .lock()
        .as_ref()
        .map(|fb| (fb.width, fb.height))
        .unwrap_or((1, 1));
    let mut state = STATE.lock();
    state.x = (width.saturating_sub(1) / 2) as i32;
    state.y = (height.saturating_sub(1) / 2) as i32;
    state.buttons = 0;
    state.wheel = 0;
}

pub fn state() -> MouseState {
    *STATE.lock()
}

pub(super) fn handle_report(report: &[u8]) {
    let (width, height) = crate::framebuffer::FRAMEBUFFER
        .lock()
        .as_ref()
        .map(|fb| (fb.width, fb.height))
        .unwrap_or((1, 1));
    let mut state = STATE.lock();
    // Some receivers advertise a boot mouse interface but only emit their
    // Report Protocol descriptor: report ID, buttons, padding, 16-bit X/Y,
    // wheel, and consumer-control byte (8 bytes total for the YICHIP
    // 3151:3020 receiver). Boot-protocol reports remain supported too.
    let (buttons, dx, dy, wheel) = if report.len() >= 8 && report[0] == 1 {
        (
            report[1] & 0x1f,
            i16::from_le_bytes([report[2], report[3]]) as i32,
            i16::from_le_bytes([report[4], report[5]]) as i32,
            report[6] as i8 as i32,
        )
    } else if report.len() >= 6 && report[0] == 1 {
        // The Trust GXT 161 (145f:024f) uses Report ID 1 with a compact
        // six-byte report: ID, buttons, relative X, relative Y, wheel, and
        // horizontal pan. Its endpoint can carry seven bytes, but the HID
        // input report itself is six bytes. Ignore horizontal pan for now.
        (
            report[1] & 0x1f,
            report[2] as i8 as i32,
            report[3] as i8 as i32,
            report[4] as i8 as i32,
        )
    } else if report.len() > 4 {
        // Longer packets use a report-ID layout. Only the mouse report IDs
        // above are understood; ignore vendor/consumer reports rather than
        // treating their report ID and payload as buttons and motion.
        return;
    } else if report.len() >= 3 {
        (
            report[0] & 0x1f,
            report[1] as i8 as i32,
            report[2] as i8 as i32,
            report.get(3).copied().unwrap_or(0) as i8 as i32,
        )
    } else {
        return;
    };
    state.x = state
        .x
        .saturating_add(dx)
        .clamp(0, width.saturating_sub(1) as i32);
    state.y = state
        .y
        .saturating_add(dy)
        .clamp(0, height.saturating_sub(1) as i32);
    state.buttons = buttons;
    state.wheel = state.wheel.saturating_add(wheel);
}

pub(super) fn clear_buttons() {
    STATE.lock().buttons = 0;
}

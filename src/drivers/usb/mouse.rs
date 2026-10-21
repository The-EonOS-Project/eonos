use crate::sync::IrqMutex;

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

    let (buttons, dx, dy, wheel) = if report.len() >= 8 && report[0] == 1 {
        (
            report[1] & 0x1f,
            i16::from_le_bytes([report[2], report[3]]) as i32,
            i16::from_le_bytes([report[4], report[5]]) as i32,
            report[6] as i8 as i32,
        )
    } else if report.len() >= 6 && report[0] == 1 {

        (
            report[1] & 0x1f,
            report[2] as i8 as i32,
            report[3] as i8 as i32,
            report[4] as i8 as i32,
        )
    } else if report.len() > 4 {

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

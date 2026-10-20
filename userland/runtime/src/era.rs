//! Small user-space drawing API for the Era graphical environment.
//!
//! Rectangles and text are drawn into a RAM shadow surface. [`present`] copies
//! only changed pixels to the display and updates the mouse cursor separately.

use super::{checked, syscall3, syscall6, Error};

const SYS_ERA_GRAPHICS: usize = 9021;
const ENTER: usize = 0;
const FILL_RECT: usize = 1;
const TEXT: usize = 2;
const PRESENT: usize = 3;
const EXIT: usize = 4;

/// Display dimensions returned when a process acquires the Era surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
}

/// Begin an exclusive graphics session and return the framebuffer size.
pub fn enter() -> Result<DisplayInfo, Error> {
    let mut info = DisplayInfo::default();
    checked(unsafe {
        syscall3(
            SYS_ERA_GRAPHICS,
            ENTER,
            (&mut info as *mut DisplayInfo) as usize,
            0,
        )
    })?;
    Ok(info)
}

/// Fill a clipped rectangle using an opaque `0xRRGGBB` color.
pub fn fill_rect(x: i32, y: i32, width: u32, height: u32, color: u32) -> Result<(), Error> {
    checked(unsafe {
        syscall6(
            SYS_ERA_GRAPHICS,
            FILL_RECT,
            x as isize as usize,
            y as isize as usize,
            width as usize,
            height as usize,
            color as usize,
        )
    })
    .map(|_| ())
}

/// Draw PSF-font text without painting over its background.
pub fn text(x: i32, y: i32, color: u32, text: &str) -> Result<(), Error> {
    checked(unsafe {
        syscall6(
            SYS_ERA_GRAPHICS,
            TEXT,
            x as isize as usize,
            y as isize as usize,
            color as usize,
            text.as_ptr() as usize,
            text.len(),
        )
    })
    .map(|_| ())
}

/// Present accumulated drawing changes and update the hardware-independent
/// cursor. With no drawing changes, this only copies the cursor's small region.
pub fn present() -> Result<(), Error> {
    checked(unsafe { syscall3(SYS_ERA_GRAPHICS, PRESENT, 0, 0) }).map(|_| ())
}

/// Restore the terminal display and release the graphics surface.
pub fn exit() -> Result<(), Error> {
    checked(unsafe { syscall3(SYS_ERA_GRAPHICS, EXIT, 0, 0) }).map(|_| ())
}

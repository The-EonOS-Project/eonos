




use super::{checked, syscall3, syscall6, Error};

const SYS_ERA_GRAPHICS: usize = 9021;
const ENTER: usize = 0;
const FILL_RECT: usize = 1;
const TEXT: usize = 2;
const PRESENT: usize = 3;
const EXIT: usize = 4;


#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
}


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



pub fn present() -> Result<(), Error> {
    checked(unsafe { syscall3(SYS_ERA_GRAPHICS, PRESENT, 0, 0) }).map(|_| ())
}


pub fn exit() -> Result<(), Error> {
    checked(unsafe { syscall3(SYS_ERA_GRAPHICS, EXIT, 0, 0) }).map(|_| ())
}

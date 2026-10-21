#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::alloc::{alloc, dealloc};
use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::ffi::{c_char, CStr};
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use eonos_rt::graphics::Surface;

const FRAME_WIDTH: u32 = 320;
const FRAME_HEIGHT: u32 = 200;
const IWAD_PATH: &str = "/doom/freedoom1.wad";

extern "C" {
    fn doomgeneric_Create(argc: i32, argv: *mut *mut c_char);
    fn doomgeneric_Tick();
}

static SURFACE: AtomicPtr<Surface> = AtomicPtr::new(core::ptr::null_mut());
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

fn parse_iwad_path(args: eonos_rt::Args<'_>) -> Result<String, &'static str> {
    let mut path = String::from(IWAD_PATH);
    let mut args = args.skip(1);
    while let Some(arg) = args.next() {
        if arg.to_bytes() != b"-iwad" {
            return Err("usage: doom [-iwad /path/to/game.wad]");
        }
        let Some(value) = args.next() else {
            return Err("usage: doom [-iwad /path/to/game.wad]");
        };
        let value = value.to_str().map_err(|_| "IWAD path must be UTF-8")?;
        if !value.starts_with('/') || value.len() > 255 {
            return Err("IWAD path must be absolute and at most 255 bytes");
        }
        path.clear();
        path.push_str(value);
    }
    Ok(path)
}

fn check_iwad(path: &str) -> Result<(), &'static str> {
    let fd = eonos_rt::open(path, eonos_rt::O_RDONLY).map_err(|_| "could not open IWAD")?;
    let result = (|| {
        let mut header = [0u8; 12];
        let read = eonos_rt::read(fd, &mut header).map_err(|_| "could not read IWAD header")?;
        if read != header.len() || &header[..4] != b"IWAD" {
            return Err("invalid IWAD header");
        }
        let lumps = u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64;
        let directory = u32::from_le_bytes(header[8..12].try_into().unwrap()) as u64;
        let length = eonos_rt::seek(fd, 0, 2).map_err(|_| "could not determine IWAD length")? as u64;
        let valid = lumps != 0
            && directory >= 12
            && lumps.checked_mul(16).and_then(|bytes| directory.checked_add(bytes))
                .is_some_and(|end| end <= length);
        if valid { Ok(()) } else { Err("invalid or truncated IWAD directory") }
    })();
    let _ = eonos_rt::close(fd);
    result
}

fn doom_key(usage: u8) -> u8 {
    const KEY_RIGHT: u8 = 0xAE;
    const KEY_LEFT: u8 = 0xAC;
    const KEY_UP: u8 = 0xAD;
    const KEY_DOWN: u8 = 0xAF;
    const KEY_USE: u8 = 0xA2;
    const KEY_FIRE: u8 = 0xA3;
    const KEY_RSHIFT: u8 = 0xB6;
    const KEY_ALT: u8 = 0xB8;
    match usage {
        0x04..=0x1D => b'a' + usage - 0x04,
        0x1E..=0x26 => b'1' + usage - 0x1E,
        0x27 => b'0',
        0x28 | 0x58 => 13,
        0x29 => 27,
        0x2A => 127,
        0x2B => 9,
        0x2C => KEY_USE,
        0x2D => b'-', 0x2E => b'=', 0x2F => b'[', 0x30 => b']',
        0x31 | 0x32 => b'\\', 0x33 => b';', 0x34 => b'\'', 0x35 => b'`',
        0x36 => b',', 0x37 => b'.', 0x38 => b'/',
        0x39 => 0xBA,
        0x3A..=0x43 => 0xBB + usage - 0x3A,
        0x44 => 0xD7, 0x45 => 0xD8,
        0x49 => 0xD2, 
        0x4A => 0xC7, 
        0x4B => 0xC9, 
        0x4C => 0xD3, 
        0x4D => 0xCF, 
        0x4E => 0xD1, 
        0x4F => KEY_RIGHT, 0x50 => KEY_LEFT, 0x51 => KEY_DOWN, 0x52 => KEY_UP,
        0x53 => 0xC5, 
        0x55 => b'*', 0x56 => b'-', 0x57 => b'+',
        0x59 => 0xCF, 0x5A => KEY_DOWN, 0x5B => 0xD1, 0x5C => KEY_LEFT,
        0x5E => KEY_RIGHT, 0x5F => 0xC7, 0x60 => KEY_UP, 0x61 => 0xC9,
        0xE0 | 0xE4 => KEY_FIRE,
        0xE1 | 0xE5 => KEY_RSHIFT,
        0xE2 | 0xE6 => KEY_ALT,
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn eonos_doom_draw_frame(pixels: *const u32) {
    if pixels.is_null() { EXIT_REQUESTED.store(true, Ordering::Release); return; }
    let surface = SURFACE.load(Ordering::Acquire);
    if surface.is_null() { EXIT_REQUESTED.store(true, Ordering::Release); return; }
    let frame = unsafe { core::slice::from_raw_parts(pixels, (FRAME_WIDTH * FRAME_HEIGHT) as usize) };
    let result = unsafe { &mut *surface };
    if result.blit_scaled(FRAME_WIDTH, FRAME_HEIGHT, frame).is_err()
        || result.present_without_cursor().is_err() {
        if !EXIT_REQUESTED.swap(true, Ordering::AcqRel) {
            eonos_rt::eprintln!("doom: EonOS graphics presentation failed");
        }
    }
}

#[no_mangle]
pub extern "C" fn eonos_doom_sleep_ms(milliseconds: u32) {
    if milliseconds == 0 { eonos_rt::yield_now(); }
    else if eonos_rt::sleep_ms(milliseconds as usize).is_err() { EXIT_REQUESTED.store(true, Ordering::Release); }
}

#[no_mangle]
pub extern "C" fn eonos_doom_ticks_ms() -> u32 {
    eonos_rt::uptime_ms() as u32
}

#[no_mangle]
pub extern "C" fn eonos_doom_key_event(pressed: *mut i32, key: *mut u8) -> i32 {
    if pressed.is_null() || key.is_null() { return 0; }
    loop {
        let event = match eonos_rt::keyboard_event() {
            Ok(Some(event)) => event,
            Ok(None) => return 0,
            Err(_) => { EXIT_REQUESTED.store(true, Ordering::Release); return 0; }
        };
        if event.usage == 0x14 && event.pressed != 0 && event.modifiers & 0x11 != 0 {
            eonos_rt::println!("doom: received Ctrl+Q; returning to Era");
            EXIT_REQUESTED.store(true, Ordering::Release);
            continue;
        }
        let translated = doom_key(event.usage);
        if translated == 0 { continue; }
        unsafe { *pressed = event.pressed as i32; *key = translated; }
        return 1;
    }
}

#[no_mangle]
pub extern "C" fn eonos_doom_alloc(size: usize) -> *mut u8 {
    let Some(layout) = Layout::from_size_align(size.max(1), 16).ok() else { return core::ptr::null_mut(); };
    unsafe { alloc(layout) }
}

#[no_mangle]
pub extern "C" fn eonos_doom_free(pointer: *mut u8) {
    if !pointer.is_null() {
        if let Ok(layout) = Layout::from_size_align(1, 16) { unsafe { dealloc(pointer, layout); } }
    }
}

#[no_mangle]
pub extern "C" fn eonos_doom_open(path: *const c_char, flags: i32) -> isize {
    if path.is_null() { return -14; }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else { return -22; };
    eonos_rt::open(path, flags as usize).map(|fd| fd as isize).unwrap_or_else(|error| -(error.0 as isize))
}

#[no_mangle]
pub extern "C" fn eonos_doom_read(fd: i32, buffer: *mut u8, count: usize) -> isize {
    if count == 0 { return 0; }
    if buffer.is_null() && count != 0 { return -14; }
    let bytes = unsafe { core::slice::from_raw_parts_mut(buffer, count) };
    eonos_rt::read(fd as usize, bytes).map(|n| n as isize).unwrap_or_else(|error| -(error.0 as isize))
}

#[no_mangle]
pub extern "C" fn eonos_doom_write(fd: i32, buffer: *const u8, count: usize) -> isize {
    if count == 0 { return 0; }
    if buffer.is_null() && count != 0 { return -14; }
    let bytes = unsafe { core::slice::from_raw_parts(buffer, count) };
    eonos_rt::write(fd as usize, bytes).map(|n| n as isize).unwrap_or_else(|error| -(error.0 as isize))
}

#[no_mangle]
pub extern "C" fn eonos_doom_seek(fd: i32, offset: isize, whence: i32) -> isize {
    eonos_rt::seek(fd as usize, offset as i64, whence as usize)
        .map(|n| n as isize).unwrap_or_else(|error| -(error.0 as isize))
}

#[no_mangle]
pub extern "C" fn eonos_doom_close(fd: i32) -> isize {
    eonos_rt::close(fd as usize).map(|_| 0).unwrap_or_else(|error| -(error.0 as isize))
}

#[no_mangle]
pub extern "C" fn eonos_doom_exit(status: i32) -> ! { eonos_rt::exit(status) }

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let iwad_path = match parse_iwad_path(args) {
        Ok(path) => path,
        Err(message) => { eonos_rt::eprintln!("doom: {}", message); return 2; }
    };
    if let Err(message) = check_iwad(&iwad_path) {
        eonos_rt::eprintln!("doom: IWAD '{}': {}", iwad_path, message);
        return 2;
    }
    let mut surface = match Surface::open() {
        Ok(surface) => surface,
        Err(error) => { eonos_rt::eprintln!("doom: could not acquire display ({})", error.0); return 1; }
    };
    if let Err(error) = eonos_rt::keyboard_claim() {
        eonos_rt::eprintln!("doom: could not acquire keyboard ({})", error.0);
        return 1;
    }
    let pointer = &mut surface as *mut Surface;
    SURFACE.store(pointer, Ordering::Release);
    EXIT_REQUESTED.store(false, Ordering::Release);

    let mut arg0 = *b"/bin/doom\0";
    let mut arg1 = *b"-iwad\0";
    let mut arg2 = Vec::with_capacity(iwad_path.len() + 1);
    arg2.extend_from_slice(iwad_path.as_bytes());
    arg2.push(0);
    let mut arg3 = *b"-nosound\0";
    let mut argv = [
        arg0.as_mut_ptr() as *mut c_char,
        arg1.as_mut_ptr() as *mut c_char,
        arg2.as_mut_ptr() as *mut c_char,
        arg3.as_mut_ptr() as *mut c_char,
    ];
    unsafe { doomgeneric_Create(argv.len() as i32, argv.as_mut_ptr()); }
    eonos_rt::println!("doom: engine initialized; rendering game frames");
    while !EXIT_REQUESTED.load(Ordering::Acquire) {
        unsafe { doomgeneric_Tick(); }
    }
    eonos_rt::println!("doom: game loop stopped; releasing resources");
    SURFACE.store(core::ptr::null_mut(), Ordering::Release);
    let _ = eonos_rt::keyboard_release();
    drop(surface);
    0
}

eonos_rt::entry!(main);

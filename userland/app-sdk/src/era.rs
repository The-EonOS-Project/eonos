//! Safe app-window API. The window contents are sent to the Era compositor
//! using a bounded drawing-command protocol over standard input and output.

use alloc::string::String;
use alloc::vec::Vec;
use eonos_rt::{Error, Result};

const MAGIC: &[u8; 4] = b"EAPP";
const PACKET_INIT: u8 = 1;
const PACKET_FRAME: u8 = 2;
const PACKET_CLOSE: u8 = 3;
const EVENT_SIZE: usize = 16;
const MAX_FRAME_BYTES: usize = 24 * 1024;
const MAX_COMMANDS: u16 = 256;
const MAX_TEXT_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Resized { width: u32, height: u32 },
    Key { usage: u8, pressed: bool, modifiers: u8 },
    Pointer { x: i32, y: i32, buttons: u8 },
    Close,
}

/// A window whose drawing is validated and composited by Era.
pub struct Window {
    title: String,
    width: u32,
    height: u32,
    frame: Vec<u8>,
    command_count: u16,
    input: Vec<u8>,
}

impl Window {
    /// Request a window. Era clamps its size to the available work area and
    /// replies with the actual client dimensions before this returns.
    pub fn open(title: &str, width: u32, height: u32) -> Result<Self> {
        if title.is_empty() || title.len() > 80 || width == 0 || height == 0 || width > 1920 || height > 1080 {
            return Err(Error(22));
        }
        let mut payload = Vec::with_capacity(title.len() + 6);
        payload.extend_from_slice(&(width as u16).to_le_bytes());
        payload.extend_from_slice(&(height as u16).to_le_bytes());
        payload.extend_from_slice(&(title.len() as u16).to_le_bytes());
        payload.extend_from_slice(title.as_bytes());
        send_packet(PACKET_INIT, &payload)?;

        let mut response = [0u8; EVENT_SIZE];
        read_exact_stdin(&mut response)?;
        if response[0] != 1 { return Err(Error(5)); }
        Ok(Self {
            title: String::from(title),
            width: u32::from_le_bytes(response[1..5].try_into().unwrap()),
            height: u32::from_le_bytes(response[5..9].try_into().unwrap()),
            frame: Vec::new(),
            command_count: 0,
            input: Vec::new(),
        })
    }

    pub fn title(&self) -> &str { &self.title }
    pub fn dimensions(&self) -> (u32, u32) { (self.width, self.height) }

    /// Add a clipped rectangle to the next frame.
    pub fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: u32) -> Result<()> {
        if self.command_count >= MAX_COMMANDS || self.frame.len() + 21 > MAX_FRAME_BYTES { return Err(Error(7)); }
        self.frame.push(1);
        self.frame.extend_from_slice(&x.to_le_bytes());
        self.frame.extend_from_slice(&y.to_le_bytes());
        self.frame.extend_from_slice(&width.to_le_bytes());
        self.frame.extend_from_slice(&height.to_le_bytes());
        self.frame.extend_from_slice(&(color & 0x00ff_ffff).to_le_bytes());
        self.command_count += 1;
        Ok(())
    }

    /// Add foreground-only text using the system PSF font.
    pub fn text(&mut self, x: i32, y: i32, color: u32, value: &str) -> Result<()> {
        if value.len() > MAX_TEXT_BYTES || self.command_count >= MAX_COMMANDS
            || self.frame.len().saturating_add(11 + value.len()) > MAX_FRAME_BYTES {
            return Err(Error(7));
        }
        self.frame.push(2);
        self.frame.extend_from_slice(&x.to_le_bytes());
        self.frame.extend_from_slice(&y.to_le_bytes());
        self.frame.extend_from_slice(&(color & 0x00ff_ffff).to_le_bytes());
        self.frame.extend_from_slice(&(value.len() as u16).to_le_bytes());
        self.frame.extend_from_slice(value.as_bytes());
        self.command_count += 1;
        Ok(())
    }

    /// Replace the current window contents with the queued commands.
    pub fn present(&mut self) -> Result<()> {
        let mut payload = Vec::with_capacity(self.frame.len() + 2);
        payload.extend_from_slice(&self.command_count.to_le_bytes());
        payload.extend_from_slice(&self.frame);
        send_packet(PACKET_FRAME, &payload)?;
        self.frame.clear();
        self.command_count = 0;
        Ok(())
    }

    /// Poll one input event without blocking.
    pub fn poll_event(&mut self) -> Result<Option<Event>> {
        let mut chunk = [0u8; 256];
        let amount = eonos_rt::pipe_read_available(0, &mut chunk)?;
        self.input.extend_from_slice(&chunk[..amount]);
        if self.input.len() < EVENT_SIZE { return Ok(None); }
        let mut raw = [0u8; EVENT_SIZE];
        raw.copy_from_slice(&self.input[..EVENT_SIZE]);
        self.input.drain(..EVENT_SIZE);
        Ok(decode_event(&raw))
    }

    /// Close the Era window and terminate the application loop.
    pub fn close(self) -> Result<()> { send_packet(PACKET_CLOSE, &[]) }
}

fn send_packet(kind: u8, payload: &[u8]) -> Result<()> {
    if payload.len() > MAX_FRAME_BYTES { return Err(Error(7)); }
    let mut packet = Vec::with_capacity(payload.len() + 9);
    packet.extend_from_slice(MAGIC);
    packet.push(kind);
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(payload);
    eonos_rt::write_all(1, &packet)
}

fn read_exact_stdin(destination: &mut [u8]) -> Result<()> {
    let mut offset = 0;
    while offset < destination.len() {
        let amount = eonos_rt::read(0, &mut destination[offset..])?;
        if amount == 0 { return Err(Error(5)); }
        offset += amount;
    }
    Ok(())
}

fn decode_event(raw: &[u8; EVENT_SIZE]) -> Option<Event> {
    Some(match raw[0] {
        1 => Event::Resized {
            width: u32::from_le_bytes(raw[1..5].try_into().ok()?),
            height: u32::from_le_bytes(raw[5..9].try_into().ok()?),
        },
        2 => Event::Key { usage: raw[1], pressed: raw[2] != 0, modifiers: raw[3] },
        3 => Event::Pointer {
            x: i32::from_le_bytes(raw[1..5].try_into().ok()?),
            y: i32::from_le_bytes(raw[5..9].try_into().ok()?),
            buttons: raw[9],
        },
        4 => Event::Close,
        _ => return None,
    })
}

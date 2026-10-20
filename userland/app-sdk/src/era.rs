//! Safe app-window API. The window contents are sent to the Era compositor
//! using a bounded drawing-command protocol over standard input and output.
//! Widgets use immediate-mode drawing and pointer clicks collected by
//! [`Window::poll_event`]: poll events, redraw the controls, then call
//! [`Window::present`]. For example:
//!
//! ```ignore
//! let mut name = String::new();
//! let mut choice = 0;
//! let mut updates = false;
//! loop {
//!     while let Some(event) = window.poll_event()? {
//!         if event == Event::Close { return Ok(()); }
//!     }
//!     window.fill_rect(0, 0, 480, 300, 0x202A44)?;
//!     window.text_input(24, 24, 240, 32, &mut name, "Your name")?;
//!     window.dropdown(24, 72, 180, 28, &["Red", "Green", "Blue"], &mut choice)?;
//!     window.checkbox(24, 190, "Send updates", &mut updates)?;
//!     if window.button(24, 220, 140, 36, "Save")? { /* handle submit */ }
//!     window.present()?;
//! }
//! ```

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
const MAX_INPUT_BYTES: usize = 256;
const BUTTON_FACE: u32 = 0xC0C0C0;
const BUTTON_LIGHT: u32 = 0xFFFFFF;
const BUTTON_DARK: u32 = 0x404040;
const BUTTON_TEXT: u32 = 0x101010;

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
    pointer_clicks: Vec<(i32, i32)>,
    pointer_buttons: u8,
    open_dropdown: Option<(i32, i32)>,
    focused_widget: usize,
    dropdown_widgets: Vec<usize>,
    next_dropdown_widgets: Vec<usize>,
    text_widgets: Vec<usize>,
    next_text_widgets: Vec<usize>,
    pending_text_keys: Vec<(usize, u8, u8)>,
    cursor_widget: Option<usize>,
    text_cursors: Vec<(usize, usize)>,
    text_cursor: usize,
    widget_count: usize,
    next_widget: usize,
    activate_widget: Option<usize>,
    dropdown_step: i8,
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
            pointer_clicks: Vec::new(),
            pointer_buttons: 0,
            open_dropdown: None,
            focused_widget: 0,
            dropdown_widgets: Vec::new(),
            next_dropdown_widgets: Vec::new(),
            text_widgets: Vec::new(),
            next_text_widgets: Vec::new(),
            pending_text_keys: Vec::new(),
            cursor_widget: None,
            text_cursors: Vec::new(),
            text_cursor: 0,
            widget_count: 0,
            next_widget: 0,
            activate_widget: None,
            dropdown_step: 0,
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

    /// Draw a standard raised button and report whether it was clicked since
    /// the previous `present()`, or activated with Enter/Space while focused.
    /// Call this again each frame to redraw it. Tab and Shift+Tab move focus
    /// between widgets in the order they are drawn.
    pub fn button(&mut self, x: i32, y: i32, width: u32, height: u32, label: &str) -> Result<bool> {
        if width < 12 || height < 18 { return Err(Error(22)); }
        let index = self.register_widget();
        let mut focused = index == self.focused_widget;
        let pointer_clicked = self.take_click(x, y, width, height);
        if pointer_clicked { self.focused_widget = index; focused = true; }
        let clicked = pointer_clicked || (focused && self.take_activation(index));
        self.fill_rect(x, y, width, height, BUTTON_FACE)?;
        self.fill_rect(x, y, width, 1, BUTTON_LIGHT)?;
        self.fill_rect(x, y, 1, height, BUTTON_LIGHT)?;
        self.fill_rect(x + width as i32 - 1, y + 1, 1, height - 1, BUTTON_DARK)?;
        self.fill_rect(x + 1, y + height as i32 - 1, width - 2, 1, BUTTON_DARK)?;
        let label_width = (label.len() as i32).saturating_mul(8);
        self.text(x + ((width as i32 - label_width) / 2).max(3), y + (height as i32 - 16) / 2, BUTTON_TEXT, label)?;
        if focused { self.draw_focus(x, y, width, height)?; }
        Ok(clicked)
    }

    /// Draw a dropdown and update `selected` when an option is clicked.
    /// Returns true when the selected option changes. The menu opens below
    /// the control and shows at most 16 options at once.
    pub fn dropdown(&mut self, x: i32, y: i32, width: u32, height: u32, options: &[&str], selected: &mut usize) -> Result<bool> {
        if width < 24 || height < 18 || options.is_empty() || options.len() > 16 { return Err(Error(22)); }
        if *selected >= options.len() { *selected = 0; }
        let widget_index = self.register_widget();
        let mut focused = widget_index == self.focused_widget;
        if !self.next_dropdown_widgets.contains(&widget_index) {
            self.next_dropdown_widgets.push(widget_index);
        }
        let is_open = self.open_dropdown == Some((x, y));
        let mut changed = false;
        if focused && self.dropdown_step != 0 {
            let step = core::mem::replace(&mut self.dropdown_step, 0);
            let next = ((*selected as isize + step as isize).rem_euclid(options.len() as isize)) as usize;
            changed = next != *selected;
            *selected = next;
        }
        if focused && self.take_activation(widget_index) {
            if is_open { self.open_dropdown = None; }
            else { self.open_dropdown = Some((x, y)); }
        }
        if is_open && !focused { self.open_dropdown = None; }
        if is_open {
            if let Some((_, click_y)) = self.take_click_at(|cx, cy| {
                cx >= x && cx < x + width as i32
                    && cy >= y + height as i32
                    && cy < y + height as i32 * (options.len() as i32 + 1)
            }) {
                self.focused_widget = widget_index;
                focused = true;
                let index = ((click_y - y - height as i32) / height as i32) as usize;
                if index < options.len() && *selected != index {
                    *selected = index;
                    changed = true;
                }
                self.open_dropdown = None;
            } else if self.take_click(x, y, width, height) {
                self.focused_widget = widget_index;
                focused = true;
                self.open_dropdown = None;
            } else if self.take_click_at(|_, _| true).is_some() {
                self.open_dropdown = None;
            }
        } else if self.take_click(x, y, width, height) {
            self.focused_widget = widget_index;
            focused = true;
            self.open_dropdown = Some((x, y));
        }

        self.draw_control(x, y, width, height, options[*selected])?;
        self.text(x + width as i32 - 15, y + (height as i32 - 16) / 2, BUTTON_TEXT, "v")?;
        if focused { self.draw_focus(x, y, width, height)?; }
        if self.open_dropdown == Some((x, y)) {
            for (index, option) in options.iter().enumerate() {
                let row_y = y + height as i32 * (index as i32 + 1);
                let background = if index == *selected { 0xD8E5FA } else { 0xFFFFFF };
                self.fill_rect(x, row_y, width, height, background)?;
                self.fill_rect(x, row_y, width, 1, BUTTON_DARK)?;
                self.text(x + 5, row_y + (height as i32 - 16) / 2, BUTTON_TEXT, option)?;
            }
        }
        Ok(changed)
    }

    /// Draw a checkbox and toggle `checked` on a pointer click or focused
    /// Enter/Space. Returns true when its value changes.
    pub fn checkbox(&mut self, x: i32, y: i32, label: &str, checked: &mut bool) -> Result<bool> {
        if label.len() > MAX_TEXT_BYTES { return Err(Error(7)); }
        let index = self.register_widget();
        let focused = index == self.focused_widget;
        let width = 28u32.saturating_add((label.len() as u32).saturating_mul(8));
        let pointer_clicked = self.take_click(x, y, width, 22);
        let focused = if pointer_clicked { self.focused_widget = index; true } else { focused };
        let activated = pointer_clicked || (focused && self.take_activation(index));
        if activated { *checked = !*checked; }
        self.fill_rect(x, y + 1, 18, 18, 0xFFFFFF)?;
        self.fill_rect(x, y + 1, 18, 1, BUTTON_DARK)?;
        self.fill_rect(x, y + 1, 1, 18, BUTTON_DARK)?;
        self.fill_rect(x + 17, y + 2, 1, 17, BUTTON_LIGHT)?;
        self.fill_rect(x + 1, y + 18, 16, 1, BUTTON_LIGHT)?;
        if *checked {
            self.fill_rect(x + 4, y + 7, 3, 7, 0x12377B)?;
            self.fill_rect(x + 6, y + 11, 8, 3, 0x12377B)?;
        }
        self.text(x + 26, y + 2, BUTTON_TEXT, label)?;
        if focused { self.draw_focus(x, y, width.max(24), 22)?; }
        Ok(activated)
    }

    /// Draw a single-line text field. The value is edited with printable
    /// keyboard input, Backspace/Delete, Left/Right, and Home/End. Returns
    /// true if the value changes. Text is limited to 256 UTF-8 bytes.
    pub fn text_input(&mut self, x: i32, y: i32, width: u32, height: u32, value: &mut String, placeholder: &str) -> Result<bool> {
        if width < 40 || height < 24 || value.len() > MAX_INPUT_BYTES { return Err(Error(22)); }
        let index = self.register_widget();
        if !self.next_text_widgets.contains(&index) { self.next_text_widgets.push(index); }
        let mut focused = index == self.focused_widget;
        if self.cursor_widget != Some(index) {
            if let Some(previous) = self.cursor_widget {
                if let Some((_, cursor)) = self.text_cursors.iter_mut().find(|(widget, _)| *widget == previous) {
                    *cursor = self.text_cursor;
                } else {
                    self.text_cursors.push((previous, self.text_cursor));
                }
            }
            self.cursor_widget = Some(index);
            self.text_cursor = self.text_cursors.iter().find(|(widget, _)| *widget == index)
                .map(|(_, cursor)| *cursor).unwrap_or(value.len());
        }
        self.text_cursor = self.text_cursor.min(value.len());
        while !value.is_char_boundary(self.text_cursor) { self.text_cursor -= 1; }
        if self.take_click(x, y, width, height) {
            self.focused_widget = index;
            focused = true;
            self.text_cursor = value.len();
        }

        let mut changed = false;
        let mut key_index = 0;
        while key_index < self.pending_text_keys.len() {
            if self.pending_text_keys[key_index].0 != index { key_index += 1; continue; }
            let (_, usage, modifiers) = self.pending_text_keys.remove(key_index);
            let shift = modifiers & 0x22 != 0;
            match usage {
                0x2a if self.text_cursor > 0 => {
                    let previous = value[..self.text_cursor].char_indices().next_back().map(|(at, _)| at).unwrap_or(0);
                    value.drain(previous..self.text_cursor);
                    self.text_cursor = previous;
                    changed = true;
                }
                0x4c if self.text_cursor < value.len() => {
                    let next = self.text_cursor + value[self.text_cursor..].chars().next().map(char::len_utf8).unwrap_or(0);
                    value.drain(self.text_cursor..next);
                    changed = true;
                }
                0x50 if self.text_cursor > 0 => {
                    self.text_cursor = value[..self.text_cursor].char_indices().next_back().map(|(at, _)| at).unwrap_or(0);
                }
                0x4f if self.text_cursor < value.len() => {
                    self.text_cursor += value[self.text_cursor..].chars().next().map(char::len_utf8).unwrap_or(0);
                }
                0x4a => self.text_cursor = 0,
                0x4d => self.text_cursor = value.len(),
                usage => if let Some(character) = input_character(usage, shift) {
                    if value.len() < MAX_INPUT_BYTES {
                        let mut encoded = [0; 4];
                        value.insert_str(self.text_cursor, character.encode_utf8(&mut encoded));
                        self.text_cursor += character.len_utf8();
                        changed = true;
                    }
                },
            }
        }

        self.fill_rect(x, y, width, height, 0xFFFFFF)?;
        self.fill_rect(x, y, width, 1, if focused { 0x12377B } else { BUTTON_DARK })?;
        self.fill_rect(x, y, 1, height, if focused { 0x12377B } else { BUTTON_DARK })?;
        self.fill_rect(x + width as i32 - 1, y + 1, 1, height - 1, BUTTON_LIGHT)?;
        self.fill_rect(x + 1, y + height as i32 - 1, width - 2, 1, BUTTON_LIGHT)?;

        let visible_chars = ((width as usize).saturating_sub(16) / 8).max(1);
        let cursor_chars = value[..self.text_cursor].chars().count();
        let start_char = cursor_chars.saturating_sub(visible_chars.saturating_sub(1));
        if value.is_empty() {
            let text_y = y + (height as i32 - 16) / 2;
            self.text(x + 6, text_y, 0x808080, placeholder)?;
            if focused { self.fill_rect(x + 6, text_y, 1, 16, 0x12377B)?; }
        } else if focused {
            let before_count = cursor_chars.saturating_sub(start_char).min(visible_chars);
            let before: String = value.chars().skip(start_char).take(before_count).collect();
            let after_count = visible_chars.saturating_sub(before_count + 1);
            let after: String = value.chars().skip(cursor_chars).take(after_count).collect();
            let text_y = y + (height as i32 - 16) / 2;
            self.text(x + 6, text_y, BUTTON_TEXT, &before)?;
            self.text(x + 6 + (before_count as i32 + 1) * 8, text_y, BUTTON_TEXT, &after)?;
            self.fill_rect(x + 6 + before_count as i32 * 8, text_y, 1, 16, 0x12377B)?;
        } else {
            let visible: String = value.chars().skip(start_char).take(visible_chars).collect();
            self.text(x + 6, y + (height as i32 - 16) / 2, BUTTON_TEXT, &visible)?;
        }
        if let Some((_, cursor)) = self.text_cursors.iter_mut().find(|(widget, _)| *widget == index) {
            *cursor = self.text_cursor;
        } else {
            self.text_cursors.push((index, self.text_cursor));
        }
        Ok(changed)
    }

    /// Replace the current window contents with the queued commands.
    pub fn present(&mut self) -> Result<()> {
        let mut payload = Vec::with_capacity(self.frame.len() + 2);
        payload.extend_from_slice(&self.command_count.to_le_bytes());
        payload.extend_from_slice(&self.frame);
        send_packet(PACKET_FRAME, &payload)?;
        self.frame.clear();
        self.command_count = 0;
        self.pointer_clicks.clear();
        self.widget_count = self.next_widget;
        self.next_widget = 0;
        core::mem::swap(&mut self.dropdown_widgets, &mut self.next_dropdown_widgets);
        self.next_dropdown_widgets.clear();
        core::mem::swap(&mut self.text_widgets, &mut self.next_text_widgets);
        self.next_text_widgets.clear();
        if self.widget_count == 0 { self.focused_widget = 0; }
        else if self.focused_widget >= self.widget_count { self.focused_widget = self.widget_count - 1; }
        self.activate_widget = None;
        self.dropdown_step = 0;
        self.pending_text_keys.clear();
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
        let event = decode_event(&raw);
        if let Some(Event::Pointer { x, y, buttons }) = event {
            if buttons & 1 != 0 && self.pointer_buttons & 1 == 0 && self.pointer_clicks.len() < 32 {
                self.pointer_clicks.push((x, y));
            }
            self.pointer_buttons = buttons;
        }
        if let Some(Event::Key { usage, pressed: true, modifiers }) = event {
            match usage {
                0x2b if self.widget_count != 0 => {
                    let reverse = modifiers & 0x22 != 0;
                    self.focused_widget = if reverse {
                        (self.focused_widget + self.widget_count - 1) % self.widget_count
                    } else {
                        (self.focused_widget + 1) % self.widget_count
                    };
                }
                0x28 | 0x2c if !self.text_widgets.contains(&self.focused_widget) => {
                    self.activate_widget = Some(self.focused_widget);
                }
                usage if self.text_widgets.contains(&self.focused_widget) => {
                    if self.pending_text_keys.len() < 64 {
                        self.pending_text_keys.push((self.focused_widget, usage, modifiers));
                    }
                }
                0x4f | 0x51 if self.dropdown_widgets.contains(&self.focused_widget) => self.dropdown_step = 1,
                0x50 | 0x52 if self.dropdown_widgets.contains(&self.focused_widget) => self.dropdown_step = -1,
                _ => {}
            }
        }
        Ok(event)
    }

    /// Close the Era window and terminate the application loop.
    pub fn close(self) -> Result<()> { send_packet(PACKET_CLOSE, &[]) }

    fn take_click(&mut self, x: i32, y: i32, width: u32, height: u32) -> bool {
        self.take_click_at(|cx, cy| cx >= x && cx < x + width as i32 && cy >= y && cy < y + height as i32).is_some()
    }

    fn take_click_at(&mut self, predicate: impl Fn(i32, i32) -> bool) -> Option<(i32, i32)> {
        let index = self.pointer_clicks.iter().position(|&(x, y)| predicate(x, y))?;
        Some(self.pointer_clicks.remove(index))
    }

    fn draw_control(&mut self, x: i32, y: i32, width: u32, height: u32, label: &str) -> Result<()> {
        self.fill_rect(x, y, width, height, BUTTON_FACE)?;
        self.fill_rect(x, y, width, 1, BUTTON_LIGHT)?;
        self.fill_rect(x, y, 1, height, BUTTON_LIGHT)?;
        self.fill_rect(x + width as i32 - 1, y + 1, 1, height - 1, BUTTON_DARK)?;
        self.fill_rect(x + 1, y + height as i32 - 1, width - 2, 1, BUTTON_DARK)?;
        self.text(x + 5, y + (height as i32 - 16) / 2, BUTTON_TEXT, label)
    }

    fn register_widget(&mut self) -> usize {
        let index = self.next_widget;
        self.next_widget = self.next_widget.saturating_add(1);
        index
    }

    fn take_activation(&mut self, widget_index: usize) -> bool {
        if self.activate_widget == Some(widget_index) {
            self.activate_widget = None;
            true
        } else {
            false
        }
    }

    fn draw_focus(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<()> {
        self.fill_rect(x + 2, y + 2, width - 4, 1, 0x12377B)?;
        self.fill_rect(x + 2, y + height as i32 - 3, width - 4, 1, 0x12377B)?;
        self.fill_rect(x + 2, y + 2, 1, height - 4, 0x12377B)?;
        self.fill_rect(x + width as i32 - 3, y + 2, 1, height - 4, 0x12377B)
    }
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

fn input_character(usage: u8, shift: bool) -> Option<char> {
    let character = match usage {
        0x04..=0x1d => {
            let letter = (b'a' + usage - 0x04) as char;
            return Some(if shift { letter.to_ascii_uppercase() } else { letter });
        }
        0x1e..=0x27 => {
            let normal = b"1234567890"[(usage - 0x1e) as usize] as char;
            if shift { *b"!@#$%^&*()".get((usage - 0x1e) as usize)? as char } else { normal }
        }
        0x2c => ' ',
        0x2d => if shift { '_' } else { '-' },
        0x2e => if shift { '+' } else { '=' },
        0x2f => if shift { '{' } else { '[' },
        0x30 => if shift { '}' } else { ']' },
        0x31 => if shift { '|' } else { '\\' },
        0x33 => if shift { ':' } else { ';' },
        0x34 => if shift { '"' } else { '\'' },
        0x35 => if shift { '~' } else { '`' },
        0x36 => if shift { '<' } else { ',' },
        0x37 => if shift { '>' } else { '.' },
        0x38 => if shift { '?' } else { '/' },
        _ => return None,
    };
    Some(character)
}

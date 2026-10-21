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
const DIALOG_FACE: u32 = 0xD4D0C8;
const DIALOG_TITLE: u32 = 0x000080;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Resized { width: u32, height: u32 },
    Key { usage: u8, pressed: bool, modifiers: u8 },
    Pointer { x: i32, y: i32, buttons: u8 },
    Close,
}

pub struct Window {
    title: String,
    width: u32,
    height: u32,
    frame: Vec<u8>,
    command_count: u16,
    last_frame: Vec<u8>,
    last_command_count: u16,
    toast: Option<(String, u64)>,
    input: Vec<u8>,
    pointer_clicks: Vec<(i32, i32)>,
    pointer_buttons: u8,
    open_dropdown: Option<(i32, i32)>,
    focused_widget: usize,
    keyboard_widget_activation: bool,
    dropdown_widgets: Vec<usize>,
    next_dropdown_widgets: Vec<usize>,
    text_widgets: Vec<usize>,
    next_text_widgets: Vec<usize>,
    text_input_bounds: Vec<(usize, i32, i32, u32, u32)>,
    next_text_input_bounds: Vec<(usize, i32, i32, u32, u32)>,
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

    pub fn open(title: &str, width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 || width > 1920 || height > 1080 {
            return Err(Error(22));
        }
        Self::open_inner(title, width, height)
    }

    pub fn open_auto(title: &str) -> Result<Self> {
        Self::open_inner(title, 0, 0)
    }

    fn open_inner(title: &str, width: u32, height: u32) -> Result<Self> {
        if title.is_empty() || title.len() > 80 {
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
            last_frame: Vec::new(),
            last_command_count: 0,
            toast: None,
            input: Vec::new(),
            pointer_clicks: Vec::new(),
            pointer_buttons: 0,
            open_dropdown: None,
            focused_widget: 0,
            keyboard_widget_activation: false,
            dropdown_widgets: Vec::new(),
            next_dropdown_widgets: Vec::new(),
            text_widgets: Vec::new(),
            next_text_widgets: Vec::new(),
            text_input_bounds: Vec::new(),
            next_text_input_bounds: Vec::new(),
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

    pub fn set_keyboard_widget_activation(&mut self, enabled: bool) {
        self.keyboard_widget_activation = enabled;
    }

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

    pub fn radio_button(&mut self, x: i32, y: i32, label: &str, selected: &mut bool) -> Result<bool> {
        if label.len() > MAX_TEXT_BYTES { return Err(Error(7)); }
        let index = self.register_widget();
        let width = 24u32.saturating_add((label.len() as u32).saturating_mul(8));
        let pointer_clicked = self.take_click(x, y, width, 22);
        let focused = if pointer_clicked {
            self.focused_widget = index;
            true
        } else {
            index == self.focused_widget
        };
        let activated = pointer_clicked || (focused && self.take_activation(index));
        let changed = activated && !*selected;
        if activated { *selected = true; }
        self.fill_rect(x + 1, y + 2, 16, 16, 0xFFFFFF)?;
        self.fill_rect(x + 4, y + 1, 10, 1, BUTTON_DARK)?;
        self.fill_rect(x + 2, y + 3, 1, 14, BUTTON_DARK)?;
        self.fill_rect(x + 15, y + 3, 1, 14, BUTTON_LIGHT)?;
        self.fill_rect(x + 4, y + 18, 10, 1, BUTTON_LIGHT)?;
        if *selected {
            self.fill_rect(x + 6, y + 6, 6, 6, 0x12377B)?;
        }
        self.text(x + 24, y + 2, BUTTON_TEXT, label)?;
        if focused { self.draw_focus(x, y, width.max(24), 22)?; }
        Ok(changed)
    }

    pub fn slider(&mut self, x: i32, y: i32, width: u32, height: u32, min: i32, max: i32, value: &mut i32) -> Result<bool> {
        if width < 64 || height < 24 || min >= max { return Err(Error(22)); }
        *value = (*value).clamp(min, max);
        let index = self.register_widget();
        let click = self.take_click_at(|cx, cy| {
            cx >= x && cx < x + width as i32 && cy >= y && cy < y + height as i32
        });
        let focused = if let Some((cx, _)) = click {
            self.focused_widget = index;
            let track_x = x + 8;
            let track_width = (width as i64 - 16).max(1);
            let offset = (cx as i64 - track_x as i64).clamp(0, track_width);
            let range = max as i64 - min as i64;
            *value = (min as i64 + (offset * range + track_width / 2) / track_width) as i32;
            true
        } else {
            index == self.focused_widget
        };
        self.fill_rect(x + 4, y + 7, width - 8, 10, BUTTON_FACE)?;
        self.fill_rect(x + 4, y + 7, width - 8, 1, BUTTON_DARK)?;
        self.fill_rect(x + 4, y + 8, 1, 8, BUTTON_DARK)?;
        self.fill_rect(x + width as i32 - 5, y + 8, 1, 8, BUTTON_LIGHT)?;
        let track_width = (width - 16) as i64;
        let offset = ((*value as i64 - min as i64) * track_width / (max as i64 - min as i64)) as i32;
        let thumb_x = x + 8 + offset;
        self.fill_rect(thumb_x - 4, y + 2, 9, 20, BUTTON_FACE)?;
        self.fill_rect(thumb_x - 4, y + 2, 9, 1, BUTTON_LIGHT)?;
        self.fill_rect(thumb_x - 4, y + 2, 1, 20, BUTTON_LIGHT)?;
        self.fill_rect(thumb_x + 4, y + 3, 1, 19, BUTTON_DARK)?;
        self.fill_rect(thumb_x - 3, y + 21, 7, 1, BUTTON_DARK)?;
        if focused { self.draw_focus(x, y, width, height)?; }
        Ok(click.is_some())
    }

    pub fn progress_bar(&mut self, x: i32, y: i32, width: u32, height: u32, value: u32, max: u32) -> Result<()> {
        if width < 8 || height < 8 || max == 0 { return Err(Error(22)); }
        let bounded = value.min(max);
        self.fill_rect(x, y, width, height, 0xFFFFFF)?;
        self.fill_rect(x, y, width, 1, BUTTON_DARK)?;
        self.fill_rect(x, y, 1, height, BUTTON_DARK)?;
        self.fill_rect(x + width as i32 - 1, y + 1, 1, height - 1, BUTTON_LIGHT)?;
        self.fill_rect(x + 1, y + height as i32 - 1, width - 2, 1, BUTTON_LIGHT)?;
        let inner_width = width - 4;
        let filled = ((inner_width as u64 * bounded as u64) / max as u64) as u32;
        if filled != 0 {
            self.fill_rect(x + 2, y + 2, filled, height - 4, 0x12377B)?;
        }
        Ok(())
    }

    pub fn confirm_dialog(&mut self, title: &str, message: &str) -> Result<bool> {
        self.run_dialog(title, message, true)
    }

    pub fn message_dialog(&mut self, title: &str, message: &str) -> Result<()> {
        self.run_dialog(title, message, false).map(|_| ())
    }

    pub fn error_dialog(&mut self, message: &str) -> Result<()> {
        self.message_dialog("Error", message)
    }

    pub fn text_input_dialog(&mut self, title: &str, prompt: &str, initial: &str) -> Result<Option<String>> {
        self.run_input_dialog(title, prompt, initial)
    }

    pub fn number_input_dialog(&mut self, title: &str, prompt: &str, initial: i64) -> Result<Option<i64>> {
        let mut value = alloc::format!("{}", initial);
        loop {
            let Some(input) = self.run_input_dialog(title, prompt, &value)? else { return Ok(None); };
            match input.trim().parse::<i64>() {
                Ok(number) => return Ok(Some(number)),
                Err(_) => {
                    value = input;
                    self.error_dialog("Enter a valid whole number.")?;
                }
            }
        }
    }

    pub fn save_file_dialog(&mut self, directory: &str, suggested_name: &str) -> Result<Option<String>> {
        if self.width < 320 || self.height < 250 || suggested_name.len() > MAX_INPUT_BYTES {
            return Err(Error(22));
        }
        let Some(mut directory) = normalize_disk_directory(directory) else { return Err(Error(22)); };
        let mut filename = String::from(suggested_name);
        let background = self.last_frame.clone();
        let background_commands = self.last_command_count;
        let mut status = String::new();
        let panel_height = self.height.saturating_sub(16).min(420) as usize;
        let widget_rows = (panel_height.saturating_sub(218) / 24).clamp(1, 6);
        let mut selected_path = None;
        let mut cancelled = false;
        let mut cached_listing: Option<(String, Option<String>)> = None;

        self.widget_count = 2 + 1 + widget_rows + 2;
        self.focused_widget = 2;
        self.next_widget = 0;
        self.activate_widget = None;
        self.text_widgets.clear();
        self.next_text_widgets.clear();
        self.text_input_bounds.clear();
        self.next_text_input_bounds.clear();
        self.dropdown_widgets.clear();
        self.next_dropdown_widgets.clear();
        self.pending_text_keys.clear();
        self.cursor_widget = None;
        self.text_cursors.clear();
        self.text_cursor = 0;
        let outcome = (|| -> Result<()> {
        loop {
            let mut enter = false;
            while let Some(event) = self.poll_event()? {
                match event {
                    Event::Close | Event::Key { usage: 0x29, pressed: true, .. } => { cancelled = true; break; }
                    Event::Key { usage: 0x28, pressed: true, .. } => {
                        enter = true;
                        self.activate_widget = None;
                    }
                    _ => {}
                }
            }
            if cancelled { break; }
            let normalized_directory = normalize_disk_directory(&directory);
            if let Some(path) = normalized_directory.as_ref() {
                let refresh = match cached_listing.as_ref() {
                    Some((cached_path, _)) => cached_path != path,
                    None => true,
                };
                if refresh {
                    cached_listing = Some((path.clone(), crate::system::list_directory(path).ok()));
                }
            }
            let listing = cached_listing.as_ref().and_then(|(path, entries)| {
                if normalized_directory.as_ref() == Some(path) { entries.as_deref() } else { None }
            });
            let folders: Vec<String> = listing.as_deref().unwrap_or("").lines()
                .filter(|entry| entry.ends_with('/') && entry.len() > 1)
                .take(widget_rows)
                .map(String::from)
                .collect();

            self.frame.clear();
            self.frame.extend_from_slice(&background);
            self.command_count = background_commands;
            let (panel_x, panel_y, panel_w, panel_h) = self.draw_dialog_panel("Save file", "", (self.height as i32 - 16).min(420))?;
            self.text(panel_x + 14, panel_y + 42, 0x202020, "Folder:")?;
            let path_changed = self.text_input(panel_x + 14, panel_y + 58, panel_w - 88, 26, &mut directory, "Path under /disk")?;
            let up = self.button(panel_x + panel_w as i32 - 64, panel_y + 58, 50, 26, "Up")?;
            if path_changed {
                if normalize_disk_directory(&directory).is_some() { status.clear(); }
                else { status = String::from("Folder must stay inside /disk."); }
            }
            if up {
                if let Some(normalized) = normalize_disk_directory(&directory) {
                    directory = parent_disk_directory(&normalized);
                    status.clear();
                } else { status = String::from("Folder must stay inside /disk."); }
            }
            self.text(panel_x + 14, panel_y + 91, 0x202020, "Filename:")?;
            self.text_input(panel_x + 14, panel_y + 107, panel_w - 28, 26, &mut filename, "filename")?;
            self.text(panel_x + 14, panel_y + 137, 0x202020, "Folders:")?;

            let rows_y = panel_y + 156;
            let mut entered_directory = None;
            for row in 0..widget_rows {
                let label = folders.get(row)
                    .map(|entry| truncate_chars(entry.trim_end_matches('/'), (panel_w as usize).saturating_sub(42) / 8))
                    .unwrap_or_default();
                let clicked = self.button(panel_x + 14, rows_y + row as i32 * 24, panel_w - 28, 22, &label)?;
                if clicked {
                    if let Some(folder) = folders.get(row) {
                        if let Some(normalized) = normalize_disk_directory(&directory) {
                            entered_directory = Some(join_directory(&normalized, folder.trim_end_matches('/')));
                        }
                    }
                }
            }
            if let Some(next) = entered_directory {
                if next.len() <= 240 { directory = next; status.clear(); }
                else { status = String::from("Folder path is too long."); }
            }

            if !status.is_empty() {
                let status_y = if panel_h < 300 { panel_y + 137 } else { panel_y + panel_h - 62 };
                self.text(panel_x + 14, status_y, 0xA00000, &truncate_chars(&status, 52))?;
            } else if listing.is_none() {
                self.text(panel_x + 14, panel_y + panel_h - 62, 0xA00000, "Cannot read this folder.")?;
            }
            let button_y = panel_y + panel_h - 40;
            let save = self.button(panel_x + panel_w as i32 - 178, button_y, 76, 28, "Save")?;
            let cancel = self.button(panel_x + panel_w as i32 - 92, button_y, 76, 28, "Cancel")?;
            self.present()?;
            if cancel { cancelled = true; break; }
            if save || enter {
                if filename.is_empty() || filename.contains('/') || filename == "." || filename == ".." || filename.contains('\0') {
                    status = String::from("Enter a valid filename.");
                    crate::system::sleep_ms(16)?;
                    continue;
                }
                let Some(normalized) = normalize_disk_directory(&directory) else {
                    status = String::from("Folder must stay inside /disk.");
                    crate::system::sleep_ms(16)?;
                    continue;
                };
                let path = join_directory(&normalized, &filename);
                if path.len() > 255 {
                    status = String::from("File path is too long.");
                    crate::system::sleep_ms(16)?;
                    continue;
                }
                let entries = match crate::system::list_directory(&normalized) {
                    Ok(entries) => entries,
                    Err(_) => {
                        status = String::from("Cannot read this folder.");
                        crate::system::sleep_ms(16)?;
                        continue;
                    }
                };
                if entries.lines().any(|entry| entry.strip_suffix('/').is_some_and(|name| name == filename)) {
                    status = String::from("A folder already uses that name.");
                    crate::system::sleep_ms(16)?;
                    continue;
                }
                let exists = entries.lines().any(|entry| entry == filename);
                selected_path = Some((path, exists));
                break;
            }
            crate::system::sleep_ms(16)?;
        }
        Ok(())
        })();

        self.frame.clear();
        self.frame.extend_from_slice(&background);
        self.command_count = background_commands;
        self.next_widget = 0;
        self.text_widgets.clear();
        self.next_text_widgets.clear();
        self.text_input_bounds.clear();
        self.next_text_input_bounds.clear();
        self.dropdown_widgets.clear();
        self.next_dropdown_widgets.clear();
        self.pending_text_keys.clear();
        self.cursor_widget = None;
        self.text_cursors.clear();
        self.text_cursor = 0;
        let restore = self.present();
        outcome?;
        restore?;
        if cancelled { return Ok(None); }
        if let Some((path, true)) = selected_path.as_ref() {
            if !self.confirm_dialog("Replace file?", &alloc::format!("Replace {}?", path))? {
                return Ok(None);
            }
        }
        Ok(selected_path.map(|(path, _)| path))
    }

    pub fn toast(&mut self, message: &str) -> Result<()> {
        self.toast_for(message, 2500)
    }

    pub fn toast_for(&mut self, message: &str, duration_ms: u64) -> Result<()> {
        if message.is_empty() || message.len() > 256 { return Err(Error(22)); }
        let deadline = crate::system::uptime_ms().saturating_add(duration_ms.clamp(250, 15_000));
        self.toast = Some((String::from(message), deadline));
        Ok(())
    }

    pub fn begin_progress<'a>(&'a mut self, title: &str, message: &str) -> Result<ProgressDialog<'a>> {
        if title.is_empty() || title.len() > 80 || message.len() > 256 {
            return Err(Error(22));
        }
        let background = self.last_frame.clone();
        let background_commands = self.last_command_count;
        let mut progress = ProgressDialog {
            window: self,
            title: String::from(title),
            background,
            background_commands,
            cancelled: false,
            finished: false,
        };
        progress.update(0, message)?;
        Ok(progress)
    }

    fn run_dialog(&mut self, title: &str, message: &str, confirm: bool) -> Result<bool> {
        if title.is_empty() || title.len() > 80 || message.len() > 2048 {
            return Err(Error(22));
        }

        let background = self.last_frame.clone();
        let background_commands = self.last_command_count;
        self.widget_count = if confirm { 2 } else { 1 };
        self.next_widget = 0;
        self.focused_widget = if confirm { 1 } else { 0 };
        self.activate_widget = None;
        self.dropdown_step = 0;
        self.open_dropdown = None;
        self.pending_text_keys.clear();
        self.text_widgets.clear();
        self.next_text_widgets.clear();
        self.text_input_bounds.clear();
        self.next_text_input_bounds.clear();
        self.dropdown_widgets.clear();
        self.next_dropdown_widgets.clear();

        let outcome = (|| -> Result<bool> {
            loop {
                let mut dismissed = false;
                while let Some(event) = self.poll_event()? {
                    match event {
                        Event::Close => { dismissed = true; break; }
                        Event::Key { usage: 0x29, pressed: true, .. } => {
                            dismissed = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if dismissed { return Ok(false); }

                self.frame.clear();
                self.frame.extend_from_slice(&background);
                self.command_count = background_commands;
                self.draw_dialog(title, message, confirm)?;
                let width = self.width.min(420) as i32;
                let left = (self.width as i32 - width) / 2;
                let button_y = self.dialog_button_y(confirm);
                let accepted = if confirm {
                    let yes = self.button(left + width - 184, button_y, 76, 28, "Yes")?;
                    let no = self.button(left + width - 96, button_y, 76, 28, "No")?;
                    if yes { Some(true) } else if no { Some(false) } else { None }
                } else {
                    let ok = self.button(left + (width - 76) / 2, button_y, 76, 28, "OK")?;
                    ok.then_some(true)
                };
                self.present()?;
                if let Some(accepted) = accepted { return Ok(accepted); }
                crate::system::sleep_ms(16)?;
            }
        })();

        self.frame.clear();
        self.frame.extend_from_slice(&background);
        self.command_count = background_commands;
        self.next_widget = 0;
        self.text_widgets.clear();
        self.next_text_widgets.clear();
        self.text_input_bounds.clear();
        self.next_text_input_bounds.clear();
        self.dropdown_widgets.clear();
        self.next_dropdown_widgets.clear();
        let restore = self.present();
        match outcome {
            Err(error) => Err(error),
            Ok(value) => restore.map(|()| value),
        }
    }

    fn dialog_button_y(&self, confirm: bool) -> i32 {
        let height = if confirm { 176 } else { 156 };
        ((self.height as i32 - height) / 2 + height - 42).max(8)
    }

    fn draw_dialog(&mut self, title: &str, message: &str, confirm: bool) -> Result<()> {
        let height = if confirm { 176 } else { 156 };
        self.draw_dialog_panel(title, message, height)?;
        Ok(())
    }

    fn draw_dialog_panel(&mut self, title: &str, message: &str, requested_height: i32) -> Result<(i32, i32, u32, i32)> {
        let width = self.width.saturating_sub(16).min(540).max(208);
        let height = requested_height.min(self.height.saturating_sub(16) as i32).max(132);
        let x = (self.width as i32 - width as i32) / 2;
        let y = (self.height as i32 - height).max(0) / 2;
        self.fill_rect(x + 4, y + 4, width, height as u32, 0x606060)?;
        self.fill_rect(x, y, width, height as u32, DIALOG_FACE)?;
        self.fill_rect(x, y, width, 2, 0xFFFFFF)?;
        self.fill_rect(x, y, 2, height as u32, 0xFFFFFF)?;
        self.fill_rect(x + width as i32 - 2, y + 2, 2, (height - 2) as u32, 0x404040)?;
        self.fill_rect(x + 2, y + height - 2, width - 4, 2, 0x404040)?;
        self.fill_rect(x + 8, y + 8, width - 16, 22, DIALOG_TITLE)?;
        self.text(x + 14, y + 11, 0xFFFFFF, title)?;
        if !message.is_empty() {
            let max_chars = ((width as usize).saturating_sub(36) / 8).max(1);
            let mut line_y = y + 48;
            let mut displayed = 0usize;
            for source_line in message.lines().take(4) {
                let chars: Vec<char> = source_line.chars().collect();
                for chunk in chars.chunks(max_chars).take(4) {
                    if displayed >= 4 { break; }
                    let line: String = chunk.iter().collect();
                    self.text(x + 18, line_y, 0x101010, &line)?;
                    line_y += 18;
                    displayed += 1;
                    if line_y >= y + height - 52 { break; }
                }
                if displayed >= 4 || line_y >= y + height - 52 { break; }
            }
        }
        Ok((x, y, width, height))
    }

    fn run_input_dialog(&mut self, title: &str, prompt: &str, initial: &str) -> Result<Option<String>> {
        if self.width < 300 || self.height < 250 || title.is_empty() || title.len() > 80 || prompt.len() > 512 || initial.len() > MAX_INPUT_BYTES {
            return Err(Error(22));
        }
        let background = self.last_frame.clone();
        let background_commands = self.last_command_count;
        let mut value = String::from(initial);
        self.widget_count = 3;
        self.next_widget = 0;
        self.focused_widget = 0;
        self.activate_widget = None;
        self.text_widgets.clear();
        self.next_text_widgets.clear();
        self.text_input_bounds.clear();
        self.next_text_input_bounds.clear();
        self.dropdown_widgets.clear();
        self.next_dropdown_widgets.clear();
        self.pending_text_keys.clear();
        self.cursor_widget = None;
        self.text_cursors.clear();
        self.text_cursor = 0;

        let outcome = (|| -> Result<Option<String>> {
            loop {
                let mut enter = false;
                let mut cancel = false;
                while let Some(event) = self.poll_event()? {
                    match event {
                        Event::Close | Event::Key { usage: 0x29, pressed: true, .. } => { cancel = true; break; }
                        Event::Key { usage: 0x28, pressed: true, .. } => enter = true,
                        _ => {}
                    }
                }
                if cancel { return Ok(None); }

                self.frame.clear();
                self.frame.extend_from_slice(&background);
                self.command_count = background_commands;
                let (x, y, width, height) = self.draw_dialog_panel(title, "", 220)?;
                let prompt_width = ((width as usize).saturating_sub(36) / 8).max(1);
                for (line, chunk) in prompt.chars().collect::<Vec<_>>().chunks(prompt_width).take(2).enumerate() {
                    let text: String = chunk.iter().collect();
                    self.text(x + 18, y + 48 + line as i32 * 18, 0x202020, &text)?;
                }
                self.text_input(x + 18, y + 88, width - 36, 30, &mut value, "Enter a value")?;
                let button_y = y + height - 42;
                let accept_x = x + width as i32 - 184;
                let accepted = self.button(accept_x, button_y, 76, 28, "OK")?;
                let cancelled = self.button(accept_x + 88, button_y, 76, 28, "Cancel")?;
                self.present()?;
                if accepted || enter { return Ok(Some(value)); }
                if cancelled { return Ok(None); }
                crate::system::sleep_ms(16)?;
            }
        })();

        self.frame.clear();
        self.frame.extend_from_slice(&background);
        self.command_count = background_commands;
        self.next_widget = 0;
        self.text_widgets.clear();
        self.next_text_widgets.clear();
        self.text_input_bounds.clear();
        self.next_text_input_bounds.clear();
        self.dropdown_widgets.clear();
        self.next_dropdown_widgets.clear();
        self.cursor_widget = None;
        self.text_cursors.clear();
        self.text_cursor = 0;
        let restore = self.present();
        match outcome {
            Err(error) => Err(error),
            Ok(value) => restore.map(|()| value),
        }
    }

    fn draw_active_toast(&mut self) -> Result<()> {
        let now = crate::system::uptime_ms();
        let Some((message, deadline)) = self.toast.as_ref() else { return Ok(()); };
        if now >= *deadline {
            self.toast = None;
            return Ok(());
        }
        let message = message.clone();
        let width = self.width.saturating_sub(24).min(420).max(1);
        let x = (self.width.saturating_sub(width + 24) / 2) as i32;
        let y = 14;
        self.fill_rect(x + 3, y + 3, width, 34, 0x606060)?;
        self.fill_rect(x, y, width, 34, 0xFFFFE1)?;
        self.fill_rect(x, y, width, 1, 0xFFFFFF)?;
        self.fill_rect(x, y, 1, 34, 0xFFFFFF)?;
        self.fill_rect(x + width as i32 - 1, y + 1, 1, 33, 0x404040)?;
        self.fill_rect(x + 1, y + 33, width - 1, 1, 0x404040)?;
        let max_chars = ((width as usize).saturating_sub(16) / 8).max(1);
        let text: String = message.chars().take(max_chars).collect();
        self.text(x + 8, y + 9, 0x101010, &text)
    }

    pub fn text_input(&mut self, x: i32, y: i32, width: u32, height: u32, value: &mut String, placeholder: &str) -> Result<bool> {
        if width < 40 || height < 24 || value.len() > MAX_INPUT_BYTES { return Err(Error(22)); }
        let index = self.register_widget();
        if !self.next_text_widgets.contains(&index) { self.next_text_widgets.push(index); }
        if !self.next_text_input_bounds.iter().any(|(widget, ..)| *widget == index) {
            self.next_text_input_bounds.push((index, x, y, width, height));
        }
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
        if let Some((click_x, _)) = self.take_click_at(|cx, cy| cx >= x && cx < x + width as i32 && cy >= y && cy < y + height as i32) {
            self.focused_widget = index;
            focused = true;
            let visible_chars = ((width as usize).saturating_sub(16) / 8).max(1);
            let old_column = value[..self.text_cursor].chars().count();
            let start_char = old_column.saturating_sub(visible_chars.saturating_sub(1));
            let clicked_column = ((click_x - x - 6).max(0) / 8) as usize;
            let target_char = (start_char + clicked_column).min(value.chars().count());
            self.text_cursor = value.char_indices().nth(target_char).map(|(at, _)| at).unwrap_or(value.len());
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

    pub fn present(&mut self) -> Result<()> {
        self.draw_active_toast()?;
        if self.frame != self.last_frame || self.command_count != self.last_command_count {
            let mut payload = Vec::with_capacity(self.frame.len() + 2);
            payload.extend_from_slice(&self.command_count.to_le_bytes());
            payload.extend_from_slice(&self.frame);
            send_packet(PACKET_FRAME, &payload)?;
            self.last_frame.clone_from(&self.frame);
            self.last_command_count = self.command_count;
        }
        self.frame.clear();
        self.command_count = 0;
        self.pointer_clicks.clear();
        self.widget_count = self.next_widget;
        self.next_widget = 0;
        core::mem::swap(&mut self.dropdown_widgets, &mut self.next_dropdown_widgets);
        self.next_dropdown_widgets.clear();
        core::mem::swap(&mut self.text_widgets, &mut self.next_text_widgets);
        self.next_text_widgets.clear();
        core::mem::swap(&mut self.text_input_bounds, &mut self.next_text_input_bounds);
        self.next_text_input_bounds.clear();
        if self.widget_count == 0 { self.focused_widget = 0; }
        else if self.focused_widget >= self.widget_count { self.focused_widget = self.widget_count - 1; }
        self.activate_widget = None;
        self.dropdown_step = 0;
        self.pending_text_keys.clear();
        Ok(())
    }

    pub fn poll_event(&mut self) -> Result<Option<Event>> {
        let mut chunk = [0u8; 256];
        let amount = eonos_rt::pipe_read_available(0, &mut chunk)?;
        self.input.extend_from_slice(&chunk[..amount]);
        if self.input.len() < EVENT_SIZE { return Ok(None); }
        let mut raw = [0u8; EVENT_SIZE];
        raw.copy_from_slice(&self.input[..EVENT_SIZE]);
        self.input.drain(..EVENT_SIZE);
        let event = decode_event(&raw);
        if let Some(Event::Resized { width, height }) = event {
            self.width = width;
            self.height = height;
        }
        if let Some(Event::Pointer { x, y, buttons }) = event {
            if buttons & 1 != 0 && self.pointer_buttons & 1 == 0 && self.pointer_clicks.len() < 32 {
                self.pointer_clicks.push((x, y));
                if let Some((widget, ..)) = self.text_input_bounds.iter()
                    .find(|(_, left, top, width, height)| x >= *left && x < *left + *width as i32 && y >= *top && y < *top + *height as i32) {
                    self.focused_widget = *widget;
                }
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
                0x28 | 0x2c if self.keyboard_widget_activation && !self.text_widgets.contains(&self.focused_widget) => {
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

pub struct ProgressDialog<'a> {
    window: &'a mut Window,
    title: String,
    background: Vec<u8>,
    background_commands: u16,
    cancelled: bool,
    finished: bool,
}

impl ProgressDialog<'_> {

    pub fn update(&mut self, percent: u8, message: &str) -> Result<bool> {
        if self.cancelled { return Ok(true); }
        if message.len() > 256 { return Err(Error(22)); }
        self.window.widget_count = 1;
        self.window.focused_widget = 0;
        while let Some(event) = self.window.poll_event()? {
            if matches!(event, Event::Close | Event::Key { usage: 0x29, pressed: true, .. }) {
                self.cancelled = true;
            }
        }
        if self.cancelled { return Ok(true); }

        self.window.frame.clear();
        self.window.frame.extend_from_slice(&self.background);
        self.window.command_count = self.background_commands;
        self.window.next_widget = 0;
        let (x, y, width, height) = self.window.draw_dialog_panel(&self.title, "", 176)?;
        self.window.text(x + 18, y + 48, 0x202020, &truncate_chars(message, 54))?;
        let bar_x = x + 18;
        let bar_y = y + 78;
        let bar_width = width - 36;
        self.window.fill_rect(bar_x, bar_y, bar_width, 22, 0xFFFFFF)?;
        self.window.fill_rect(bar_x, bar_y, bar_width, 1, 0x404040)?;
        self.window.fill_rect(bar_x, bar_y, 1, 22, 0x404040)?;
        let filled = ((bar_width.saturating_sub(4) as u64) * percent.min(100) as u64 / 100) as u32;
        if filled > 0 { self.window.fill_rect(bar_x + 2, bar_y + 2, filled, 18, 0x12377B)?; }
        let button_y = y + height - 42;
        let cancelled = self.window.button(x + width as i32 - 94, button_y, 76, 28, "Cancel")?;
        self.window.present()?;
        if cancelled { self.cancelled = true; }
        Ok(self.cancelled)
    }

    pub fn finish(mut self) -> Result<()> {
        let result = self.restore();
        self.finished = true;
        result
    }

    fn restore(&mut self) -> Result<()> {
        self.window.frame.clear();
        self.window.frame.extend_from_slice(&self.background);
        self.window.command_count = self.background_commands;
        self.window.next_widget = 0;
        self.window.text_widgets.clear();
        self.window.next_text_widgets.clear();
        self.window.text_input_bounds.clear();
        self.window.next_text_input_bounds.clear();
        self.window.dropdown_widgets.clear();
        self.window.next_dropdown_widgets.clear();
        self.window.present()
    }
}

impl Drop for ProgressDialog<'_> {
    fn drop(&mut self) {
        if !self.finished { let _ = self.restore(); }
    }
}

fn normalize_disk_directory(path: &str) -> Option<String> {
    if path.is_empty() || path.len() > 240 || !path.starts_with('/') || path.contains('\0') { return None; }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.len() > 1 => { parts.pop(); }
            ".." => return None,
            value => parts.push(value),
        }
    }
    if parts.first().copied() != Some("disk") { return None; }
    let mut normalized = String::from("/");
    for (index, part) in parts.iter().enumerate() {
        if index != 0 { normalized.push('/'); }
        normalized.push_str(part);
    }
    Some(normalized)
}

fn parent_disk_directory(path: &str) -> String {
    if path == "/disk" { return String::from("/disk"); }
    match path.rfind('/') {
        Some(index) if index >= 5 => String::from(&path[..index]),
        _ => String::from("/disk"),
    }
}

fn join_directory(directory: &str, name: &str) -> String {
    if directory == "/" { alloc::format!("/{}", name) }
    else { alloc::format!("{}/{}", directory.trim_end_matches('/'), name) }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
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

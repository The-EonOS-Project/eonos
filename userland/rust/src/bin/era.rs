#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::sync::atomic::{AtomicBool, AtomicI8, AtomicU8, Ordering};

const PANEL: u32 = 0xC0C0C0;
const PANEL_RAISED: u32 = 0xC0C0C0;
const PANEL_EDGE: u32 = 0x808080;
const TEXT: u32 = 0x101010;
const MUTED: u32 = 0x404040;
const ACCENT: u32 = 0x12377B;
const AQUA: u32 = 0x12377B;
const GOLD: u32 = 0x805000;
const SETTINGS_PATH: &str = "/disk/era-settings";
static WALLPAPER_THEME: AtomicU8 = AtomicU8::new(0);
static RESTORE_SESSION: AtomicBool = AtomicBool::new(true);
static DISK_MOUNTED: AtomicBool = AtomicBool::new(false);
static SETTINGS_SAVE_STATUS: AtomicU8 = AtomicU8::new(0);
static APP_LAUNCH_STATUS: AtomicU8 = AtomicU8::new(0);
static SYSTEM_TAB: AtomicU8 = AtomicU8::new(0);
static SETTINGS_TAB: AtomicU8 = AtomicU8::new(0);
static TIMEZONE_OFFSET: AtomicI8 = AtomicI8::new(0);

struct RegisteredApp { name: &'static str, description: &'static str, executable: &'static str }
const APPLICATIONS: [RegisteredApp; 1] = [RegisteredApp {
    name: "DOOM",
    description: "Freedoom Phase 1 · keyboard play",
    executable: "/bin/doom",
}];

#[derive(Clone)]
struct InstalledApp { id: String, name: String, description: String, kind: String, executable: String }

#[derive(Clone)]
enum AppDrawOp {
    Rect(i32, i32, u32, u32, u32),
    Text(i32, i32, u32, String),
}

struct AppSession {
    title: String,
    width: i32,
    height: i32,
    process: eonos_user::Process,
    output_fd: usize,
    input_fd: usize,
    pending_events: Vec<u8>,
    incoming: Vec<u8>,
    drawing: Vec<AppDrawOp>,
    closed: bool,
    resources_closed: bool,
    reaped: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum View {
    Welcome,
    Files,
    TextViewer,
    System,
    Apps,
    Konsul,
    About,
    Settings,
    AppWindow(usize),
}

fn refresh_disk_status() -> bool {
    let mounted = eonos_user::is_mountpoint("/disk").unwrap_or(false);
    DISK_MOUNTED.store(mounted, Ordering::Relaxed);
    mounted
}

fn load_settings() {
    if !refresh_disk_status() { return; }
    let Ok(mut file) = eonos_user::File::open(SETTINGS_PATH, eonos_user::flags::READ_ONLY) else { return; };
    let Ok(contents) = file.read_to_string() else { return; };
    let mut lines = contents.lines();
    let version = lines.next();
    if version != Some("ERA-SETTINGS-1") && version != Some("ERA-SETTINGS-2") { return; }
    if let Some(theme) = lines.next().and_then(|value| value.parse::<u8>().ok()).filter(|value| *value < 3) {
        WALLPAPER_THEME.store(theme, Ordering::Relaxed);
    }
    if let Some(restore) = lines.next() {
        RESTORE_SESSION.store(restore == "1", Ordering::Relaxed);
    }
    if version == Some("ERA-SETTINGS-2") {
        if let Some(offset) = lines.next().and_then(|value| value.parse::<i8>().ok()).filter(|value| (-12..=14).contains(value)) {
            TIMEZONE_OFFSET.store(offset, Ordering::Relaxed);
        }
    }
}

fn save_settings() {
    if !refresh_disk_status() {
        SETTINGS_SAVE_STATUS.store(2, Ordering::Relaxed);
        return;
    }
    let result = (|| {
        let mut file = eonos_user::File::open(
            SETTINGS_PATH,
            eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
        )?;
        let data = format!("ERA-SETTINGS-2\n{}\n{}\n{}\n", WALLPAPER_THEME.load(Ordering::Relaxed), RESTORE_SESSION.load(Ordering::Relaxed) as u8, TIMEZONE_OFFSET.load(Ordering::Relaxed));
        file.write_all(data.as_bytes())?;
        file.close()?;
        eonos_rt::sync()?;
        Ok::<(), eonos_user::Error>(())
    })();
    SETTINGS_SAVE_STATUS.store(if result.is_ok() { 1 } else { 3 }, Ordering::Relaxed);
}

fn wallpaper_name() -> &'static str {
    match WALLPAPER_THEME.load(Ordering::Relaxed) {
        1 => "Violet",
        2 => "Forest",
        _ => "Midnight",
    }
}

#[derive(Clone)]
struct Layout {
    width: i32,
    height: i32,
    window_x: i32,
    window_y: i32,
    window_w: i32,
    window_h: i32,
    taskbar_y: i32,
    windows: Vec<View>,
    placements: Vec<WindowPlacement>,
}

#[derive(Clone, Copy)]
struct WindowPlacement {
    view: View,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

struct FileState {
    directory: String,
    listing: String,
    offset: usize,
    opened_path: String,
    contents: String,
    truncated: bool,
}

const SESSION_PATH: &str = "/disk/era-session";

fn view_name(view: Option<View>) -> &'static str {
    match view {
        None => "closed",
        Some(View::Welcome) => "welcome",
        Some(View::Files) => "files",
        Some(View::TextViewer) => "text",
        Some(View::System) => "system",
        Some(View::Apps) => "apps",
        Some(View::Konsul) => "konsul",
        Some(View::About) => "about",
        Some(View::Settings) => "settings",
        Some(View::AppWindow(_)) => "apps",
    }
}

fn restore_session(files: &mut FileState) -> Result<Option<View>, ()> {
    let mut file = eonos_user::File::open(SESSION_PATH, eonos_user::flags::READ_ONLY).map_err(|_| ())?;
    let state = file.read_to_string().map_err(|_| ())?;
    let mut lines = state.lines();
    if lines.next() != Some("ERA1") { return Err(()); }
    let view = match lines.next().ok_or(())? {
        "closed" => return Ok(None),
        "welcome" => View::Welcome,
        "files" => View::Files,
        "text" => View::TextViewer,
        "system" => View::System,
        "apps" => View::Apps,
        "konsul" => View::Konsul,
        "about" => View::About,
        "settings" => View::Settings,
        _ => return Err(()),
    };
    if let Some(directory) = lines.next() {
        if !directory.is_empty() && !directory.contains('\n') {
            files.navigate(String::from(directory));
        }
    }
    if let Some(offset) = lines.next().and_then(|value| value.parse::<usize>().ok()) {
        files.offset = offset.min(files.listing.lines().count().saturating_sub(1));
    }
    if let Some(path) = lines.next().filter(|path| path.starts_with('/')) {
        files.opened_path = String::from(path);
        if view == View::TextViewer {
            files.open_file(String::from(path));
        }
    }
    Ok(Some(view))
}

fn save_session(view: Option<View>, files: &FileState) -> Result<(), eonos_user::Error> {
    let mut file = eonos_user::File::open(
        SESSION_PATH,
        eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
    )?;
    let state = format!("ERA1\n{}\n{}\n{}\n{}\n", view_name(view), files.directory, files.offset, files.opened_path);
    file.write_all(state.as_bytes())?;
    file.close()?;
    eonos_rt::sync()?;
    Ok(())
}

impl FileState {
    fn new() -> Self {
        let directory = String::from("/");
        let listing = eonos_user::list_directory(&directory)
            .unwrap_or_else(|error| format!("Unable to read directory: {}", error));
        Self { directory, listing, offset: 0, opened_path: String::new(), contents: String::new(), truncated: false }
    }

    fn navigate(&mut self, path: String) {
        match eonos_user::list_directory(&path) {
            Ok(listing) => { self.directory = path; self.listing = listing; self.offset = 0; }
            Err(error) => eonos_rt::eprintln!("Files: {}", error),
        }
    }

    fn open_file(&mut self, path: String) {
        const LIMIT: usize = 8192;
        let result = (|| {
            let mut file = eonos_user::File::open(&path, eonos_user::flags::READ_ONLY)?;
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 512];
            while bytes.len() < LIMIT {
                let amount = (LIMIT - bytes.len()).min(chunk.len());
                let count = file.read(&mut chunk[..amount])?;
                if count == 0 { break; }
                bytes.extend_from_slice(&chunk[..count]);
            }
            Ok::<_, eonos_user::Error>((bytes, file))
        })();
        match result {
            Ok((bytes, _file)) => {
                self.truncated = bytes.len() == LIMIT;
                self.contents = String::from_utf8_lossy(&bytes).into_owned();
                self.opened_path = path;
            }
            Err(error) => {
                self.truncated = false;
                self.contents = format!("Unable to open this file: {}", error);
                self.opened_path = path;
            }
        }
    }
}

struct KonsulState {
    input: String,
    output: Vec<String>,
    history: Vec<String>,
    history_index: usize,
    history_draft: Option<String>,
    environment: Vec<String>,
    running: Option<KonsulCommand>,
}

struct KonsulCommand {
    child: eonos_user::Process,
    read_fd: usize,
    output: Vec<u8>,
    truncated: bool,
    finished: bool,
}

impl KonsulState {
    fn new(environment: &[&CStr]) -> Self {
        let mut environment: Vec<String> = environment.iter()
            .filter_map(|value| value.to_str().ok())
            .map(String::from)
            .collect();
        set_konsul_env(&mut environment, "PATH", "/disk/apps/bin:/apps/bin:/bin");
        set_konsul_env(&mut environment, "USER", "superuser");
        set_konsul_env(&mut environment, "HOSTNAME", "eonos");
        if let Ok(path) = eonos_user::current_dir() { set_konsul_env(&mut environment, "PWD", &path); }
        let mut state = Self { input: String::new(), output: Vec::new(), history: Vec::new(), history_index: 0, history_draft: None, environment, running: None };
        state.push_output("EonOS Konsul — interactive system shell");
        state.push_output("Type 'help' to list commands.");
        state
    }

    fn cwd(&self) -> &str {
        self.environment.iter().find_map(|entry| entry.strip_prefix("PWD=")).unwrap_or("/")
    }

    fn prompt(&self) -> String {
        format!("{}@{}:{}# ",
            env_value_konsul(&self.environment, "USER").unwrap_or("superuser"),
            env_value_konsul(&self.environment, "HOSTNAME").unwrap_or("eonos"), self.cwd())
    }

    fn push_output(&mut self, line: &str) {
        for line in line.lines() { self.output.push(String::from(line)); }
        if line.is_empty() { self.output.push(String::new()); }
        if self.output.len() > 160 { self.output.drain(..self.output.len() - 160); }
    }
}

fn env_value_konsul<'a>(environment: &'a [String], name: &str) -> Option<&'a str> {
    let prefix = format!("{}=", name);
    environment.iter().find_map(|entry| entry.strip_prefix(prefix.as_str()))
}

fn set_konsul_env(environment: &mut Vec<String>, name: &str, value: &str) {
    let prefix = format!("{}=", name);
    environment.retain(|entry| !entry.starts_with(prefix.as_str()));
    environment.push(format!("{}{}", prefix, value));
}

fn complete_konsul_input(state: &mut KonsulState) {
    let start = state.input.as_bytes().iter().rposition(|byte| byte.is_ascii_whitespace()).map_or(0, |index| index + 1);
    let prefix = &state.input[start..];

    let (directory_part, name_prefix) = match prefix.rsplit_once('/') {
        Some((directory, name)) => (Some(directory), name),
        None => (None, prefix),
    };
    let mut candidates: Vec<(String, bool)> = Vec::new();
    if start == 0 && directory_part.is_none() {
        for command in ["cat", "cd", "clear", "cp", "echo", "edit", "env", "exec", "exit", "export", "grep", "help", "head", "jobs", "kill", "ls", "lsblk", "mkdir", "mount", "mouse", "net", "nslookup", "ping", "pwd", "rm", "sleep", "sync", "tail", "touch", "tree", "unset", "uptime", "wait", "wc", "which", "write", "era", "curler"] {
            if command.starts_with(name_prefix) {
                candidates.push((String::from(command), false));
            }
        }
        if let Some(path) = state.environment.iter().find_map(|entry| entry.strip_prefix("PATH=")) {
            for directory in path.split(':').filter(|directory| !directory.is_empty()) {
                if let Ok(entries) = eonos_user::list_directory(directory) {
                    for entry in entries.lines().filter(|entry| !entry.ends_with('/')) {
                        if entry.starts_with(name_prefix) && !candidates.iter().any(|(name, _)| name == entry) {
                            candidates.push((String::from(entry), false));
                        }
                    }
                }
            }
        }
    } else {
        let directory = match directory_part {
            Some("") => String::from("/"),
            Some(path) if path.starts_with('/') => String::from(path),
            Some(path) => format!("{}/{}", state.cwd().trim_end_matches('/'), path),
            None => String::from(state.cwd()),
        };
        if let Ok(entries) = eonos_user::list_directory(&directory) {
            for entry in entries.lines() {
                let is_directory = entry.ends_with('/');
                let name = entry.strip_suffix('/').unwrap_or(entry);
                if !name.starts_with(name_prefix) { continue; }
                let candidate = match directory_part {
                    Some(part) => format!("{}/{}{}", part, name, if is_directory { "/" } else { "" }),
                    None => String::from(name),
                };
                candidates.push((candidate, is_directory));
            }
        }
    }

    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    if candidates.is_empty() { return; }
    if candidates.len() == 1 {
        let (candidate, is_directory) = candidates.pop().unwrap();
        state.input.truncate(start);
        state.input.push_str(&candidate);
        if is_directory {
            if !state.input.ends_with('/') { state.input.push('/'); }
        } else {
            state.input.push(' ');
        }
        return;
    }

    let mut common = candidates[0].0.clone();
    for (candidate, _) in candidates.iter().skip(1) {
        let shared = common.chars().zip(candidate.chars()).take_while(|(a, b)| a == b).count();
        common = common.chars().take(shared).collect();
    }
    if common.len() > prefix.len() {
        state.input.truncate(start);
        state.input.push_str(&common);
    } else {
        state.push_output(&candidates.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join("  "));
    }
}

fn set_konsul_focus(state: &mut KonsulState, focused: bool) {
    if focused {
        if let Err(error) = eonos_rt::keyboard_claim() {
            state.push_output(&format!("Konsul: could not claim keyboard input ({})", error.0));
        }
    } else {
        let _ = eonos_rt::keyboard_release();
    }
}

fn view_needs_input(view: View) -> bool {
    matches!(view, View::Konsul | View::AppWindow(_))
}

fn discover_installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    for (directory, executable_directory) in [
        ("/disk/apps/manifests", "/disk/apps/bin"),
        ("/apps/manifests", "/apps/bin"),
    ] {
        let Ok(entries) = eonos_user::list_directory(directory) else { continue; };
        for filename in entries.lines().take(64) {
            let Some(id) = filename.strip_suffix(".eapp") else { continue; };
            if id.is_empty() || id.len() > 32 || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-') { continue; }
            let path = format!("{}/{}.eapp", directory, id);
            let Ok(mut file) = eonos_user::File::open(&path, eonos_user::flags::READ_ONLY) else { continue; };
            let Ok(manifest) = file.read_to_string() else { continue; };
            let mut lines = manifest.lines();
            if lines.next() != Some("EONOSAPP1") || lines.next() != Some(id) { continue; }
            let Some(kind) = lines.next().filter(|kind| *kind == "SHELL" || *kind == "ERA") else { continue; };
            let Some(name) = lines.next().filter(|name| !name.is_empty() && name.len() <= 80) else { continue; };
            let Some(description) = lines.next().filter(|value| value.len() <= 160) else { continue; };
            if apps.iter().any(|app: &InstalledApp| app.id == id) { continue; }
            apps.push(InstalledApp {
                id: String::from(id), name: String::from(name), description: String::from(description),
                kind: String::from(kind), executable: format!("{}/{}", executable_directory, id),
            });
        }
    }
    apps
}

fn start_era_app(apps: &[InstalledApp], app_index: usize, sessions: &mut Vec<AppSession>) -> Result<usize, eonos_user::Error> {
    let app = apps.get(app_index).ok_or(eonos_user::Error::InvalidArgument)?;
    if app.kind != "ERA" { return Err(eonos_user::Error::InvalidArgument); }
    let (to_era_read, to_era_write) = eonos_rt::pipe()?;
    let (to_app_read, to_app_write) = match eonos_rt::pipe() {
        Ok(pair) => pair,
        Err(error) => { let _ = eonos_rt::close(to_era_read); let _ = eonos_rt::close(to_era_write); return Err(error.into()); }
    };
    let env = ["PATH=/disk/apps/bin:/apps/bin:/bin", "HOME=/", "USER=superuser", "HOSTNAME=eonos", "EONOS_ERA_SESSION=1"];
    let process = match eonos_user::Process::spawn_sandboxed_with_stdio(
        &app.executable, &[], &env, Some(to_app_read), Some(to_era_write),
    ) {
        Ok(process) => process,
        Err(error) => {
            let _ = eonos_rt::close(to_era_read); let _ = eonos_rt::close(to_era_write);
            let _ = eonos_rt::close(to_app_read); let _ = eonos_rt::close(to_app_write);
            return Err(error);
        }
    };
    let _ = eonos_rt::close(to_era_write);
    let _ = eonos_rt::close(to_app_read);
    let session_id = sessions.len();
    sessions.push(AppSession {
        title: app.name.clone(), width: 480, height: 300, process,
        output_fd: to_era_read, input_fd: to_app_write,
        pending_events: Vec::new(), incoming: Vec::new(), drawing: Vec::new(), closed: false,
        resources_closed: false, reaped: false,
    });
    Ok(session_id)
}

fn send_app_event(session: &mut AppSession, event: [u8; 16]) {
    if session.pending_events.len() + event.len() <= 4096 {
        session.pending_events.extend_from_slice(&event);
    }
    flush_app_events(session);
}

fn send_app_pointer(session: &mut AppSession, x: i32, y: i32, buttons: u8) {
    let mut event = [0u8; 16];
    event[0] = 3;
    event[1..5].copy_from_slice(&x.to_le_bytes());
    event[5..9].copy_from_slice(&y.to_le_bytes());
    event[9] = buttons;
    send_app_event(session, event);
}

fn flush_app_events(session: &mut AppSession) {
    while !session.pending_events.is_empty() {
        let amount = session.pending_events.len().min(4096);
        match eonos_rt::pipe_write_available(session.input_fd, &session.pending_events[..amount]) {
            Ok(0) | Err(_) => break,
            Ok(written) => { session.pending_events.drain(..written); }
        }
    }
}

fn poll_app_sessions(sessions: &mut [AppSession], layout: &Layout) -> bool {
    let mut redraw = false;
    let mut chunk = [0u8; 4096];
    for session in sessions {
        if session.closed {
            if !session.resources_closed {
                let _ = session.process.signal(15);
                let _ = eonos_rt::close(session.output_fd);
                let _ = eonos_rt::close(session.input_fd);
                session.resources_closed = true;
            }
            if !session.reaped && session.process.try_wait().ok().flatten().is_some() {
                session.reaped = true;
            }
            continue;
        }
        flush_app_events(session);
        for _ in 0..8 {
            let amount = match eonos_rt::pipe_read_available(session.output_fd, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(amount) => amount,
            };
            if session.incoming.len().saturating_add(amount) > 32 * 1024 { session.incoming.clear(); break; }
            session.incoming.extend_from_slice(&chunk[..amount]);
        }
        loop {
            if session.incoming.len() < 9 { break; }
            if &session.incoming[..4] != b"EAPP" { session.closed = true; break; }
            let kind = session.incoming[4];
            let len = u32::from_le_bytes(session.incoming[5..9].try_into().unwrap()) as usize;
            if len > 24 * 1024 { session.closed = true; break; }
            if session.incoming.len() < 9 + len { break; }
            let packet = session.incoming.drain(..9 + len).skip(9).collect::<Vec<_>>();
            match kind {
                1 if packet.len() >= 6 => {
                    let requested_w = u16::from_le_bytes([packet[0], packet[1]]) as i32;
                    let requested_h = u16::from_le_bytes([packet[2], packet[3]]) as i32;
                    let title_len = u16::from_le_bytes([packet[4], packet[5]]) as usize;
                    if title_len > 80 || packet.len() != 6 + title_len { session.closed = true; break; }
                    if let Ok(title) = core::str::from_utf8(&packet[6..]) {
                        if !title.is_empty() { session.title = String::from(title); }
                    }
                    let max_w = (layout.width - 80).clamp(240, 1920);
                    let max_h = (layout.taskbar_y - 80).clamp(160, 1080);
                    session.width = requested_w.clamp(240, max_w);
                    session.height = requested_h.clamp(160, max_h);
                    let mut response = [0u8; 16];
                    response[0] = 1;
                    response[1..5].copy_from_slice(&(session.width as u32).to_le_bytes());
                    response[5..9].copy_from_slice(&(session.height as u32).to_le_bytes());
                    send_app_event(session, response);
                    redraw = true;
                }
                2 => { parse_app_frame(session, &packet); redraw = true; }
                3 => { session.closed = true; redraw = true; }
                _ => { session.closed = true; break; }
            }
        }
        if session.process.try_wait().ok().flatten().is_some() {
            session.closed = true;
            session.reaped = true;
            redraw = true;
        }
        if session.closed {
            let _ = session.process.signal(15);
            let _ = eonos_rt::close(session.output_fd);
            let _ = eonos_rt::close(session.input_fd);
            session.resources_closed = true;
        }
    }
    redraw
}

fn close_app_sessions(sessions: &mut [AppSession]) {
    for session in sessions {
        if !session.resources_closed {
            let _ = session.process.signal(15);
            let _ = eonos_rt::close(session.output_fd);
            let _ = eonos_rt::close(session.input_fd);
            session.resources_closed = true;
        }
        if !session.reaped && session.process.wait().is_ok() { session.reaped = true; }
        session.closed = true;
    }
}

fn parse_app_frame(session: &mut AppSession, payload: &[u8]) {
    if payload.len() < 2 { return; }
    let count = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    if count > 256 { return; }
    let mut pos = 2;
    let mut ops = Vec::with_capacity(count);
    for _ in 0..count {
        let Some(tag) = payload.get(pos).copied() else { return; }; pos += 1;
        match tag {
            1 if pos + 20 <= payload.len() => {
                let x = i32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()); pos += 4;
                let y = i32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()); pos += 4;
                let w = u32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()); pos += 4;
                let h = u32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()); pos += 4;
                let color = u32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()) & 0x00ff_ffff; pos += 4;
                if w <= 4096 && h <= 4096 { ops.push(AppDrawOp::Rect(x, y, w, h, color)); }
            }
            2 if pos + 10 <= payload.len() => {
                let x = i32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()); pos += 4;
                let y = i32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()); pos += 4;
                let color = u32::from_le_bytes(payload[pos..pos+4].try_into().unwrap()) & 0x00ff_ffff; pos += 4;
                let len = u16::from_le_bytes(payload[pos..pos+2].try_into().unwrap()) as usize; pos += 2;
                if len > 1024 || pos + len > payload.len() { return; }
                let Ok(value) = core::str::from_utf8(&payload[pos..pos+len]) else { return; };
                ops.push(AppDrawOp::Text(x, y, color, String::from(value))); pos += len;
            }
            _ => return,
        }
    }
    if pos == payload.len() { session.drawing = ops; }
}

fn konsul_character(usage: u8, shifted: bool) -> Option<char> {
    let character = match usage {
        0x04..=0x1D => {
            let letter = (b'a' + usage - 0x04) as char;
            return Some(if shifted { letter.to_ascii_uppercase() } else { letter });
        }
        0x1E..=0x26 => (b'1' + usage - 0x1E) as char,
        0x27 => '0',
        0x2C => ' ',
        0x2D => if shifted { '_' } else { '-' },
        0x2E => if shifted { '+' } else { '=' },
        0x2F => if shifted { '{' } else { '[' },
        0x30 => if shifted { '}' } else { ']' },
        0x31 | 0x32 => if shifted { '|' } else { '\\' },
        0x33 => if shifted { ':' } else { ';' },
        0x34 => if shifted { '"' } else { '\'' },
        0x35 => if shifted { '~' } else { '`' },
        0x36 => if shifted { '<' } else { ',' },
        0x37 => if shifted { '>' } else { '.' },
        0x38 => if shifted { '?' } else { '/' },
        0x2B => '\t',
        _ => return None,
    };
    Some(if shifted && usage >= 0x1E && usage <= 0x27 {
        match character { '1' => '!', '2' => '@', '3' => '#', '4' => '$', '5' => '%', '6' => '^', '7' => '&', '8' => '*', '9' => '(', '0' => ')', _ => character }
    } else { character })
}

fn start_konsul_command(state: &mut KonsulState, command: &str) {
    if command.trim() == "clear" { state.output.clear(); }
    let (read_fd, write_fd) = match eonos_rt::pipe() {
        Ok(pipe) => pipe,
        Err(error) => { state.push_output(&format!("Konsul: pipe failed ({})", error.0)); return; }
    };
    let mut environment = state.environment.clone();
    set_konsul_env(&mut environment, "EONOS_ERA_SESSION", "1");
    let environment: Vec<&str> = environment.iter().map(String::as_str).collect();
    let child = match eonos_user::Process::spawn_with_stdio(
        "/bin/rust-shell", &["-c", command], &environment, None, Some(write_fd),
    ) {
        Ok(child) => child,
        Err(error) => {
            let _ = eonos_rt::close(read_fd);
            let _ = eonos_rt::close(write_fd);
            state.push_output(&format!("Konsul: could not start system shell: {}", error));
            return;
        }
    };
    let _ = eonos_rt::close(write_fd);
    state.running = Some(KonsulCommand { child, read_fd, output: Vec::new(), truncated: false, finished: false });
}

fn poll_konsul_command(state: &mut KonsulState) {
    if state.running.is_none() { return; }
    let mut buffer = [0u8; 2048];
    let mut drained_to_empty = false;
    let mut read_failure = None;
    let mut wait_failure = None;
    // Drain promptly so a chatty command cannot fill the pipe and block the
    // shell, while keeping every read nonblocking on Era's UI thread.
    {
        let command = state.running.as_mut().unwrap();
        for _ in 0..8 {
            let count = match eonos_rt::pipe_read_available(command.read_fd, &mut buffer) {
                Ok(count) => count,
                Err(error) => { read_failure = Some(error.0); break; }
            };
            if count == 0 { drained_to_empty = true; break; }
            let keep = count.min((64 * 1024usize).saturating_sub(command.output.len()));
            command.output.extend_from_slice(&buffer[..keep]);
            command.truncated |= keep < count;
        }
        if read_failure.is_none() && !command.finished {
            match command.child.try_wait() {
                Ok(Some(_)) => command.finished = true,
                Ok(None) => {}
                Err(error) => {
                    wait_failure = Some(format!("Konsul: wait failed: {}", error));
                    command.finished = true;
                }
            }
        }
    }
    if let Some(error) = read_failure {
        if let Some(command) = state.running.take() { let _ = eonos_rt::close(command.read_fd); }
        state.push_output(&format!("Konsul: output read failed ({})", error));
        return;
    }
    if let Some(error) = wait_failure { state.push_output(&error); }
    let command = state.running.as_ref().unwrap();
    // The child's exit closes its pipe reference. A final empty read means
    // all captured output has been consumed and the prompt can return.
    if !command.finished || !drained_to_empty { return; }
    let command = state.running.take().unwrap();
    let _ = eonos_rt::close(command.read_fd);

    let marker_start = b"\x1eKONSUL_STATE_BEGIN\n";
    let marker_end = b"\x1eKONSUL_STATE_END\n";
    let visible_end = command.output.windows(marker_start.len()).position(|window| window == marker_start);
    if let Some(start) = visible_end {
        let state_start = start + marker_start.len();
        if let Some(relative_end) = command.output[state_start..].windows(marker_end.len()).position(|window| window == marker_end) {
            let end = state_start + relative_end;
            let env_text = String::from_utf8_lossy(&command.output[state_start..end]);
            state.environment = env_text.lines().filter(|line| line.contains('=')).map(String::from).collect();
            if let Some(cwd) = env_value_konsul(&state.environment, "PWD") { let _ = eonos_rt::chdir(cwd); }
            let visible = String::from_utf8_lossy(&command.output[..start]);
            append_shell_output(state, &visible);
        } else {
            append_shell_output(state, &String::from_utf8_lossy(&command.output));
        }
    } else {
        append_shell_output(state, &String::from_utf8_lossy(&command.output));
    }
    if command.truncated { state.push_output("[output truncated at 64 KiB]"); }
}

fn append_shell_output(state: &mut KonsulState, output: &str) {
    let mut clean = String::new();
    let mut escape = false;
    let mut csi = false;
    for character in output.chars() {
        if escape {
            if !csi && character == '[' { csi = true; continue; }
            if csi && !((' '..='?').contains(&character)) { escape = false; csi = false; }
            else if !csi { escape = false; }
            continue;
        }
        if character == '\u{1b}' { escape = true; continue; }
        if character == '\r' { continue; }
        if character == '\u{1e}' || character == '\u{1f}' { continue; }
        clean.push(character);
    }
    if !clean.is_empty() { state.push_output(&clean); }
}

fn main(args: eonos_rt::Args<'_>) -> i32 {
    if !eonos_user::is_mountpoint("/disk").unwrap_or(false) {
        eonos_rt::eprintln!("era: a writable data disk must be mounted at /disk first");
        return 1;
    }
    let environment: Vec<&CStr> = args.envs().collect();
    let display = match eonos_rt::era::enter() {
        Ok(display) => display,
        Err(error) => {
            if error.0 == 16 {
                eonos_rt::println!("ERROR : Era Session Already Running! (EC : ESAR)");
                return 1;
            }
            eonos_rt::eprintln!("era: could not open graphics surface ({})", error.0);
            return 1;
        }
    };
    if display.width < 640 || display.height < 420 {
        eonos_rt::eprintln!("era: display must be at least 640x420");
        let _ = eonos_rt::era::exit();
        return 1;
    }

    let mut layout = make_layout(display.width as i32, display.height as i32);
    let mut normal_window = (layout.window_x, layout.window_y, layout.window_w, layout.window_h);
    let mut files = FileState::new();
    let mut konsul = KonsulState::new(&environment);
    let installed_apps = discover_installed_apps();
    let mut app_sessions: Vec<AppSession> = Vec::new();
    let mut processes = eonos_user::list_processes().unwrap_or_else(|_| String::from("Process data unavailable"));
    let mut devices = eonos_user::list_block_devices().unwrap_or_else(|_| String::from("No block devices"));
    load_settings();
    let mut view = if RESTORE_SESSION.load(Ordering::Relaxed) {
        restore_session(&mut files).unwrap_or(Some(View::Welcome))
    } else {
        Some(View::Welcome)
    };
    if let Some(initial) = view { open_window(&mut layout, initial); }
    let mut power_menu = false;
    let mut start_menu = false;
    let mut minimized = false;
    let mut maximized = false;
    let mut drag_offset: Option<(i32, i32)> = None;
    let mut pending_power = None;
    let mut power_status = String::new();
    if view.is_some_and(view_needs_input) { set_konsul_focus(&mut konsul, true); }
    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
    let _ = eonos_rt::era::present();
    let mut last_clock_minute = eonos_rt::wall_time_secs().unwrap_or(0) / 60;
    let mut last_app_pointer: Option<(usize, i32, i32, u8)> = None;
    let mut app_pointer_armed = false;

    // Debounce the physical button state. A single noisy USB report should
    // not open or close a window, and returning from the terminal must not
    // replay the click that launched it.
    let initial_buttons = eonos_rt::mouse_state().map(|state| state.buttons).unwrap_or(0);
    let mut stable_left = initial_buttons & 1 != 0;
    let mut candidate_left = stable_left;
    let mut candidate_since = eonos_rt::uptime_ms();
    loop {
        let _ = eonos_rt::sleep_ms(16);
        let app_changed = poll_app_sessions(&mut app_sessions, &layout);
        if app_changed && matches!(view, Some(View::AppWindow(_))) {
            draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
        }
        let output_line_count = konsul.output.len();
        let was_running = konsul.running.is_some();
        poll_konsul_command(&mut konsul);
        if was_running && (konsul.running.is_none() || konsul.output.len() != output_line_count) {
            draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
        }
        let now = eonos_rt::uptime_ms();
        let clock_minute = eonos_rt::wall_time_secs().unwrap_or(0) / 60;
        if clock_minute != last_clock_minute {
            last_clock_minute = clock_minute;
            draw_taskbar(&layout, view, minimized);
        }
        let mouse = match eonos_rt::mouse_state() {
            Ok(mouse) => mouse,
            Err(_) => continue,
        };
        let raw_left = mouse.buttons & 1 != 0;
        if raw_left != candidate_left {
            candidate_left = raw_left;
            candidate_since = now;
        }
        let mut left_pressed = false;
        if candidate_left != stable_left && now.wrapping_sub(candidate_since) >= 48 {
            stable_left = candidate_left;
            left_pressed = stable_left;
            if !stable_left { drag_offset = None; }
        }

        if !minimized && !start_menu && !power_menu {
            if let Some(View::AppWindow(index)) = view {
                let pointer_x = mouse.x - (layout.window_x + 4);
                let pointer_y = mouse.y - (layout.window_y + 28);
                if !app_pointer_armed && !stable_left { app_pointer_armed = true; }
                let buttons = (mouse.buttons & !1) | ((stable_left && app_pointer_armed) as u8);
                let state = (index, pointer_x, pointer_y, buttons);
                if last_app_pointer != Some(state) {
                    if let Some(session) = app_sessions.get_mut(index) {
                        send_app_pointer(session, pointer_x, pointer_y, buttons);
                    }
                    last_app_pointer = Some(state);
                }
            } else {
                last_app_pointer = None;
                app_pointer_armed = false;
            }
        } else {
            last_app_pointer = None;
            app_pointer_armed = false;
        }

        if stable_left {
            if let Some((offset_x, offset_y)) = drag_offset {
                let max_x = (layout.width - layout.window_w).max(0);
                let max_y = (layout.taskbar_y - 32).max(0);
                layout.window_x = (mouse.x - offset_x).clamp(0, max_x);
                layout.window_y = (mouse.y - offset_y).clamp(0, max_y);
                if let Some(active) = view { save_active_position(&mut layout, active); }
                draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
            }
        }

        if matches!(view, Some(View::Konsul | View::AppWindow(_))) && !minimized && !start_menu && !power_menu && (view != Some(View::Konsul) || konsul.running.is_none()) {
            let mut changed = false;
            while let Ok(Some(event)) = eonos_rt::keyboard_event() {
                if let Some(View::AppWindow(index)) = view {
                    if let Some(session) = app_sessions.get_mut(index) {
                        let mut raw = [0u8; 16];
                        raw[0] = 2;
                        raw[1] = event.usage;
                        raw[2] = event.pressed;
                        raw[3] = event.modifiers;
                        send_app_event(session, raw);
                    }
                    continue;
                }
                if event.pressed == 0 { continue; }
                match event.usage {
                    0x28 | 0x58 => {
                        let command = core::mem::take(&mut konsul.input);
                        if !command.trim().is_empty() {
                            let prompt = konsul.prompt();
                            konsul.push_output(&format!("{}{}", prompt, command));
                            konsul.history.push(command.clone());
                            if konsul.history.len() > 128 { konsul.history.remove(0); }
                            konsul.history_index = konsul.history.len();
                            konsul.history_draft = None;
                            start_konsul_command(&mut konsul, &command);
                        }
                        changed = true;
                    }
                    0x2A => { konsul.input.pop(); changed = true; }
                    0x52 if !konsul.history.is_empty() => {
                        if konsul.history_index == konsul.history.len() {
                            konsul.history_draft = Some(konsul.input.clone());
                        }
                        konsul.history_index = konsul.history_index.saturating_sub(1);
                        konsul.input = konsul.history[konsul.history_index].clone();
                        changed = true;
                    }
                    0x51 if konsul.history_index < konsul.history.len() => {
                        konsul.history_index += 1;
                        konsul.input = if konsul.history_index == konsul.history.len() {
                            konsul.history_draft.take().unwrap_or_default()
                        } else {
                            konsul.history[konsul.history_index].clone()
                        };
                        changed = true;
                    }
                    0x2B => {
                        complete_konsul_input(&mut konsul);
                        changed = true;
                    }
                    0x0F if event.modifiers & 0x11 != 0 => { konsul.output.clear(); changed = true; }
                    usage => if let Some(character) = konsul_character(usage, event.modifiers & 0x22 != 0) {
                        if konsul.input.len() < 512 { konsul.input.push(character); changed = true; }
                    },
                }
            }
            if changed { draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions); }
        }

        if left_pressed && !minimized && !start_menu && !power_menu {
            if let Some(next) = inactive_window_at(&layout, view.filter(|_| !minimized), mouse.x, mouse.y) {
                leave_maximized(&mut layout, view, normal_window, &mut maximized);
                open_window(&mut layout, next);
                view = Some(next);
                minimized = false;
                set_konsul_focus(&mut konsul, view_needs_input(next));
                draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                left_pressed = false;
            }
        }

        if left_pressed {
            let action = hit_test(&layout, view, minimized, mouse.x, mouse.y, &files, &installed_apps, start_menu, power_menu, pending_power);
            if start_menu && !matches!(&action, Action::StartMenu) {
                start_menu = false;
                draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
            }
            match action {
                Action::StartMenu => {
                    start_menu = !start_menu;
                    power_menu = false;
                    pending_power = None;
                    set_konsul_focus(&mut konsul, view.is_some_and(view_needs_input) && !start_menu && !minimized);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                    if start_menu { draw_start_menu(&layout); }
                }
                Action::CloseStartMenu => {
                    start_menu = false;
                    set_konsul_focus(&mut konsul, view.is_some_and(view_needs_input) && !minimized);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::BeginWindowDrag(offset_x, offset_y) => {
                    if maximized {
                        (layout.window_x, layout.window_y, layout.window_w, layout.window_h) = normal_window;
                        layout.window_x = (mouse.x - layout.window_w / 2).clamp(0, (layout.width - layout.window_w).max(0));
                        layout.window_y = (mouse.y - 12).clamp(0, (layout.taskbar_y - 32).max(0));
                        maximized = false;
                        normal_window = (layout.window_x, layout.window_y, layout.window_w, layout.window_h);
                        if let Some(active) = view { save_active_frame(&mut layout, active); }
                        drag_offset = Some((layout.window_w / 2, 12));
                    } else {
                        drag_offset = Some((offset_x, offset_y));
                    }
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::Minimize => {
                    minimized = true;
                    drag_offset = None;
                    set_konsul_focus(&mut konsul, false);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::Maximize => {
                    if maximized {
                        (layout.window_x, layout.window_y, layout.window_w, layout.window_h) = normal_window;
                        maximized = false;
                    } else {
                        normal_window = (layout.window_x, layout.window_y, layout.window_w, layout.window_h);
                        (layout.window_x, layout.window_y, layout.window_w, layout.window_h) =
                            (0, 0, layout.width, layout.taskbar_y);
                        maximized = true;
                    }
                    minimized = false;
                    if let Some(active) = view { save_active_frame(&mut layout, active); }
                    set_konsul_focus(&mut konsul, view.is_some_and(view_needs_input));
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::TaskWindow(index) => {
                    if let Some(next) = layout.windows.get(index).copied() {
                        if view == Some(next) && !minimized {
                            minimized = true;
                            set_konsul_focus(&mut konsul, false);
                        } else {
                            leave_maximized(&mut layout, view, normal_window, &mut maximized);
                            open_window(&mut layout, next);
                            view = Some(next);
                            minimized = false;
                            set_konsul_focus(&mut konsul, view_needs_input(next));
                        }
                    }
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::PowerMenu => {
                    power_menu = true;
                    start_menu = false;
                    set_konsul_focus(&mut konsul, false);
                    pending_power = None;
                    power_status.clear();
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                    draw_power_menu(&layout, pending_power, &power_status);
                }
                Action::PowerCancel => {
                    if pending_power.is_some() {
                        pending_power = None;
                        draw_power_menu(&layout, pending_power, &power_status);
                    } else {
                        power_menu = false;
                        set_konsul_focus(&mut konsul, view.is_some_and(view_needs_input) && !minimized);
                        draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                    }
                }
                Action::Shutdown => {
                    pending_power = Some(PowerAction::Shutdown);
                    draw_power_menu(&layout, pending_power, &power_status);
                }
                Action::Reboot => {
                    pending_power = Some(PowerAction::Reboot);
                    draw_power_menu(&layout, pending_power, &power_status);
                }
                Action::ConfirmPower => {
                    if let Err(error) = save_session(view, &files) {
                        eonos_rt::eprintln!("era: session not saved: {}", error);
                    }
                    let result = match pending_power {
                        Some(PowerAction::Shutdown) => eonos_rt::shutdown(),
                        Some(PowerAction::Reboot) => eonos_rt::reboot(),
                        None => Ok(()),
                    };
                    if let Err(error) = result {
                        power_status = format!("Power request failed: {}", error.0);
                        eonos_rt::eprintln!("era: power request failed ({})", error.0);
                        pending_power = None;
                        power_menu = true;
                        draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                        draw_power_menu(&layout, pending_power, &power_status);
                    }
                }
                Action::Exit => {
                    set_konsul_focus(&mut konsul, false);
                    close_app_sessions(&mut app_sessions);
                    let _ = save_session(view, &files);
                    let _ = eonos_rt::era::exit();
                    return 0;
                }
                Action::Terminal => {
                    minimized = false;
                    set_konsul_focus(&mut konsul, false);
                    close_app_sessions(&mut app_sessions);
                    let _ = save_session(view, &files);
                    let _ = eonos_rt::era::exit();
                    run_terminal(&environment);
                    match eonos_rt::era::enter() {
                        Ok(_) => {
                            view = if RESTORE_SESSION.load(Ordering::Relaxed) {
                                restore_session(&mut files).unwrap_or(Some(View::Welcome))
                            } else {
                                Some(View::Welcome)
                            };
                            if let Some(restored) = view { open_window(&mut layout, restored); }
                            set_konsul_focus(&mut konsul, view.is_some_and(view_needs_input));
                            stable_left = eonos_rt::mouse_state().map(|state| state.buttons & 1 != 0).unwrap_or(false);
                            candidate_left = stable_left;
                            candidate_since = eonos_rt::uptime_ms();
                            draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                        }
                        Err(error) => {
                            eonos_rt::eprintln!("era: could not resume graphics ({})", error.0);
                            return 1;
                        }
                    }
                }
                Action::LaunchDoom => {
                    minimized = false;
                    set_konsul_focus(&mut konsul, false);
                    close_app_sessions(&mut app_sessions);
                    let _ = save_session(view, &files);
                    let _ = eonos_rt::era::exit();
                    let status = run_registered_app(&APPLICATIONS[0], &environment);
                    APP_LAUNCH_STATUS.store(status, Ordering::Relaxed);
                    match eonos_rt::era::enter() {
                        Ok(_) => {
                            view = if RESTORE_SESSION.load(Ordering::Relaxed) {
                                restore_session(&mut files).unwrap_or(Some(View::Apps))
                            } else { Some(View::Apps) };
                            if let Some(restored) = view { open_window(&mut layout, restored); }
                            set_konsul_focus(&mut konsul, view.is_some_and(view_needs_input));
                            stable_left = eonos_rt::mouse_state().map(|state| state.buttons & 1 != 0).unwrap_or(false);
                            candidate_left = stable_left;
                            candidate_since = eonos_rt::uptime_ms();
                            draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                        }
                        Err(error) => {
                            eonos_rt::eprintln!("era: could not resume graphics after DOOM ({})", error.0);
                            return 1;
                        }
                    }
                }
                Action::LaunchPackage(app_index) => {
                    let Some(app) = installed_apps.get(app_index) else { continue; };
                    if app.kind == "SHELL" {
                        open_window(&mut layout, View::Konsul);
                        view = Some(View::Konsul);
                        minimized = false;
                        set_konsul_focus(&mut konsul, true);
                        start_konsul_command(&mut konsul, &app.id);
                    } else {
                        match start_era_app(&installed_apps, app_index, &mut app_sessions) {
                            Ok(session_id) => {
                                let app_view = View::AppWindow(session_id);
                                open_window(&mut layout, app_view);
                                if let Some(session) = app_sessions.get(session_id) {
                                    if let Some(frame) = layout.placements.iter_mut().find(|frame| frame.view == app_view) {
                                        frame.w = (session.width + 8).min(layout.width - 24).max(280);
                                        frame.h = (session.height + 32).min(layout.taskbar_y - 16).max(190);
                                        frame.x = ((layout.width - frame.w) / 2).max(0);
                                        frame.y = ((layout.taskbar_y - frame.h) / 2).max(0);
                                    }
                                }
                                sync_window_layout(&mut layout, app_view);
                                view = Some(app_view);
                                minimized = false;
                                set_konsul_focus(&mut konsul, true);
                            }
                            Err(error) => konsul.push_output(&format!("Era: app launch failed: {}", error)),
                        }
                    }
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::Open(next) => {
                    minimized = false;
                    if view != Some(next) { leave_maximized(&mut layout, view, normal_window, &mut maximized); }
                    open_window(&mut layout, next);
                    set_konsul_focus(&mut konsul, view_needs_input(next));
                    if next == View::Settings {
                        refresh_disk_status();
                    }
                    if next == View::System {
                        processes = eonos_user::list_processes().unwrap_or_else(|_| String::from("Process data unavailable"));
                        devices = eonos_user::list_block_devices().unwrap_or_else(|_| String::from("No block devices"));
                    }
                    view = Some(next);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::WallpaperNext => {
                    WALLPAPER_THEME.store((WALLPAPER_THEME.load(Ordering::Relaxed) + 1) % 3, Ordering::Relaxed);
                    save_settings();
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::ToggleRestore => {
                    RESTORE_SESSION.store(!RESTORE_SESSION.load(Ordering::Relaxed), Ordering::Relaxed);
                    save_settings();
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::TimezoneStep(direction) => {
                    let current = TIMEZONE_OFFSET.load(Ordering::Relaxed);
                    TIMEZONE_OFFSET.store((current + direction).clamp(-12, 14), Ordering::Relaxed);
                    save_settings();
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::SettingsTab(tab) => {
                    SETTINGS_TAB.store(tab, Ordering::Relaxed);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::SystemTab(tab) => {
                    SYSTEM_TAB.store(tab, Ordering::Relaxed);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::Navigate(path) => {
                    files.navigate(path);
                    if view != Some(View::Files) { leave_maximized(&mut layout, view, normal_window, &mut maximized); }
                    open_window(&mut layout, View::Files);
                    view = Some(View::Files);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::OpenFile(path) => {
                    files.open_file(path);
                    if view != Some(View::TextViewer) { leave_maximized(&mut layout, view, normal_window, &mut maximized); }
                    open_window(&mut layout, View::TextViewer);
                    view = Some(View::TextViewer);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::BackToFiles => {
                    if view != Some(View::Files) { leave_maximized(&mut layout, view, normal_window, &mut maximized); }
                    open_window(&mut layout, View::Files);
                    view = Some(View::Files);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::Page(direction) => {
                    let page_size = ((layout.window_h - 185) / 22).clamp(1, 12) as usize;
                    let total = files.listing.lines().count();
                    if direction < 0 { files.offset = files.offset.saturating_sub(page_size); }
                    else if files.offset + page_size < total { files.offset += page_size; }
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::Close => {
                    set_konsul_focus(&mut konsul, false);
                    if let Some(View::AppWindow(index)) = view {
                        if let Some(session) = app_sessions.get_mut(index) { session.closed = true; }
                    }
                    if maximized { leave_maximized(&mut layout, view, normal_window, &mut maximized); }
                    if let Some(closing) = view {
                        layout.windows.retain(|open| *open != closing);
                        layout.placements.retain(|frame| frame.view != closing);
                    }
                    view = layout.windows.last().copied();
                    if let Some(active) = view { sync_window_layout(&mut layout, active); }
                    set_konsul_focus(&mut konsul, view.is_some_and(view_needs_input));
                    minimized = false;
                    if maximized {
                        (layout.window_x, layout.window_y, layout.window_w, layout.window_h) = normal_window;
                        maximized = false;
                    }
                    let _ = save_session(view, &files);
                    draw_era(&layout, view, minimized, &konsul, &files, &processes, &devices, &installed_apps, &app_sessions);
                }
                Action::None => {}
            }
        }
        let _ = eonos_rt::era::present();
    }
}

enum Action {
    None,
    StartMenu,
    CloseStartMenu,
    BeginWindowDrag(i32, i32),
    Minimize,
    Maximize,
    TaskWindow(usize),
    Exit,
    Terminal,
    LaunchDoom,
    LaunchPackage(usize),
    Open(View),
    Navigate(String),
    OpenFile(String),
    BackToFiles,
    Page(i8),
    Close,
    PowerMenu,
    PowerCancel,
    Shutdown,
    Reboot,
    ConfirmPower,
    WallpaperNext,
    ToggleRestore,
    SystemTab(u8),
    SettingsTab(u8),
    TimezoneStep(i8),
}

#[derive(Clone, Copy)]
enum PowerAction { Shutdown, Reboot }

fn make_layout(width: i32, height: i32) -> Layout {
    let window_w = (width - 64).min(720);
    let window_h = (height - 170).min(440);
    Layout {
        width,
        height,
        window_x: (width - window_w) / 2,
        window_y: (height - window_h) / 2 - 18,
        window_w,
        window_h,
        taskbar_y: height - 30,
        windows: Vec::new(),
        placements: Vec::new(),
    }
}

fn open_window(layout: &mut Layout, view: View) {
    if !layout.placements.iter().any(|placement| placement.view == view) {
        let w = (layout.width - 64).min(720);
        let h = (layout.height - 170).min(440);
        let (x, y) = if let Some(previous) = layout.placements.last() {
            let cascade = layout.placements.len() as i32 - 1;
            (previous.x + 36 + cascade * 8, previous.y + 32 + cascade * 8)
        } else {
            (layout.window_x, layout.window_y)
        };
        let x = x.clamp(0, (layout.width - w).max(0));
        let y = y.clamp(0, (layout.taskbar_y - h).max(0));
        layout.placements.push(WindowPlacement { view, x, y, w, h });
    }
    layout.windows.retain(|open| *open != view);
    layout.windows.push(view);
    sync_window_layout(layout, view);
}

fn sync_window_layout(layout: &mut Layout, view: View) {
    if let Some(frame) = layout.placements.iter().find(|frame| frame.view == view).copied() {
        layout.window_x = frame.x;
        layout.window_y = frame.y;
        layout.window_w = frame.w;
        layout.window_h = frame.h;
    }
}

fn save_active_position(layout: &mut Layout, view: View) {
    if let Some(frame) = layout.placements.iter_mut().find(|frame| frame.view == view) {
        frame.x = layout.window_x;
        frame.y = layout.window_y;
    }
}

fn save_active_frame(layout: &mut Layout, view: View) {
    if let Some(frame) = layout.placements.iter_mut().find(|frame| frame.view == view) {
        frame.x = layout.window_x;
        frame.y = layout.window_y;
        frame.w = layout.window_w;
        frame.h = layout.window_h;
    }
}

fn leave_maximized(layout: &mut Layout, view: Option<View>, normal: (i32, i32, i32, i32), maximized: &mut bool) {
    if !*maximized { return; }
    (layout.window_x, layout.window_y, layout.window_w, layout.window_h) = normal;
    if let Some(active) = view { save_active_frame(layout, active); }
    *maximized = false;
}

fn window_layout(base: &Layout, index: usize) -> Layout {
    let mut layout = base.clone();
    layout.windows.clear();
    layout.placements.clear();
    if let Some(view) = base.windows.get(index) {
        if let Some(frame) = base.placements.iter().find(|frame| frame.view == *view) {
            layout.window_x = frame.x;
            layout.window_y = frame.y;
            layout.window_w = frame.w;
            layout.window_h = frame.h;
        }
    }
    layout
}

fn inactive_window_at(layout: &Layout, active: Option<View>, x: i32, y: i32) -> Option<View> {
    if active.is_some() {
        let active_layout = window_layout(layout, layout.windows.iter().position(|item| Some(*item) == active).unwrap_or(0));
        if x >= active_layout.window_x && x < active_layout.window_x + active_layout.window_w
            && y >= active_layout.window_y && y < active_layout.window_y + active_layout.window_h
        { return None; }
    }
    for (index, view) in layout.windows.iter().enumerate().rev() {
        if Some(*view) == active { continue; }
        let window = window_layout(layout, index);
        if x >= window.window_x && x < window.window_x + window.window_w
            && y >= window.window_y && y < window.window_y + window.window_h {
            return Some(*view);
        }
    }
    None
}

fn hit_test(layout: &Layout, view: Option<View>, minimized: bool, x: i32, y: i32, files: &FileState, apps: &[InstalledApp], start_menu: bool, power_menu: bool, pending_power: Option<PowerAction>) -> Action {
    let start_x = 4;
    let start_y = layout.taskbar_y - 294;
    if start_menu {
        if x >= start_x && x < start_x + 248 && y >= start_y && y < layout.taskbar_y {
            if y >= start_y + 42 && y < start_y + 266 {
                let row = (y - start_y - 42) / 28;
                return match row {
                    0 => Action::Terminal,
                    1 => Action::Open(View::Files),
                    2 => Action::Open(View::System),
                    3 => Action::Open(View::Apps),
                    4 => Action::Open(View::Konsul),
                    5 => Action::Open(View::Settings),
                    6 => Action::Open(View::About),
                    7 => Action::LaunchDoom,
                    _ => Action::None,
                };
            }
            return Action::None;
        }
        if x >= start_x && x < start_x + 94 && y >= layout.taskbar_y && y < layout.height {
            return Action::StartMenu;
        }
        return Action::CloseStartMenu;
    }
    if power_menu && pending_power.is_some() {
        let mx = layout.width - 372;
        let my = layout.taskbar_y - 250;
        if x >= mx + 24 && x < mx + 170 && y >= my + 122 && y < my + 170 { return Action::ConfirmPower; }
        if x >= mx + 190 && x < mx + 336 && y >= my + 122 && y < my + 170 { return Action::PowerCancel; }
        return Action::None;
    }
    if power_menu {
        let mx = layout.width - 372;
        let my = layout.taskbar_y - 250;
        if x >= mx + 24 && x < mx + 170 && y >= my + 92 && y < my + 140 { return Action::Shutdown; }
        if x >= mx + 190 && x < mx + 336 && y >= my + 92 && y < my + 140 { return Action::Reboot; }
        if x >= mx + 24 && x < mx + 170 && y >= my + 164 && y < my + 204 { return Action::Exit; }
        if x >= mx + 190 && x < mx + 336 && y >= my + 164 && y < my + 204 { return Action::PowerCancel; }
        return Action::None;
    }
    if y >= layout.taskbar_y && y < layout.height {
        if x >= 4 && x < 98 { return Action::StartMenu; }
        let available = (layout.width - 254).max(1);
        if !layout.windows.is_empty() && x >= 104 && x < 104 + available {
            let tab_width = (available / layout.windows.len() as i32).max(1);
            let index = ((x - 104) / tab_width).max(0) as usize;
            if !layout.windows.is_empty() { return Action::TaskWindow(index.min(layout.windows.len() - 1)); }
        }
        if x >= layout.width - 142 && x < layout.width - 76 { return Action::PowerMenu; }
        return Action::None;
    }
    if let Some(view) = view.filter(|_| !minimized) {
        let controls_x = layout.window_x + layout.window_w - 65;
        if y >= layout.window_y + 6 && y < layout.window_y + 24 {
            if x >= controls_x && x < controls_x + 18 { return Action::Minimize; }
            if x >= controls_x + 20 && x < controls_x + 38 { return Action::Maximize; }
            if x >= controls_x + 40 && x < controls_x + 58 { return Action::Close; }
        }
        let close_x = layout.window_x + layout.window_w - 25;
        if x >= close_x && x < close_x + 18 && y >= layout.window_y + 5 && y < layout.window_y + 23 {
            return Action::Close;
        }
        if x >= layout.window_x + 4 && x < layout.window_x + layout.window_w - 65
            && y >= layout.window_y + 4 && y < layout.window_y + 28
        {
            return Action::BeginWindowDrag(x - layout.window_x, y - layout.window_y);
        }
        if view == View::Welcome {
            let button_y = layout.window_y + layout.window_h - 76;
            if y >= button_y && y < button_y + 42 {
                if x >= layout.window_x + 32 && x < layout.window_x + 190 {
                    return Action::Terminal;
                }
                if x >= layout.window_x + 202 && x < layout.window_x + 360 {
                    return Action::Open(View::Files);
                }
            }
        } else if view == View::Files && x >= layout.window_x && x < layout.window_x + layout.window_w {
            if y >= layout.window_y + 88 && y < layout.window_y + 116 {
                return Action::Navigate(parent_path(&files.directory));
            }
            if y >= layout.window_y + layout.window_h - 66 && y < layout.window_y + layout.window_h - 20 {
                let previous_x = layout.window_x + layout.window_w - 190;
                let next_x = layout.window_x + layout.window_w - 108;
                if x >= previous_x && x < previous_x + 72 && files.offset > 0 { return Action::Page(-1); }
                if x >= next_x && x < next_x + 72 { return Action::Page(1); }
                if x >= layout.window_x + 34 && x < layout.window_x + 122 {
                    return Action::Navigate(parent_path(&files.directory));
                }
            }
            if y >= layout.window_y + 130 {
                let index = ((y - layout.window_y - 130) / 22) as usize;
                let visible_rows = ((layout.window_h - 185) / 22).clamp(0, 12) as usize;
                if let Some(name) = files.listing.lines().nth(files.offset + index).filter(|_| index < visible_rows) {
                    return if name.ends_with('/') {
                        Action::Navigate(join_path(&files.directory, &name[..name.len() - 1]))
                    } else {
                        Action::OpenFile(join_path(&files.directory, name))
                    };
                }
            }
        } else if view == View::Apps
            && x >= layout.window_x + 34
            && x < layout.window_x + layout.window_w - 34
            && y >= layout.window_y + 122
            && y < layout.window_y + 170
        {
            return Action::LaunchDoom;
        } else if view == View::Apps
            && x >= layout.window_x + 34
            && x < layout.window_x + layout.window_w - 34
            && y >= layout.window_y + 176
            && y < layout.window_y + 224
        {
            return Action::Open(View::Konsul);
        } else if view == View::Apps && y >= layout.window_y + 252 {
            let index = ((y - layout.window_y - 252) / 54) as usize;
            if index < 4 && apps.get(index).is_some() {
                return Action::LaunchPackage(index);
            }
        } else if view == View::TextViewer
            && x >= layout.window_x
            && x < layout.window_x + layout.window_w
            && y >= layout.window_y + layout.window_h - 60
        {
            return Action::BackToFiles;
        } else if view == View::System
            && y >= layout.window_y + 90
            && y < layout.window_y + 120
        {
            let tab_x = layout.window_x + 34;
            if x >= tab_x && x < tab_x + 92 { return Action::SystemTab(0); }
            if x >= tab_x + 100 && x < tab_x + 192 { return Action::SystemTab(1); }
            if x >= tab_x + 200 && x < tab_x + 284 { return Action::SystemTab(2); }
        } else if view == View::Settings {
            if y >= layout.window_y + 90 && y < layout.window_y + 120 {
                let tab_x = layout.window_x + 34;
                if x >= tab_x && x < tab_x + 92 { return Action::SettingsTab(0); }
                if x >= tab_x + 100 && x < tab_x + 192 { return Action::SettingsTab(1); }
            }
            let tab = SETTINGS_TAB.load(Ordering::Relaxed);
            if tab == 0 && y >= layout.window_y + 122 && y < layout.window_y + 170
                && x >= layout.window_x + layout.window_w - 190
                && x < layout.window_x + layout.window_w - 34
            {
                return Action::WallpaperNext;
            }
            if tab == 0 && y >= layout.window_y + 190 && y < layout.window_y + 238
                && x >= layout.window_x + layout.window_w - 190
                && x < layout.window_x + layout.window_w - 34
            {
                return Action::ToggleRestore;
            }
            if tab == 1 && y >= layout.window_y + 142 && y < layout.window_y + 190 {
                if x >= layout.window_x + layout.window_w - 190 && x < layout.window_x + layout.window_w - 114 { return Action::TimezoneStep(-1); }
                if x >= layout.window_x + layout.window_w - 108 && x < layout.window_x + layout.window_w - 34 { return Action::TimezoneStep(1); }
            }
        }
    }
    Action::None
}

fn join_path(directory: &str, name: &str) -> String {
    if directory == "/" { format!("/{}", name) }
    else { format!("{}/{}", directory.trim_end_matches('/'), name) }
}

fn parent_path(path: &str) -> String {
    if path == "/" { return String::from("/"); }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => String::from("/"),
        Some(index) => String::from(&trimmed[..index]),
    }
}

fn draw_era(layout: &Layout, view: Option<View>, minimized: bool, konsul: &KonsulState, files: &FileState, processes: &str, devices: &str, apps: &[InstalledApp], sessions: &[AppSession]) {
    draw_desktop(layout, view, minimized, konsul, files, processes, devices, apps, sessions);
}

fn draw_desktop(layout: &Layout, view: Option<View>, minimized: bool, konsul: &KonsulState, files: &FileState, processes: &str, devices: &str, apps: &[InstalledApp], sessions: &[AppSession]) {
    draw_wallpaper_region(layout, 0, 0, layout.width, layout.height);
    draw_wallpaper_accents(layout, (0, 0, layout.width, layout.height));

    if !minimized {
        for (index, window_view) in layout.windows.iter().enumerate() {
            let window = window_layout(layout, index);
            let focused = Some(*window_view) == view;
            draw_window(&window, *window_view, files, processes, devices, focused, apps, sessions);
            if *window_view == View::Konsul {
                draw_konsul_contents(&window, konsul);
            }
        }
    }
    draw_taskbar(layout, view, minimized);
}

fn draw_konsul_contents(layout: &Layout, konsul: &KonsulState) {
    let x = layout.window_x + 14;
    let y = layout.window_y + 36;
    let width = layout.window_w - 28;
    let height = layout.window_h - 50;
    rect(x, y, width, height, 0x080B08);
    let columns = ((width - 16) / 8).max(1) as usize;
    let rows = ((height - 24) / 18).max(1) as usize;
    let visible = rows.saturating_sub(1);
    let start = konsul.output.len().saturating_sub(visible);
    for (row, line) in konsul.output.iter().skip(start).enumerate() {
        let value: String = line.chars().take(columns).collect();
        text(x + 8, y + 6 + row as i32 * 18, 0x39FF65, &value);
    }
    if konsul.running.is_some() {
        text(x + 8, y + height - 20, 0x39FF65, "Running command...");
    } else {
        let prompt = konsul.prompt();
        let max_input = columns.saturating_sub(prompt.len());
        let input: String = konsul.input.chars().rev().take(max_input).collect::<String>().chars().rev().collect();
        text(x + 8, y + height - 20, 0x39FF65, &format!("{}{}", prompt, input));
    }
}

fn draw_power_menu(layout: &Layout, pending: Option<PowerAction>, status: &str) {
    let x = layout.width - 372;
    let y = layout.taskbar_y - 250;
    let title = match pending {
        Some(PowerAction::Shutdown) => "Confirm power off",
        Some(PowerAction::Reboot) => "Confirm restart",
        None => "Power options",
    };
    draw_classic_menu_frame(x, y, 360, 250, title);
    if let Some(action) = pending {
        let question = match action {
            PowerAction::Shutdown => "Save the ERA session and power off EonOS?",
            PowerAction::Reboot => "Save the ERA session and restart EonOS?",
        };
        text(x + 24, y + 72, 0x202020, question);
        classic_button(x + 24, y + 122, 146, 40, "Confirm");
        classic_button(x + 190, y + 122, 146, 40, "Cancel");
    } else {
        text(x + 24, y + 62, 0x202020, "Save this desktop session, then:");
        classic_button(x + 24, y + 92, 146, 40, "Power off");
        classic_button(x + 190, y + 92, 146, 40, "Restart");
        classic_button(x + 24, y + 164, 146, 36, "Exit EonOS");
        classic_button(x + 190, y + 164, 146, 36, "Cancel");
    }
    if !status.is_empty() { text(x + 20, y + 220, 0x8A2020, status); }
}

fn draw_classic_panel(x: i32, y: i32, width: i32, height: i32) {
    rect(x + 4, y + 4, width, height, 0x303030);
    rect(x, y, width, height, 0xC0C0C0);
    rect(x, y, width, 2, 0xFFFFFF);
    rect(x, y, 2, height, 0xFFFFFF);
    rect(x + width - 2, y + 2, 2, height - 2, 0x404040);
    rect(x + 2, y + height - 2, width - 2, 2, 0x404040);
}

fn draw_classic_menu_frame(x: i32, y: i32, width: i32, height: i32, title: &str) {
    draw_classic_panel(x, y, width, height);
    let header_w = width - 8;
    rect(x + 4, y + 4, header_w, 34, 0x12377B);
    rect(x + 4, y + 4, header_w, 2, 0x315FA8);
    rect(x + 4, y + 36, header_w, 2, 0x0B2452);
    text(x + 14, y + 13, 0xFFFFFF, title);
}

fn classic_button(x: i32, y: i32, width: i32, height: i32, label: &str) {
    rect(x, y, width, height, 0xC0C0C0);
    rect(x, y, width, 2, 0xFFFFFF);
    rect(x, y, 2, height, 0xFFFFFF);
    rect(x + width - 2, y + 2, 2, height - 2, 0x404040);
    rect(x + 2, y + height - 2, width - 2, 2, 0x404040);
    text(x + (width - label.len() as i32 * 8) / 2, y + (height - 16) / 2, 0x101010, label);
}

fn draw_start_menu(layout: &Layout) {
    let x = 4;
    let y = layout.taskbar_y - 294;
    draw_classic_menu_frame(x, y, 248, 288, "EonOS Applications");
    let entries = ["Terminal", "Files", "System Monitor", "Applications", "Konsul", "Settings", "About Era", "DOOM / Freedoom"];
    for (index, label) in entries.iter().enumerate() {
        let row_y = y + 44 + index as i32 * 28;
        rect(x + 10, row_y + 4, 20, 20, if index == 7 { 0x702820 } else { 0x315FA8 });
        text(x + 16, row_y + 6, 0xFFFFFF, if index == 7 { "D" } else { "E" });
        text(x + 40, row_y + 6, 0x101010, label);
        if index == 5 { rect(x + 38, row_y + 25, 198, 1, 0x808080); }
    }
}

fn draw_wallpaper_region(layout: &Layout, x: i32, y: i32, width: i32, height: i32) {
    const BANDS: i32 = 32;
    let bottom = y.saturating_add(height);
    for band in 0..BANDS {
        let band_top = layout.height * band / BANDS;
        let band_bottom = layout.height * (band + 1) / BANDS;
        let top = y.max(band_top);
        let end = bottom.min(band_bottom);
        if top < end {
            rect(x, top, width, end - top, wallpaper_color(band));
        }
    }
}

fn wallpaper_color(band: i32) -> u32 {
    let amount = band as u32;
    let (start, end) = match WALLPAPER_THEME.load(Ordering::Relaxed) {
        1 => ((0x20u32, 0x16, 0x32), (0x68, 0x37, 0x78)),
        2 => ((0x12u32, 0x28, 0x22), (0x35, 0x68, 0x48)),
        _ => ((0x11u32, 0x18, 0x27), (0x39, 0x2E, 0x5B)),
    };
    let r = start.0 + (end.0 - start.0) * amount / 31;
    let g = start.1 + (end.1 - start.1) * amount / 31;
    let b = start.2 + (end.2 - start.2) * amount / 31;
    (r << 16) | (g << 8) | b
}

fn draw_wallpaper_accents(layout: &Layout, area: (i32, i32, i32, i32)) {
    let accents = [
        (layout.width - 340, 92, 250, 3),
        (layout.width - 300, 95, 3, 116),
        (72, layout.height / 2 + 110, 3, 88),
        (75, layout.height / 2 + 195, 190, 3),
    ];
    for accent in accents {
        draw_intersection(area, accent, 0x46517A);
    }
}

fn draw_intersection(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32), color: u32) {
    let left = a.0.max(b.0);
    let top = a.1.max(b.1);
    let right = a.0.saturating_add(a.2).min(b.0.saturating_add(b.2));
    let bottom = a.1.saturating_add(a.3).min(b.1.saturating_add(b.3));
    if left < right && top < bottom {
        rect(left, top, right - left, bottom - top, color);
    }
}

fn draw_window(layout: &Layout, view: View, files: &FileState, processes: &str, devices: &str, focused: bool, apps: &[InstalledApp], sessions: &[AppSession]) {
    let x = layout.window_x;
    let y = layout.window_y;
    let w = layout.window_w;
    let h = layout.window_h;
    // Only paint the two exposed edges of the shadow. Filling its full area
    // first redraws almost the entire window immediately before covering it.
    rect(x + w, y + 10, 8, h, 0x101522);
    rect(x + 8, y + h, w, 10, 0x101522);

    // The active window keeps the blue title bar; inactive windows use gray
    // chrome so focus is clear while preserving the same classic frame.
    rect(x, y, w, h, 0xC0C0C0);
    rect(x, y, w, 2, 0xFFFFFF);
    rect(x, y, 2, h, 0xFFFFFF);
    rect(x + w - 2, y + 2, 2, h - 2, 0x404040);
    rect(x + 2, y + h - 2, w - 2, 2, 0x404040);
    rect(x + 3, y + 3, w - 6, h - 6, PANEL);

    let title_x = x + 4;
    let title_y = y + 4;
    let title_w = w - 8;
    let (title_color, title_highlight, title_shadow) = if focused {
        (0x12377B, 0x315FA8, 0x0B2452)
    } else {
        (0x808080, 0xA0A0A0, 0x606060)
    };
    rect(title_x, title_y, title_w, 24, title_color);
    rect(title_x, title_y, title_w, 2, title_highlight);
    rect(title_x, title_y + 22, title_w, 2, title_shadow);
    draw_era_window_icon(x + 8, y + 9);
    let title = if let View::AppWindow(index) = view {
        sessions.get(index).map(|session| session.title.as_str()).unwrap_or("EonOS Application")
    } else { window_title(view) };
    text(x + 28, y + 8, 0xFFFFFF, title);

    // The compact gray control buttons use the raised Windows-style bevel.
    // Close is wired to Era's existing close action; minimize and maximize
    // are shown as window chrome while Era remains a single-window desktop.
    let controls_x = x + w - 65;
    draw_window_control(controls_x, y + 6, 18, 18, WindowControl::Minimize);
    draw_window_control(controls_x + 20, y + 6, 18, 18, WindowControl::Maximize);
    draw_window_control(controls_x + 40, y + 6, 18, 18, WindowControl::Close);

    match view {
        View::Welcome => {
            rect(x + 34, y + 68, 4, 44, ACCENT);
            text(x + 54, y + 72, TEXT, "Welcome to Era");
            text(x + 54, y + 104, MUTED, "A lightweight graphical environment for EonOS.");
            draw_sunken_panel(x + 34, y + 148, w - 68, 126.min(h - 230));
            text(x + 44, y + 168, AQUA, "READY");
            text(x + 44, y + 196, TEXT, "Your desktop is running with USB mouse input.");
            text(x + 44, y + 224, MUTED, "Open the terminal to use the EonOS command line.");
            text(x + 44, y + 252, MUTED, "The display updates only the areas that change.");
            let button_y = y + h - 76;
            button(x + 34, button_y, 156, 42, ACCENT, "Open terminal");
            button(x + 202, button_y, 156, 42, PANEL_RAISED, "Browse files");
        }
        View::Files => {
            text(x + 34, y + 68, TEXT, "Files");
            draw_sunken_panel(x + 34, y + 90, w - 68, 28);
            text(x + 42, y + 96, MUTED, &format!("Location: {}   (click to go up)", files.directory));
            draw_sunken_panel(x + 34, y + 124, w - 68, (h - 184).max(48));
            let max_chars = ((w - 100) / 8).max(8) as usize;
            let visible_rows = ((h - 185) / 22).clamp(0, 12) as usize;
            for (index, line) in files.listing.lines().skip(files.offset).take(visible_rows).enumerate() {
                let name: String = line.chars().take(max_chars).collect();
                let color = if line.ends_with('/') { ACCENT } else { TEXT };
                text(x + 46, y + 142 + index as i32 * 22, color, &name);
            }
            let total = files.listing.lines().count();
            let first = if total == 0 { 0 } else { files.offset + 1 };
            let last = (files.offset + visible_rows).min(total);
            button(x + 34, y + h - 58, 88, 32, PANEL_RAISED, "Parent");
            text(x + 132, y + h - 48, MUTED, &format!("{}-{} of {}", first, last, total));
            button(x + w - 190, y + h - 58, 72, 32, PANEL_RAISED, "Prev");
            button(x + w - 108, y + h - 58, 72, 32, PANEL_RAISED, "Next");
        }
        View::TextViewer => {
            let title: String = files.opened_path.chars().rev().take(48).collect::<String>().chars().rev().collect();
            text(x + 34, y + 68, TEXT, &title);
            draw_sunken_panel(x + 34, y + 98, w - 68, (h - 170).max(48));
            let max_chars = ((w - 94) / 8).max(8) as usize;
            let visible_lines = ((h - 180) / 20).clamp(0, 12) as usize;
            for (index, line) in files.contents.lines().take(visible_lines).enumerate() {
                let value: String = line.chars().take(max_chars).collect();
                text(x + 42, y + 112 + index as i32 * 20, TEXT, &value);
            }
            if files.truncated {
                text(x + 34, y + h - 78, GOLD, "Preview limited to the first 8 KiB.");
            }
            button(x + 34, y + h - 62, 112, 36, PANEL_RAISED, "Back to files");
        }
        View::System => {
            text(x + 34, y + 68, TEXT, "System monitor");
            let tab = SYSTEM_TAB.load(Ordering::Relaxed).min(2);
            for (index, (label, width)) in [("General", 92), ("Network", 92), ("OS", 84)].into_iter().enumerate() {
                let tab_x = x + 34 + index as i32 * 100;
                if index as u8 == tab {
                    rect(tab_x, y + 91, width, 29, 0xFFFFFF);
                    rect(tab_x, y + 91, width, 2, ACCENT);
                    rect(tab_x, y + 91, 2, 29, ACCENT);
                    rect(tab_x + width - 2, y + 93, 2, 27, PANEL_EDGE);
                    text(tab_x + (width - label.len() as i32 * 8) / 2, y + 98, ACCENT, label);
                } else {
                    button(tab_x, y + 93, width, 27, PANEL_RAISED, label);
                }
            }
            match tab {
                1 => {
                    draw_sunken_panel(x + 34, y + 136, w - 68, (h - 160).max(52));
                    text(x + 46, y + 146, GOLD, "Network interface");
                    match (eonos_rt::network_info(), eonos_rt::ipv4_config()) {
                        (Ok(adapter), Ok(config)) => {
                            text(x + 46, y + 172, TEXT, &format!("Device: Intel E1000 | Link: {}", if adapter.link_up != 0 { "up" } else { "down" }));
                            text(x + 46, y + 194, TEXT, &format!("MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", adapter.mac[0], adapter.mac[1], adapter.mac[2], adapter.mac[3], adapter.mac[4], adapter.mac[5]));
                            if config.configured != 0 {
                                text(x + 46, y + 216, TEXT, &format!("IPv4: {}.{}.{}.{} / {}.{}.{}.{}", config.address[0], config.address[1], config.address[2], config.address[3], config.netmask[0], config.netmask[1], config.netmask[2], config.netmask[3]));
                                text(x + 46, y + 238, TEXT, &format!("Gateway: {}.{}.{}.{}", config.gateway[0], config.gateway[1], config.gateway[2], config.gateway[3]));
                                text(x + 46, y + 260, TEXT, &format!("DNS: {}.{}.{}.{}", config.dns_server[0], config.dns_server[1], config.dns_server[2], config.dns_server[3]));
                                text(x + 46, y + 282, MUTED, &format!("DHCP lease: {} seconds", config.lease_seconds));
                            } else {
                                text(x + 46, y + 216, GOLD, "IPv4 is not configured. Run `net dhcp` in Konsul.");
                            }
                            text(x + 46, y + 318, AQUA, &format!("Packets received: {}", adapter.received));
                            text(x + 46, y + 340, AQUA, &format!("Packets sent: {}", adapter.transmitted));
                        }
                        _ => text(x + 46, y + 172, MUTED, "Network adapter information unavailable."),
                    }
                }
                2 => {
                    draw_sunken_panel(x + 34, y + 136, w - 68, (h - 160).max(52));
                    text(x + 46, y + 146, GOLD, "Operating system");
                    text(x + 46, y + 174, TEXT, "Name: EonOS");
                    text(x + 46, y + 198, TEXT, "Architecture: x86_64");
                    text(x + 46, y + 222, TEXT, "Bootloader: Limine");
                    text(x + 46, y + 246, TEXT, "Desktop: Era");
                    text(x + 46, y + 270, TEXT, "Root filesystem: read-only initramfs");
                    text(x + 46, y + 294, TEXT, &format!("Data disk: {}", if eonos_user::is_mountpoint("/disk").unwrap_or(false) { "mounted at /disk" } else { "not mounted" }));
                    text(x + 46, y + 326, MUTED, &format!("Display: {} × {}", layout.width, layout.height));
                }
                _ => {
                    let uptime = eonos_rt::uptime_ms();
                    let uptime_minutes = uptime / 60_000;
                    text(x + 34, y + 140, AQUA, &format!("Uptime: {}d {}h {}m", uptime_minutes / 1440, (uptime_minutes / 60) % 24, uptime_minutes % 60));
                    text(x + 34, y + 164, MUTED, &format!("Current process: {}", eonos_rt::getpid()));
                    if let Ok(memory) = eonos_rt::memory_info() {
                        text(x + 34, y + 188, MUTED, &format!("Free RAM: {} MiB | Kernel heap: {} / {} KiB", memory.free_physical / (1024 * 1024), memory.heap_used / 1024, memory.heap_total / 1024));
                    }
                    draw_sunken_panel(x + 34, y + 218, w - 68, (h - 330).max(48));
                    text(x + 42, y + 222, GOLD, "Processes");
                    let process_rows = ((h - 358) / 18).clamp(1, 4) as usize;
                    for (index, line) in processes.lines().take(process_rows).enumerate() {
                        let value: String = line.chars().take(((w - 90) / 8).max(8) as usize).collect();
                        text(x + 42, y + 242 + index as i32 * 18, TEXT, &value);
                    }
                    if h >= 400 {
                        draw_sunken_panel(x + 34, y + h - 92, w - 68, 58);
                        text(x + 42, y + h - 88, GOLD, "Storage");
                        for (index, line) in devices.lines().take(2).enumerate() {
                            let value: String = line.chars().take(((w - 90) / 8).max(8) as usize).collect();
                            text(x + 42, y + h - 66 + index as i32 * 18, MUTED, &value);
                        }
                    }
                }
            }
        }
        View::Apps => {
            text(x + 34, y + 68, TEXT, "Applications & tools");
            text(x + 34, y + 98, MUTED, "Built-in applications");
            let app = &APPLICATIONS[0];
            let app_y = y + 122;
            draw_app_row(x + 34, app_y, w - 68, app.name, app.description, true);
            draw_app_row(x + 34, app_y + 54, w - 68, "Konsul", "EonOS command shell", false);
            match APP_LAUNCH_STATUS.load(Ordering::Relaxed) {
                1 => text(x + 84, app_y + 28, GOLD, "Launch failed; check /doom/freedoom1.wad."),
                2 => text(x + 84, app_y + 28, GOLD, "DOOM exited with an error; check the IWAD."),
                _ => {}
            }
            text(x + 34, y + 230, TEXT, "Installed Rust applications");
            for (index, app) in apps.iter().take(3).enumerate() {
                draw_app_row(x + 34, y + 252 + index as i32 * 54, w - 68,
                    &app.name, &format!("{} ({})", app.description, app.kind), false);
            }
            if apps.is_empty() {
                text(x + 44, y + 262, MUTED, "Build a .eonapp package, then install it from Konsul.");
            }
            text(x + 34, y + h - 30, MUTED, "Select an application to launch it.");
        }
        View::About => {
            rect(x + 34, y + 72, 42, 42, ACCENT);
            text(x + 45, y + 85, 0xFFFFFF, "E");
            text(x + 92, y + 76, TEXT, "Era for EonOS");
            text(x + 92, y + 104, MUTED, "Fast, small, and built in Rust.");
            rect(x + 34, y + 142, w - 68, 1, PANEL_EDGE);
            text(x + 42, y + 166, TEXT, "Renderer");
            text(x + 190, y + 166, MUTED, "RAM back buffer + dirty rectangle presentation");
            text(x + 42, y + 198, TEXT, "Input");
            text(x + 190, y + 198, MUTED, "USB HID boot mouse");
            text(x + 42, y + 230, TEXT, "Terminal");
            text(x + 190, y + 230, MUTED, "Use the EonOS menu to open a shell");
            text(x + 42, y + h - 48, MUTED, "Era is the first step toward a full EonOS desktop.");
        }
        View::Settings => {
            text(x + 34, y + 68, TEXT, "Settings");
            for (index, (label, width)) in [("General", 92), ("Time", 92)].into_iter().enumerate() {
                let tab_x = x + 34 + index as i32 * 100;
                if index as u8 == SETTINGS_TAB.load(Ordering::Relaxed) {
                    rect(tab_x, y + 91, width, 29, 0xFFFFFF);
                    rect(tab_x, y + 91, width, 2, ACCENT);
                    rect(tab_x, y + 91, 2, 29, ACCENT);
                    rect(tab_x + width - 2, y + 93, 2, 27, PANEL_EDGE);
                    text(tab_x + (width - label.len() as i32 * 8) / 2, y + 98, ACCENT, label);
                } else {
                    button(tab_x, y + 93, width, 27, PANEL_RAISED, label);
                }
            }
            rect(x + 34, y + 120, w - 68, 1, PANEL_EDGE);
            if SETTINGS_TAB.load(Ordering::Relaxed) == 1 {
                text(x + 42, y + 152, TEXT, "Timezone");
                let offset = TIMEZONE_OFFSET.load(Ordering::Relaxed);
                let zone = format!("UTC{}{}", if offset >= 0 { "+" } else { "" }, offset);
                text(x + 190, y + 158, MUTED, &zone);
                button(x + w - 190, y + 142, 76, 40, PANEL_RAISED, "- 1 hour");
                button(x + w - 108, y + 142, 74, 40, PANEL_RAISED, "+ 1 hour");
                text(x + 42, y + 208, MUTED, "Choose an offset from UTC-12 through UTC+14.");
                text(x + 42, y + 236, MUTED, "Offsets use whole hours; no country or city names.");
            } else {
                text(x + 42, y + 140, TEXT, "Wallpaper");
                text(x + 190, y + 146, MUTED, wallpaper_name());
                button(x + w - 190, y + 132, 156, 40, PANEL_RAISED, "Change");
                text(x + 42, y + 202, TEXT, "Restore last session");
                let restore_label = if RESTORE_SESSION.load(Ordering::Relaxed) { "On" } else { "Off" };
                button(x + w - 190, y + 194, 156, 40, ACCENT, restore_label);
            }
            if !DISK_MOUNTED.load(Ordering::Relaxed) {
                text(x + 34, y + h - 70, GOLD, "Warning: no writable disk is mounted at /disk.");
                text(x + 34, y + h - 46, MUTED, "Settings changes will not be saved after shutdown.");
            } else {
                match SETTINGS_SAVE_STATUS.load(Ordering::Relaxed) {
                    1 => text(x + 34, y + h - 58, AQUA, "Settings saved to /disk/era-settings."),
                    3 => text(x + 34, y + h - 58, GOLD, "Warning: could not save settings to /disk."),
                    _ => text(x + 34, y + h - 58, MUTED, "Settings will be saved to /disk/era-settings."),
                }
            }
        }
        View::Konsul => {}
        View::AppWindow(index) => draw_app_window_contents(layout, sessions.get(index)),
    }
}

fn draw_app_row(x: i32, y: i32, width: i32, name: &str, description: &str, doom: bool) {
    rect(x, y, width, 48, PANEL_RAISED);
    rect(x, y, width, 1, 0xFFFFFF);
    rect(x, y, 1, 48, 0xFFFFFF);
    rect(x + width - 1, y + 1, 1, 47, 0x404040);
    rect(x + 1, y + 47, width - 2, 1, 0x404040);
    if doom { draw_doom_icon(x + 10, y + 10); }
    else {
        rect(x + 10, y + 10, 28, 28, 0x111111);
        text(x + 14, y + 16, 0x39FF65, ">_");
    }
    text(x + 50, y + 8, AQUA, name);
    text(x + 50, y + 28, MUTED, description);
}

fn draw_app_window_contents(layout: &Layout, session: Option<&AppSession>) {
    let x = layout.window_x + 4;
    let y = layout.window_y + 28;
    let width = layout.window_w - 8;
    let height = layout.window_h - 32;
    rect(x, y, width, height, 0x101010);
    let Some(session) = session else { text(x + 12, y + 12, 0xFFFFFF, "Application has exited."); return; };
    if session.closed { text(x + 12, y + 12, 0xFFFFFF, "Application has exited."); return; }
    let clip = (x + 1, y + 1, width - 2, height - 2);
    for operation in &session.drawing {
        match operation {
            AppDrawOp::Rect(px, py, w, h, color) => {
                let rect_area = (x.saturating_add(*px), y.saturating_add(*py), (*w).min(i32::MAX as u32) as i32, (*h).min(i32::MAX as u32) as i32);
                draw_intersection(clip, rect_area, *color);
            }
            AppDrawOp::Text(px, py, color, value) => {
                let max_chars = ((clip.0 + clip.2 - (x + *px)).max(0) / 8) as usize;
                if *px >= 0 && *py >= 0 && *py + 16 <= height && x + *px < clip.0 + clip.2 {
                    let clipped: String = value.chars().take(max_chars).collect();
                    text(x + *px, y + *py, *color, &clipped);
                }
            }
        }
    }
}

fn draw_taskbar(layout: &Layout, view: Option<View>, minimized: bool) {
    let y = layout.taskbar_y;
    rect(0, y, layout.width, 30, 0xC0C0C0);
    rect(0, y, layout.width, 2, 0xFFFFFF);
    rect(0, y + 2, layout.width, 1, 0x808080);

    // Start button.
    rect(4, y + 4, 94, 22, 0xC0C0C0);
    rect(4, y + 4, 94, 2, 0xFFFFFF);
    rect(4, y + 4, 2, 22, 0xFFFFFF);
    rect(96, y + 6, 2, 20, 0x404040);
    rect(6, y + 24, 92, 2, 0x404040);
    rect(10, y + 8, 16, 14, 0x12377B);
    rect(12, y + 10, 12, 2, 0xFFFFFF);
    rect(12, y + 14, 8, 2, 0xFFFFFF);
    rect(12, y + 18, 12, 2, 0xFFFFFF);
    text(32, y + 7, 0x101010, "EonOS");

    // Each open window gets a taskbar button. The active window is sunken;
    // the other buttons stay raised and can be clicked to focus that window.
    let available = (layout.width - 254).max(1);
    if layout.windows.is_empty() {
        text(112, y + 7, 0x404040, "Desktop");
    } else {
        let tab_width = (available / layout.windows.len() as i32).max(1);
        for (index, window) in layout.windows.iter().enumerate() {
            let task_x = 104 + index as i32 * tab_width;
            let task_w = if index + 1 == layout.windows.len() {
                (layout.width - 150 - task_x).max(1)
            } else { tab_width };
            let focused = Some(*window) == view && !minimized;
            rect(task_x, y + 4, task_w, 22, 0xC0C0C0);
            let (top_left, bottom_right) = if focused { (0x404040, 0xFFFFFF) } else { (0xFFFFFF, 0x404040) };
            rect(task_x, y + 4, task_w, 1, top_left);
            rect(task_x, y + 4, 1, 22, top_left);
            rect(task_x + task_w - 1, y + 5, 1, 21, bottom_right);
            rect(task_x + 1, y + 25, task_w - 2, 1, bottom_right);
            let label: String = taskbar_title(*window).chars().take(((task_w - 14) / 8).max(1) as usize).collect();
            text(task_x + 7, y + 7, 0x101010, &label);
        }
    }

    // Power controls and the UTC real-time clock live in the notification area.
    classic_button(layout.width - 142, y + 4, 66, 22, "Power");
    rect(layout.width - 72, y + 4, 68, 22, 0xC0C0C0);
    rect(layout.width - 72, y + 4, 1, 22, 0x808080);
    let clock = match eonos_rt::wall_time_secs() {
        Some(seconds) => {
            let local_seconds = (seconds as i64 + TIMEZONE_OFFSET.load(Ordering::Relaxed) as i64 * 3600).rem_euclid(86_400) as u64;
            format!("{:02}:{:02}", local_seconds / 3600, (local_seconds / 60) % 60)
        }
        None => String::from("--:--"),
    };
    text(layout.width - 68, y + 7, 0x101010, &clock);
}

fn window_title(view: View) -> &'static str {
    match view {
        View::Welcome => "Era - Welcome",
        View::Files => "Era - File Manager",
        View::TextViewer => "Era - Text Viewer",
        View::System => "Era - System Monitor",
        View::Apps => "Era - Applications",
        View::About => "About Era",
        View::Settings => "Era - Settings",
        View::Konsul => "Konsul - EonOS Shell",
        View::AppWindow(_) => "EonOS Application",
    }
}

fn taskbar_title(view: View) -> &'static str {
    match view {
        View::Welcome => "Welcome",
        View::Files => "Files",
        View::TextViewer => "Text",
        View::System => "System",
        View::Apps => "Apps",
        View::Konsul => "Konsul",
        View::About => "About",
        View::Settings => "Settings",
        View::AppWindow(_) => "App",
    }
}

#[derive(Clone, Copy)]
enum WindowControl { Minimize, Maximize, Close }

fn draw_era_window_icon(x: i32, y: i32) {
    rect(x, y, 14, 14, 0x061A43);
    rect(x + 1, y + 1, 12, 12, 0x477CC8);
    rect(x + 3, y + 3, 8, 2, 0xFFFFFF);
    rect(x + 3, y + 6, 6, 2, 0xFFFFFF);
    rect(x + 3, y + 9, 8, 2, 0xFFFFFF);
}

fn draw_window_control(x: i32, y: i32, width: i32, height: i32, control: WindowControl) {
    rect(x, y, width, height, 0xC0C0C0);
    rect(x, y, width, 1, 0xFFFFFF);
    rect(x, y, 1, height, 0xFFFFFF);
    rect(x + width - 1, y + 1, 1, height - 1, 0x404040);
    rect(x + 1, y + height - 1, width - 1, 1, 0x404040);
    match control {
        WindowControl::Minimize => rect(x + 5, y + 11, 8, 2, 0x202020),
        WindowControl::Maximize => {
            rect(x + 5, y + 4, 9, 2, 0x202020);
            rect(x + 5, y + 4, 2, 9, 0x202020);
            rect(x + 12, y + 4, 2, 9, 0x202020);
            rect(x + 5, y + 11, 9, 2, 0x202020);
        }
        WindowControl::Close => {
            for offset in 0..7 {
                rect(x + 5 + offset, y + 5 + offset, 2, 2, 0x202020);
                rect(x + 11 - offset, y + 5 + offset, 2, 2, 0x202020);
            }
        }
    }
}

fn button(x: i32, y: i32, width: i32, height: i32, _color: u32, label: &str) {
    rect(x, y, width, height, 0xC0C0C0);
    rect(x, y, width, 2, 0xFFFFFF);
    rect(x, y, 2, height, 0xFFFFFF);
    rect(x + width - 2, y + 2, 2, height - 2, 0x404040);
    rect(x + 2, y + height - 2, width - 2, 2, 0x404040);
    let text_x = x + (width - label.len() as i32 * 8) / 2;
    text(text_x, y + (height - 16) / 2, TEXT, label);
}

fn draw_sunken_panel(x: i32, y: i32, width: i32, height: i32) {
    rect(x, y, width, height, 0xFFFFFF);
    rect(x, y, width, 1, 0x404040);
    rect(x, y, 1, height, 0x404040);
    rect(x + 1, y + 1, width - 2, 1, 0x808080);
    rect(x + 1, y + 1, 1, height - 2, 0x808080);
    rect(x + width - 1, y, 1, height, 0xFFFFFF);
    rect(x, y + height - 1, width, 1, 0xFFFFFF);
}

fn draw_doom_icon(x: i32, y: i32) {
    rect(x, y, 28, 28, 0x151923);
    rect(x + 6, y + 3, 16, 5, 0xB94135);
    rect(x + 4, y + 8, 20, 13, 0xD9B68D);
    rect(x + 8, y + 11, 4, 4, 0x26222A);
    rect(x + 17, y + 11, 4, 4, 0x26222A);
    rect(x + 9, y + 18, 11, 3, 0x7E2C29);
    rect(x + 6, y + 22, 16, 3, 0x525B68);
}

fn rect(x: i32, y: i32, width: i32, height: i32, color: u32) {
    let _ = eonos_rt::era::fill_rect(x, y, width.max(0) as u32, height.max(0) as u32, color);
}

fn text(x: i32, y: i32, color: u32, value: &str) {
    let _ = eonos_rt::era::text(x, y, color, value);
}

fn run_terminal(environment: &[&CStr]) {
    static SHELL_ARG0: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"/init\0") };
    match eonos_rt::exec("/init", &[SHELL_ARG0], environment) {
        Ok(pid) => {
            let mut status = 0;
            let _ = eonos_rt::wait4(pid as isize, &mut status, 0);
        }
        Err(error) => eonos_rt::eprintln!("era: could not launch terminal ({})", error.0),
    }
}

fn run_registered_app(app: &RegisteredApp, environment: &[&CStr]) -> u8 {
    let arg0 = unsafe { CStr::from_bytes_with_nul_unchecked(b"/bin/doom\0") };
    match eonos_rt::exec(app.executable, &[arg0], environment) {
        Ok(pid) => {
            let mut status = 0;
            match eonos_rt::wait4(pid as isize, &mut status, 0) {
                Ok(_) if status == 0 => 0,
                Ok(_) => { eonos_rt::eprintln!("era: {} exited with status {}", app.name, status); 2 }
                Err(error) => { eonos_rt::eprintln!("era: wait for {} failed ({})", app.name, error.0); 2 }
            }
        }
        Err(error) => { eonos_rt::eprintln!("era: could not launch {} ({})", app.name, error.0); 1 }
    }
}

eonos_rt::entry!(main);

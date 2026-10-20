#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use eonos_user::{flags, File};

const USAGE: &str = "usage: curler <http://host[:port]/path> [output-file]";
const HEADER_LIMIT: usize = 16 * 1024;
const READ_TIMEOUT_MS: u64 = 15_000;

struct Url<'a> {
    authority: &'a str,
    host: &'a str,
    port: u16,
    path: String,
}

struct Headers {
    status: u16,
    content_length: Option<u64>,
    chunked: bool,
}

struct ChunkDecoder {
    state: u8,
    line: Vec<u8>,
    remaining: usize,
    crlf: u8,
}

impl ChunkDecoder {
    fn new() -> Self { Self { state: 0, line: Vec::new(), remaining: 0, crlf: 0 } }

    fn feed(&mut self, input: &[u8], output: &mut Vec<u8>) -> Result<(), ()> {
        for &byte in input {
            match self.state {
                0 | 3 => {
                    self.line.push(byte);
                    if self.line.len() > 1024 { return Err(()); }
                    if byte == b'\n' {
                        if self.line.len() < 2 || self.line[self.line.len() - 2] != b'\r' { return Err(()); }
                        if self.state == 3 {
                            if self.line.len() == 2 { self.state = 4; }
                            self.line.clear();
                        } else {
                            let end = self.line.len() - 2;
                            let line = &self.line[..end];
                            let size_text = line.split(|b| *b == b';').next().ok_or(())?;
                            let text = core::str::from_utf8(size_text).map_err(|_| ())?;
                            self.remaining = usize::from_str_radix(text.trim(), 16).map_err(|_| ())?;
                            self.line.clear();
                            if self.remaining == 0 { self.state = 3; }
                            else { self.state = 1; }
                        }
                    }
                }
                1 => {
                    output.push(byte);
                    self.remaining -= 1;
                    if self.remaining == 0 { self.state = 2; self.crlf = 0; }
                }
                2 => {
                    let expected = if self.crlf == 0 { b'\r' } else { b'\n' };
                    if byte != expected { return Err(()); }
                    self.crlf += 1;
                    if self.crlf == 2 { self.state = 0; }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn is_done(&self) -> bool { self.state == 4 }
}

fn parse_url(value: &str) -> Result<Url<'_>, ()> {
    if value.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) { return Err(()); }
    let rest = value.strip_prefix("http://").ok_or(())?;
    let authority_end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') || authority.contains('[') { return Err(()); }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, port.parse::<u16>().map_err(|_| ())?),
        _ => (authority, 80),
    };
    if host.is_empty() || port == 0 { return Err(()); }
    let suffix = rest[authority_end..].split('#').next().unwrap_or("");
    let path = if suffix.is_empty() { String::from("/") }
        else if suffix.starts_with('?') { alloc::format!("/{}", suffix) }
        else { suffix.to_string() };
    Ok(Url { authority, host, port, path })
}

fn parse_ipv4(value: &str) -> Option<[u8; 4]> {
    let mut result = [0u8; 4];
    let mut parts = value.split('.');
    for slot in &mut result { *slot = parts.next()?.parse().ok()?; }
    if parts.next().is_some() { return None; }
    Some(result)
}

fn default_filename(path: &str) -> String {
    let path = path.split('?').next().unwrap_or(path);
    let name = path.rsplit('/').find(|part| !part.is_empty()).unwrap_or("index.html");
    if name == "." || name == ".." { return String::from("download.bin"); }
    name.bytes().map(|byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') { byte as char } else { '_' }
    }).collect()
}

fn parse_headers(bytes: &[u8]) -> Result<Headers, ()> {
    let text = core::str::from_utf8(bytes).map_err(|_| ())?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next().ok_or(())?;
    let mut status_parts = status_line.split_ascii_whitespace();
    let version = status_parts.next().ok_or(())?;
    if !version.starts_with("HTTP/") { return Err(()); }
    let status = status_parts.next().ok_or(())?.parse::<u16>().map_err(|_| ())?;
    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        if line.is_empty() { break; }
        let Some((name, value)) = line.split_once(':') else { continue; };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.trim().parse::<u64>().map_err(|_| ())?);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.split(',').any(|part| part.trim().eq_ignore_ascii_case("chunked"));
        }
    }
    Ok(Headers { status, content_length, chunked })
}

fn write_body(file: &mut File, bytes: &[u8], written: &mut u64) -> Result<(), i32> {
    if !bytes.is_empty() {
        file.write_all(bytes).map_err(|error| match error {
            eonos_user::Error::System(system) => system.0,
            _ => 5,
        })?;
        *written = written.saturating_add(bytes.len() as u64);
    }
    Ok(())
}

fn download(url_text: &str, output: &str) -> Result<u64, i32> {
    let url = parse_url(url_text).map_err(|_| 22)?;
    let config = eonos_rt::ipv4_config().map_err(|error| error.0)?;
    if config.configured == 0 { eonos_rt::dhcp().map_err(|error| error.0)?; }
    let address = match parse_ipv4(url.host) {
        Some(address) => address,
        None => eonos_rt::resolve_ipv4(url.host).map_err(|error| error.0)?,
    };
    let mut file = File::open(output, flags::WRITE_ONLY | flags::CREATE | flags::TRUNCATE)
        .map_err(|error| match error { eonos_user::Error::System(system) => system.0, _ => 5 })?;
    eonos_rt::eprintln!("curler: connecting to {}:{}", url.host, url.port);
    if let Err(error) = eonos_rt::tcp_connect(address, url.port) {
        eonos_rt::eprintln!("curler: TCP connect failed (error {})", error.0);
        return Err(error.0);
    }
    let result = (|| -> Result<u64, i32> {
    eonos_rt::eprintln!("curler: sending HTTP request");
    let request = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: curler/0.1\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        url.path, url.authority
    );
    if let Err(error) = eonos_rt::tcp_send(request.as_bytes()) {
        eonos_rt::eprintln!("curler: sending HTTP request failed (error {})", error.0);
        return Err(error.0);
    }

    let mut header_bytes = Vec::new();
    let mut receive = [0u8; 4096];
    let mut body_started = false;
    let mut headers = Headers { status: 0, content_length: None, chunked: false };
    let mut content_received = 0u64;
    let mut written = 0u64;
    let mut decoder = ChunkDecoder::new();
    let mut done = false;
    eonos_rt::eprintln!("curler: waiting for HTTP response");
    while !done {
        let count = match eonos_rt::tcp_receive(&mut receive, READ_TIMEOUT_MS) {
            Ok(count) => count,
            Err(error) => {
                eonos_rt::eprintln!("curler: receiving HTTP response failed (error {})", error.0);
                return Err(error.0);
            }
        };
        if count == 0 {
            if !body_started { return Err(71); }
            if headers.chunked && !decoder.is_done() { return Err(71); }
            if headers.content_length.is_some_and(|length| content_received < length) { return Err(71); }
            break;
        }
        let mut body: &[u8] = &receive[..count];
        if !body_started {
            header_bytes.extend_from_slice(body);
            if header_bytes.len() > HEADER_LIMIT { return Err(71); }
            let Some(marker) = header_bytes.windows(4).position(|window| window == b"\r\n\r\n") else { continue; };
            headers = parse_headers(&header_bytes[..marker + 2]).map_err(|_| 71)?;
            if !(200..300).contains(&headers.status) {
                eonos_rt::eprintln!("curler: server returned HTTP {}", headers.status);
                return Err(0);
            }
            body_started = true;
            body = &header_bytes[marker + 4..];
        }
        if headers.chunked {
            let mut decoded = Vec::new();
            decoder.feed(body, &mut decoded).map_err(|_| 71)?;
            write_body(&mut file, &decoded, &mut written)?;
            if decoder.is_done() { done = true; }
        } else if let Some(length) = headers.content_length {
            let remaining = length.saturating_sub(content_received) as usize;
            let take = body.len().min(remaining);
            write_body(&mut file, &body[..take], &mut written)?;
            content_received = content_received.saturating_add(take as u64);
            if content_received == length { done = true; }
        } else {
            write_body(&mut file, body, &mut written)?;
        }
    }
    Ok(written)
    })();
    let _ = eonos_rt::tcp_close();
    result
}

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut values = args.skip(1);
    let Some(url) = values.next().and_then(|arg| arg.to_str().ok()) else {
        eonos_rt::eprintln!("{}", USAGE);
        return 2;
    };
    let output = match values.next() {
        Some(arg) if values.next().is_none() => match arg.to_str() {
            Ok(path) => path.to_string(),
            Err(_) => { eonos_rt::eprintln!("curler: output path must be UTF-8"); return 2; }
        },
        None => match parse_url(url) {
            Ok(parsed) => default_filename(&parsed.path),
            Err(()) => { eonos_rt::eprintln!("{}", USAGE); return 2; }
        },
        Some(_) => { eonos_rt::eprintln!("{}", USAGE); return 2; }
    };
    match download(url, &output) {
        Ok(bytes) => { eonos_rt::println!("curler: saved {} bytes to {}", bytes, output); 0 }
        Err(0) => 1,
        Err(error) => { eonos_rt::eprintln!("curler: download failed (error {})", error); 1 }
    }
}

eonos_rt::entry!(main);

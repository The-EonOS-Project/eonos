#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Debug;
use eonos_user::{flags, File};
use rustls::client::UnbufferedClientConnection;
use rustls::pki_types::{ServerName, UnixTime};
use rustls::unbuffered::ConnectionState;
use rustls::{ClientConfig, RootCertStore};

const USAGE: &str = "usage: curler <http[s]://host[:port]/path> [output-file]";
const HEADER_LIMIT: usize = 16 * 1024;
const READ_TIMEOUT_MS: u64 = 15_000;
const TLS_BUFFER_LIMIT: usize = 256 * 1024;
const TLS_RECORD_BUFFER: usize = 32 * 1024;

fn eonos_getrandom(buffer: &mut [u8]) -> Result<(), getrandom::Error> {
    eonos_rt::secure_random(buffer).map_err(|_| {
        getrandom::Error::from(core::num::NonZeroU32::new(getrandom::Error::CUSTOM_START).unwrap())
    })
}

getrandom::register_custom_getrandom!(eonos_getrandom);

#[derive(Debug)]
struct EonosTime;

impl rustls::time_provider::TimeProvider for EonosTime {
    fn current_time(&self) -> Option<UnixTime> {
        eonos_rt::wall_time_secs().map(|secs| UnixTime::since_unix_epoch(core::time::Duration::from_secs(secs)))
    }
}

struct Url<'a> {
    authority: &'a str,
    host: &'a str,
    port: u16,
    path: String,
    https: bool,
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
    let (rest, https, default_port) = if let Some(rest) = value.strip_prefix("https://") {
        (rest, true, 443)
    } else if let Some(rest) = value.strip_prefix("http://") {
        (rest, false, 80)
    } else {
        return Err(());
    };
    let authority_end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') || authority.contains('[') { return Err(()); }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, port.parse::<u16>().map_err(|_| ())?),
        _ => (authority, default_port),
    };
    if host.is_empty() || port == 0 { return Err(()); }
    let suffix = rest[authority_end..].split('#').next().unwrap_or("");
    let path = if suffix.is_empty() { String::from("/") }
        else if suffix.starts_with('?') { alloc::format!("/{}", suffix) }
        else { suffix.to_string() };
    Ok(Url { authority, host, port, path, https })
}

trait HttpStream {
    fn send(&mut self, bytes: &[u8]) -> Result<(), i32>;
    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, i32>;
}

struct PlainStream;

impl HttpStream for PlainStream {
    fn send(&mut self, bytes: &[u8]) -> Result<(), i32> {
        eonos_rt::tcp_send(bytes).map(|_| ()).map_err(|error| error.0)
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, i32> {
        eonos_rt::tcp_receive(buffer, READ_TIMEOUT_MS).map_err(|error| error.0)
    }
}

struct TlsStream {
    connection: UnbufferedClientConnection,
    incoming: Vec<u8>,
    plaintext: Vec<u8>,
    plaintext_offset: usize,
    outgoing: Vec<u8>,
    closed: bool,
}

impl TlsStream {
    fn connect(host: &str) -> Result<Self, i32> {
        let provider = rustls::crypto::ring::default_provider();
        let provider = Arc::new(provider);
        let time_provider = Arc::new(EonosTime);
        let root_store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder_with_details(provider, time_provider)
            .with_safe_default_protocol_versions().map_err(|_| 71)?
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let server_name = ServerName::try_from(host.to_string()).map_err(|_| 22)?;
        let connection = UnbufferedClientConnection::new(Arc::new(config), server_name).map_err(|_| 71)?;
        let mut stream = Self {
            connection,
            incoming: Vec::new(),
            plaintext: Vec::new(),
            plaintext_offset: 0,
            outgoing: Vec::new(),
            closed: false,
        };
        stream.drive_handshake()?;
        Ok(stream)
    }

    fn receive_tls(&mut self) -> Result<(), i32> {
        if self.incoming.len() >= TLS_BUFFER_LIMIT { return Err(71); }
        let available = TLS_BUFFER_LIMIT - self.incoming.len();
        let mut buffer = [0u8; 4096];
        let capacity = available.min(buffer.len());
        let count = eonos_rt::tcp_receive(&mut buffer[..capacity], READ_TIMEOUT_MS)
            .map_err(|error| error.0)?;
        if count == 0 { self.closed = true; return Ok(()); }
        self.incoming.extend_from_slice(&buffer[..count]);
        Ok(())
    }

    fn drive_handshake(&mut self) -> Result<(), i32> {
        loop {
            let (discard, need_input, ready) = {
                let status = self.connection.process_tls_records(&mut self.incoming);
                let mut discard = status.discard;
                let mut need_input = false;
                let mut ready = false;
                match status.state.map_err(|error| {
                    eonos_rt::eprintln!("curler: TLS handshake failed: {:?}", error);
                    71
                })? {
                    ConnectionState::EncodeTlsData(mut data) => {
                        self.outgoing.resize(TLS_RECORD_BUFFER, 0);
                        let count = data.encode(&mut self.outgoing).map_err(|_| 71)?;
                        self.outgoing.truncate(count);
                    }
                    ConnectionState::TransmitTlsData(tx) => {
                        if !self.outgoing.is_empty() {
                            eonos_rt::tcp_send(&self.outgoing).map_err(|error| error.0)?;
                            self.outgoing.clear();
                        }
                        tx.done();
                    }
                    ConnectionState::BlockedHandshake => need_input = true,
                    ConnectionState::WriteTraffic(_) => ready = true,
                    ConnectionState::ReadTraffic(mut traffic) => {
                        while let Some(record) = traffic.next_record() {
                            let record = record.map_err(|_| 71)?;
                            self.plaintext.extend_from_slice(record.payload);
                            discard = discard.saturating_add(record.discard);
                        }
                    }
                    ConnectionState::PeerClosed | ConnectionState::Closed => { self.closed = true; return Err(71); }
                    _ => return Err(71),
                }
                (discard, need_input, ready)
            };
            if discard > 0 { self.incoming.drain(..discard.min(self.incoming.len())); }
            if ready { return Ok(()); }
            if need_input {
                self.receive_tls()?;
                if self.closed { return Err(71); }
            }
        }
    }
}

impl HttpStream for TlsStream {
    fn send(&mut self, bytes: &[u8]) -> Result<(), i32> {
        let mut offset = 0;
        while offset < bytes.len() {
            let (discard, need_input, sent) = {
                let status = self.connection.process_tls_records(&mut self.incoming);
                let mut discard = status.discard;
                let mut need_input = false;
                let mut sent = false;
                match status.state.map_err(|error| {
                    eonos_rt::eprintln!("curler: TLS write failed: {:?}", error);
                    71
                })? {
                    ConnectionState::WriteTraffic(mut traffic) => {
                        self.outgoing.resize(TLS_RECORD_BUFFER, 0);
                        let plain_count = (bytes.len() - offset).min(16 * 1024);
                        let count = traffic.encrypt(&bytes[offset..offset + plain_count], &mut self.outgoing).map_err(|_| 71)?;
                        self.outgoing.truncate(count);
                        eonos_rt::tcp_send(&self.outgoing).map_err(|error| error.0)?;
                        offset += plain_count;
                        self.outgoing.clear();
                        sent = true;
                    }
                    ConnectionState::EncodeTlsData(mut data) => {
                        self.outgoing.resize(TLS_RECORD_BUFFER, 0);
                        let count = data.encode(&mut self.outgoing).map_err(|_| 71)?;
                        self.outgoing.truncate(count);
                    }
                    ConnectionState::TransmitTlsData(tx) => {
                        if !self.outgoing.is_empty() {
                            eonos_rt::tcp_send(&self.outgoing).map_err(|error| error.0)?;
                            self.outgoing.clear();
                        }
                        tx.done();
                    }
                    ConnectionState::BlockedHandshake => need_input = true,
                    ConnectionState::ReadTraffic(mut traffic) => {
                        while let Some(record) = traffic.next_record() {
                            let record = record.map_err(|_| 71)?;
                            self.plaintext.extend_from_slice(record.payload);
                            discard = discard.saturating_add(record.discard);
                        }
                    }
                    ConnectionState::PeerClosed | ConnectionState::Closed => { self.closed = true; return Err(71); }
                    _ => return Err(71),
                }
                (discard, need_input, sent)
            };
            if discard > 0 { self.incoming.drain(..discard.min(self.incoming.len())); }
            if sent { continue; }
            if need_input {
                self.receive_tls()?;
                if self.closed { return Err(71); }
            }
        }
        Ok(())
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, i32> {
        if self.plaintext_offset < self.plaintext.len() {
            let count = buffer.len().min(self.plaintext.len() - self.plaintext_offset);
            buffer[..count].copy_from_slice(&self.plaintext[self.plaintext_offset..self.plaintext_offset + count]);
            self.plaintext_offset += count;
            if self.plaintext_offset == self.plaintext.len() { self.plaintext.clear(); self.plaintext_offset = 0; }
            return Ok(count);
        }
        if self.closed { return Ok(0); }
        loop {
            let (discard, need_input, closed) = {
                let status = self.connection.process_tls_records(&mut self.incoming);
                let mut discard = status.discard;
                let mut need_input = false;
                let mut closed = false;
                match status.state.map_err(|error| {
                    eonos_rt::eprintln!("curler: TLS read failed: {:?}", error);
                    71
                })? {
                    ConnectionState::ReadTraffic(mut traffic) => {
                        while let Some(record) = traffic.next_record() {
                            let record = record.map_err(|_| 71)?;
                            self.plaintext.extend_from_slice(record.payload);
                            discard = discard.saturating_add(record.discard);
                        }
                    }
                    ConnectionState::EncodeTlsData(mut data) => {
                        self.outgoing.resize(TLS_RECORD_BUFFER, 0);
                        let count = data.encode(&mut self.outgoing).map_err(|_| 71)?;
                        self.outgoing.truncate(count);
                    }
                    ConnectionState::TransmitTlsData(tx) => {
                        if !self.outgoing.is_empty() {
                            eonos_rt::tcp_send(&self.outgoing).map_err(|error| error.0)?;
                            self.outgoing.clear();
                        }
                        tx.done();
                    }
                    ConnectionState::WriteTraffic(_) | ConnectionState::BlockedHandshake => need_input = true,
                    ConnectionState::PeerClosed | ConnectionState::Closed => { closed = true; }
                    _ => return Err(71),
                }
                (discard, need_input, closed)
            };
            if discard > 0 { self.incoming.drain(..discard.min(self.incoming.len())); }
            if !self.plaintext.is_empty() {
                let count = buffer.len().min(self.plaintext.len());
                buffer[..count].copy_from_slice(&self.plaintext[..count]);
                if count == self.plaintext.len() { self.plaintext.clear(); self.plaintext_offset = 0; }
                else { self.plaintext_offset = count; }
                return Ok(count);
            }
            if closed { self.closed = true; return Ok(0); }
            if need_input {
                self.receive_tls()?;
                if self.closed { return Ok(0); }
            }
        }
    }
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

fn receive_response(stream: &mut impl HttpStream, file: &mut File, url: &Url<'_>) -> Result<u64, i32> {
    eonos_rt::eprintln!("curler: sending HTTP request");
    let request = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: curler/0.1\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        url.path, url.authority
    );
    stream.send(request.as_bytes()).map_err(|error| {
        eonos_rt::eprintln!("curler: sending HTTP request failed (error {})", error);
        error
    })?;

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
        let count = stream.receive(&mut receive).map_err(|error| {
            eonos_rt::eprintln!("curler: receiving HTTP response failed (error {})", error);
            error
        })?;
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
            write_body(file, &decoded, &mut written)?;
            if decoder.is_done() { done = true; }
        } else if let Some(length) = headers.content_length {
            let remaining = length.saturating_sub(content_received) as usize;
            let take = body.len().min(remaining);
            write_body(file, &body[..take], &mut written)?;
            content_received = content_received.saturating_add(take as u64);
            if content_received == length { done = true; }
        } else {
            write_body(file, body, &mut written)?;
        }
    }
    Ok(written)
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
        if url.https {
            eonos_rt::eprintln!("curler: starting verified TLS connection");
            let mut stream = TlsStream::connect(url.host)?;
            receive_response(&mut stream, &mut file, &url)
        } else {
            let mut stream = PlainStream;
            receive_response(&mut stream, &mut file, &url)
        }
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

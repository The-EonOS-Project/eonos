use alloc::string::String;
use alloc::vec::Vec;
use eonos_rt::{Error, Result};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_LINKS: usize = 256;
const MAX_URL_BYTES: usize = 2048;
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    pub href: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    pub title: String,
    pub text: String,
    pub links: Vec<Link>,
}

pub fn fetch(url: &str) -> Result<Vec<u8>> {
    if url.len() > MAX_URL_BYTES
        || !(url.starts_with("https://") || url.starts_with("http://"))
        || url.bytes().any(|byte| byte <= 0x20 || byte == 0x7f)
    {
        return Err(Error(22));
    }
    let (read_fd, write_fd) = eonos_rt::pipe()?;
    let pid = match eonos_rt::start_web_fetch(url, write_fd) {
        Ok(pid) => pid,
        Err(error) => {
            let _ = eonos_rt::close(read_fd);
            let _ = eonos_rt::close(write_fd);
            return Err(error);
        }
    };
    eonos_rt::close(write_fd)?;

    let mut body = Vec::new();
    let mut overflow = false;
    let mut buffer = [0u8; 4096];
    let read_result = loop {
        match eonos_rt::read(read_fd, &mut buffer) {
            Ok(0) => break Ok(()),
            Ok(count) => {
                if !overflow && body.len().checked_add(count).is_some_and(|size| size <= MAX_RESPONSE_BYTES) {
                    body.extend_from_slice(&buffer[..count]);
                } else {
                    overflow = true;
                }
            }
            Err(error) => break Err(error),
        }
    };
    let close_result = eonos_rt::close(read_fd);
    let mut status = 0;
    let wait_result = eonos_rt::wait4(pid as isize, &mut status, 0);
    read_result?;
    close_result?;
    wait_result?;
    if overflow { return Err(Error(27)); }
    if status != 0 { return Err(Error(71)); }
    Ok(body)
}

pub fn fetch_html(url: &str) -> Result<Page> {
    let body = fetch(url)?;
    let source = core::str::from_utf8(&body).map_err(|_| Error(84))?;
    parse_html(source)
}

pub fn parse_html(source: &str) -> Result<Page> {
    if source.len() > MAX_SOURCE_BYTES { return Err(Error(7)); }
    let mut page = Page { title: String::new(), text: String::new(), links: Vec::new() };
    let mut offset = 0;
    let mut in_title = false;
    let mut in_head = false;
    let mut skipped_element: Option<String> = None;
    let mut active_link: Option<(String, usize)> = None;

    while offset < source.len() {
        let Some(relative_start) = source[offset..].find('<') else {
            if skipped_element.is_none() {
                append_text(&mut page.text, &source[offset..])?;
                if in_title { append_text(&mut page.title, &source[offset..])?; }
            }
            break;
        };
        let tag_start = offset + relative_start;
        if tag_start > offset && skipped_element.is_none() {
            let segment = &source[offset..tag_start];
            if !in_head { append_text(&mut page.text, segment)?; }
            if in_title { append_text(&mut page.title, segment)?; }
        }

        if source[tag_start..].starts_with("<!--") {
            if let Some(end) = source[tag_start + 4..].find("-->") {
                offset = tag_start + 4 + end + 3;
            } else {
                break;
            }
            continue;
        }
        let Some(relative_end) = source[tag_start..].find('>') else { break; };
        let tag_end = tag_start + relative_end;
        let raw_tag = &source[tag_start + 1..tag_end];
        let (closing, name) = tag_name(raw_tag);
        offset = tag_end + 1;
        if name.is_empty() { continue; }

        if let Some(skipped) = skipped_element.as_deref() {
            if closing && name.eq_ignore_ascii_case(skipped) { skipped_element = None; }
            continue;
        }
        if !closing && (name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style")) {
            skipped_element = Some(name.to_ascii_lowercase());
            continue;
        }
        if name.eq_ignore_ascii_case("head") {
            in_head = !closing;
            continue;
        }
        if name.eq_ignore_ascii_case("title") {
            in_title = !closing;
            continue;
        }
        if in_head { continue; }

        if name.eq_ignore_ascii_case("a") {
            if closing {
                if let Some((href, start)) = active_link.take() {
                    let mut end = page.text.len();
                    while end > start && page.text.as_bytes()[end - 1].is_ascii_whitespace() { end -= 1; }
                    let mut start = start;
                    while start < end && page.text.as_bytes()[start].is_ascii_whitespace() { start += 1; }
                    if start < end && page.links.len() < MAX_LINKS {
                        page.links.push(Link { href, start, end });
                    }
                }
            } else if let Some(href) = attribute(raw_tag, "href") {
                if href.len() <= MAX_URL_BYTES { active_link = Some((href, page.text.len())); }
            }
        } else if !closing && name.eq_ignore_ascii_case("img") {
            if let Some(alt) = attribute(raw_tag, "alt") { append_text(&mut page.text, &alt)?; }
        }

        if is_block_element(&name) { append_line_break(&mut page.text); }
    }

    let removed = trim_text(&mut page.text);
    trim_text(&mut page.title);
    for link in &mut page.links {
        link.start = link.start.saturating_sub(removed).min(page.text.len());
        link.end = link.end.saturating_sub(removed).min(page.text.len());
    }
    if page.title.is_empty() { page.title = String::from("Untitled"); }
    for link in &mut page.links {
        link.start = link.start.min(page.text.len());
        link.end = link.end.min(page.text.len());
    }
    Ok(page)
}

fn tag_name(tag: &str) -> (bool, String) {
    let mut value = tag.trim_start();
    let closing = value.starts_with('/');
    if closing { value = value[1..].trim_start(); }
    let end = value.find(|character: char| character.is_ascii_whitespace() || character == '/')
        .unwrap_or(value.len());
    (closing, value[..end].to_ascii_lowercase())
}

fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let mut rest = tag.trim_start();
    if rest.starts_with('/') { rest = rest[1..].trim_start(); }
    let name_end = rest.find(|character: char| character.is_ascii_whitespace() || character == '/')
        .unwrap_or(rest.len());
    rest = &rest[name_end..];
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('/') { break; }
        let key_end = rest.find(|character: char| character.is_ascii_whitespace() || character == '=' || character == '/')
            .unwrap_or(rest.len());
        if key_end == 0 { rest = &rest[1..]; continue; }
        let key = &rest[..key_end];
        rest = &rest[key_end..].trim_start();
        if !rest.starts_with('=') { continue; }
        rest = &rest[1..].trim_start();
        let (value, consumed) = if rest.starts_with('"') || rest.starts_with('\'') {
            let quote = rest.as_bytes()[0] as char;
            match rest[1..].find(quote) {
                Some(end) => (&rest[1..1 + end], end + 2),
                None => return None,
            }
        } else {
            let end = rest.find(|character: char| character.is_ascii_whitespace() || character == '>')
                .unwrap_or(rest.len());
            (&rest[..end], end)
        };
        if key.eq_ignore_ascii_case(wanted) { return Some(decode_entities(value)); }
        rest = &rest[consumed..];
    }
    None
}

fn append_text(output: &mut String, input: &str) -> Result<()> {
    let mut cursor = 0;
    while cursor < input.len() {
        let Some(relative_amp) = input[cursor..].find('&') else {
            append_normalized(output, &input[cursor..])?;
            break;
        };
        let amp = cursor + relative_amp;
        append_normalized(output, &input[cursor..amp])?;
        let entity_start = amp + 1;
        let Some(relative_end) = input[entity_start..].find(';') else {
            append_normalized(output, "&")?;
            cursor = entity_start;
            continue;
        };
        let end = entity_start + relative_end;
        if end - entity_start > 12 {
            append_normalized(output, "&")?;
            cursor = entity_start;
            continue;
        }
        let entity = &input[entity_start..end];
        if let Some(character) = decode_entity(entity) {
            let mut encoded = [0; 4];
            append_normalized(output, character.encode_utf8(&mut encoded))?;
            cursor = end + 1;
        } else {
            append_normalized(output, &input[amp..=end])?;
            cursor = end + 1;
        }
    }
    Ok(())
}

fn append_normalized(output: &mut String, input: &str) -> Result<()> {
    for character in input.chars() {
        if character.is_whitespace() {
            if !output.is_empty() && !output.ends_with([' ', '\n']) { output.push(' '); }
        } else {
            output.push(character);
        }
        if output.len() > MAX_TEXT_BYTES { return Err(Error(7)); }
    }
    Ok(())
}

fn append_line_break(output: &mut String) {
    while output.ends_with(' ') { output.pop(); }
    if !output.is_empty() && !output.ends_with('\n') { output.push('\n'); }
}

fn trim_text(value: &mut String) -> usize {
    while value.ends_with(' ') || value.ends_with('\n') { value.pop(); }
    let start = value.bytes().position(|byte| !byte.is_ascii_whitespace()).unwrap_or(value.len());
    if start != 0 { value.drain(..start); }
    start
}

fn decode_entities(input: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    while cursor < input.len() {
        if input.as_bytes()[cursor] == b'&' {
            if let Some(end) = input[cursor + 1..].find(';') {
                let end = cursor + 1 + end;
                if let Some(character) = decode_entity(&input[cursor + 1..end]) {
                    output.push(character);
                    cursor = end + 1;
                    continue;
                }
            }
        }
        let character = input[cursor..].chars().next().unwrap();
        output.push(character);
        cursor += character.len_utf8();
    }
    output
}

fn decode_entity(value: &str) -> Option<char> {
    match value {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ if value.starts_with("#x") || value.starts_with("#X") => {
            u32::from_str_radix(&value[2..], 16).ok().and_then(char::from_u32)
        }
        _ if value.starts_with('#') => value[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

fn is_block_element(name: &str) -> bool {
    matches!(name,
        "address" | "article" | "blockquote" | "br" | "dd" | "div" | "dl" | "dt"
        | "fieldset" | "figcaption" | "figure" | "footer" | "form" | "h1" | "h2"
        | "h3" | "h4" | "h5" | "h6" | "header" | "hr" | "li" | "main" | "nav"
        | "ol" | "p" | "pre" | "section" | "table" | "tr" | "ul")
}

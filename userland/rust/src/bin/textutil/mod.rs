#![allow(dead_code)]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

enum Input {
    Stdin,
    File(eonos_user::File),
}

impl Input {
    fn open(path: Option<&str>) -> Result<Self, eonos_rt::Error> {
        match path {
            Some(path) => eonos_user::File::open(path, eonos_user::flags::READ_ONLY)
                .map(Self::File)
                .map_err(|_| eonos_rt::Error(2)),
            None => Ok(Self::Stdin),
        }
    }

    fn read(&mut self, bytes: &mut [u8]) -> Result<usize, eonos_rt::Error> {
        match self {
            Self::Stdin => eonos_rt::read(0, bytes),
            Self::File(file) => file.read(bytes).map_err(|_| eonos_rt::Error(5)),
        }
    }
}

fn arguments(args: eonos_rt::Args<'_>) -> Result<Vec<String>, ()> {
    args.skip(1)
        .map(|arg| arg.to_str().map(ToString::to_string).map_err(|_| ()))
        .collect()
}

fn report_error(tool: &str, error: eonos_rt::Error) -> i32 {
    let message = match error.0 {
        2 => "no such file or directory",
        7 => "line exceeds the supported size",
        13 => "permission denied",
        _ => "I/O error",
    };
    eonos_rt::eprintln!("{}: {}", tool, message);
    1
}

pub fn head(args: eonos_rt::Args<'_>) -> i32 {
    let args = match arguments(args) {
        Ok(args) => args,
        Err(()) => { eonos_rt::eprintln!("head: arguments must be UTF-8"); return 2; }
    };
    let (lines, path_index) = if args.first().is_some_and(|arg| arg == "-n") {
        let Some(value) = args.get(1) else { eonos_rt::eprintln!("usage: head [-n lines] [file]"); return 2; };
        match value.parse::<usize>() {
            Ok(lines) => (lines, 2),
            Err(_) => { eonos_rt::eprintln!("head: invalid line count"); return 2; }
        }
    } else { (10, 0) };
    if args.len().saturating_sub(path_index) > 1 {
        eonos_rt::eprintln!("usage: head [-n lines] [file]");
        return 2;
    }
    let mut input = match Input::open(args.get(path_index).map(String::as_str)) {
        Ok(input) => input,
        Err(error) => return report_error("head", error),
    };
    if lines == 0 { return 0; }
    let mut remaining = lines;
    let mut buffer = [0u8; 512];
    loop {
        let count = match input.read(&mut buffer) {
            Ok(count) => count,
            Err(error) => return report_error("head", error),
        };
        if count == 0 { return 0; }
        let mut end = count;
        for (index, byte) in buffer[..count].iter().enumerate() {
            if *byte == b'\n' {
                remaining -= 1;
                if remaining == 0 { end = index + 1; break; }
            }
        }
        if eonos_rt::write_all(1, &buffer[..end]).is_err() { return 1; }
        if remaining == 0 { return 0; }
    }
}

pub fn wc(args: eonos_rt::Args<'_>) -> i32 {
    let paths = match arguments(args) {
        Ok(paths) => paths,
        Err(()) => { eonos_rt::eprintln!("wc: arguments must be UTF-8"); return 2; }
    };
    if paths.is_empty() {
        let mut input = Input::Stdin;
        return match count(&mut input) {
            Ok((lines, words, bytes)) => { eonos_rt::println!("{:>6} {:>6} {:>8}", lines, words, bytes); 0 }
            Err(error) => report_error("wc", error),
        };
    }
    let mut total = (0usize, 0usize, 0usize);
    for path in &paths {
        let mut input = match Input::open(Some(path)) {
            Ok(input) => input,
            Err(error) => { eonos_rt::eprintln!("wc: {}: I/O error ({})", path, error.0); return 1; }
        };
        match count(&mut input) {
            Ok((lines, words, bytes)) => {
                total.0 += lines; total.1 += words; total.2 += bytes;
                eonos_rt::println!("{:>6} {:>6} {:>8} {}", lines, words, bytes, path);
            }
            Err(error) => return report_error("wc", error),
        }
    }
    if paths.len() > 1 {
        eonos_rt::println!("{:>6} {:>6} {:>8} total", total.0, total.1, total.2);
    }
    0
}

fn count(input: &mut Input) -> Result<(usize, usize, usize), eonos_rt::Error> {
    let (mut lines, mut words, mut bytes_read) = (0usize, 0usize, 0usize);
    let mut in_word = false;
    let mut buffer = [0u8; 512];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 { return Ok((lines, words, bytes_read)); }
        bytes_read += count;
        for byte in &buffer[..count] {
            if *byte == b'\n' { lines += 1; }
            if byte.is_ascii_whitespace() { in_word = false; }
            else if !in_word { words += 1; in_word = true; }
        }
    }
}

pub fn grep(args: eonos_rt::Args<'_>) -> i32 {
    let args = match arguments(args) {
        Ok(args) => args,
        Err(()) => { eonos_rt::eprintln!("grep: arguments must be UTF-8"); return 2; }
    };
    let mut index = 0;
    let mut show_line = false;
    let mut ignore_case = false;
    while let Some(option) = args.get(index) {
        if option == "-n" { show_line = true; }
        else if option == "-i" { ignore_case = true; }
        else { break; }
        index += 1;
    }
    let Some(pattern) = args.get(index) else {
        eonos_rt::eprintln!("usage: grep [-n] [-i] <pattern> [file...]");
        return 2;
    };
    let paths = &args[index + 1..];
    if paths.is_empty() {
        let mut input = Input::Stdin;
        return match grep_input(&mut input, None, pattern.as_bytes(), show_line, ignore_case) {
            Ok(true) => 0,
            Ok(false) => 1,
            Err(error) => report_error("grep", error),
        };
    }
    let mut found = false;
    for path in paths {
        let mut input = match Input::open(Some(path)) {
            Ok(input) => input,
            Err(error) => { eonos_rt::eprintln!("grep: {}: I/O error ({})", path, error.0); return 1; }
        };
        let label = if paths.len() > 1 { Some(path.as_str()) } else { None };
        match grep_input(&mut input, label, pattern.as_bytes(), show_line, ignore_case) {
            Ok(matched) => found |= matched,
            Err(error) => return report_error("grep", error),
        }
    }
    if found { 0 } else { 1 }
}

fn grep_input(
    input: &mut Input,
    label: Option<&str>,
    pattern: &[u8],
    show_line: bool,
    ignore_case: bool,
) -> Result<bool, eonos_rt::Error> {
    const MAX_LINE: usize = 64 * 1024;
    let mut line = Vec::new();
    let mut number = 1usize;
    let mut found = false;
    let mut buffer = [0u8; 512];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            if !line.is_empty() { found |= print_match(&line, number, label, pattern, show_line, ignore_case)?; }
            return Ok(found);
        }
        for byte in &buffer[..count] {
            line.push(*byte);
            if line.len() > MAX_LINE { return Err(eonos_rt::Error(7)); }
            if *byte == b'\n' {
                found |= print_match(&line, number, label, pattern, show_line, ignore_case)?;
                line.clear();
                number += 1;
            }
        }
    }
}

fn print_match(
    line: &[u8], number: usize, label: Option<&str>, pattern: &[u8], show_line: bool, ignore_case: bool,
) -> Result<bool, eonos_rt::Error> {
    if !contains(line, pattern, ignore_case) { return Ok(false); }
    let mut prefix = Vec::new();
    if let Some(label) = label { prefix.extend_from_slice(label.as_bytes()); prefix.push(b':'); }
    if show_line { prefix.extend_from_slice(number.to_string().as_bytes()); prefix.push(b':'); }
    eonos_rt::write_all(1, &prefix)?;
    eonos_rt::write_all(1, line)?;
    if line.last() != Some(&b'\n') { eonos_rt::write_all(1, b"\n")?; }
    Ok(true)
}

fn contains(line: &[u8], pattern: &[u8], ignore_case: bool) -> bool {
    if pattern.is_empty() { return true; }
    line.windows(pattern.len()).any(|window| window.iter().zip(pattern).all(|(left, right)| {
        if ignore_case { left.eq_ignore_ascii_case(right) } else { left == right }
    }))
}

pub fn tail(args: eonos_rt::Args<'_>) -> i32 {
    let args = match arguments(args) {
        Ok(args) => args,
        Err(()) => { eonos_rt::eprintln!("tail: arguments must be UTF-8"); return 2; }
    };
    let (lines, path_index) = if args.first().is_some_and(|arg| arg == "-n") {
        let Some(value) = args.get(1) else { eonos_rt::eprintln!("usage: tail [-n lines] [file]"); return 2; };
        match value.parse::<usize>() {
            Ok(lines) if lines <= 1000 => (lines, 2),
            _ => { eonos_rt::eprintln!("tail: line count must be between 0 and 1000"); return 2; }
        }
    } else { (10, 0) };
    if args.len().saturating_sub(path_index) > 1 {
        eonos_rt::eprintln!("usage: tail [-n lines] [file]");
        return 2;
    }
    let mut input = match Input::open(args.get(path_index).map(String::as_str)) {
        Ok(input) => input,
        Err(error) => return report_error("tail", error),
    };
    match tail_input(&mut input, lines) {
        Ok(()) => 0,
        Err(error) => report_error("tail", error),
    }
}

fn tail_input(input: &mut Input, count: usize) -> Result<(), eonos_rt::Error> {
    const MAX_LINE: usize = 16 * 1024;
    if count == 0 { return Ok(()); }
    let mut lines: VecDeque<Vec<u8>> = VecDeque::new();
    let mut line = Vec::new();
    let mut buffer = [0u8; 512];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 { break; }
        for byte in &buffer[..read] {
            line.push(*byte);
            if line.len() > MAX_LINE { return Err(eonos_rt::Error(7)); }
            if *byte == b'\n' {
                if count > 0 {
                    if lines.len() == count { lines.pop_front(); }
                    lines.push_back(core::mem::take(&mut line));
                } else { line.clear(); }
            }
        }
    }
    if !line.is_empty() && count > 0 {
        if lines.len() == count { lines.pop_front(); }
        lines.push_back(line);
    }
    for line in lines { eonos_rt::write_all(1, &line)?; }
    Ok(())
}

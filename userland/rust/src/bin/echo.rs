#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut values = args;
    let _program = values.next();
    let mut first = true;
    let mut newline = true;
    let mut pending = alloc::vec::Vec::new();
    for arg in values {
        if first && arg.to_bytes() == b"-n" {
            newline = false;
            first = false;
            continue;
        }
        first = false;
        if !pending.is_empty() {
            pending.push(b' ');
        }
        pending.extend_from_slice(arg.to_bytes());
    }
    if newline {
        pending.push(b'\n');
    }
    match eonos_rt::write_all(1, &pending) {
        Ok(()) => 0,
        Err(error) => {
            eonos_rt::eprintln!("echo: write failed: {}", error.0);
            1
        }
    }
}

extern crate alloc;
eonos_rt::entry!(main);

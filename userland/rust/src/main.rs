#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut args = args;
    let program = args
        .next()
        .and_then(|arg| arg.to_str().ok())
        .unwrap_or("rust program");
    eonos_rt::println!("hello from Rust ({})", program);

    for arg in args {
        match arg.to_str() {
            Ok(value) => eonos_rt::println!("arg: {}", value),
            Err(_) => eonos_rt::println!("arg: <non-UTF-8>"),
        }
    }

    let mut values = Vec::new();
    values.push(String::from("dynamic Rust memory works"));
    values.push(String::from("Vec + String"));
    eonos_rt::println!("heap: {} values; {}", values.len(), values[0]);

    eonos_rt::println!("pid: {}", eonos_rt::getpid());
    0
}

eonos_rt::entry!(main);

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let environment: alloc::vec::Vec<alloc::string::String> = args
        .envs()
        .filter_map(|entry| entry.to_str().ok().map(alloc::string::String::from))
        .collect();
    let mut values = args;
    let _program = values.next();
    let requested: alloc::vec::Vec<alloc::string::String> = values
        .filter_map(|value| value.to_str().ok().map(alloc::string::String::from))
        .collect();
    if requested.is_empty() {
        for entry in environment {
            eonos_rt::println!("{}", entry);
        }
        return 0;
    }
    for name in requested {
        let found = environment.iter().find(|entry| {
            entry.split_once('=').is_some_and(|(key, _)| key == name)
        });
        if let Some(value) = found {
            eonos_rt::println!("{}", value);
        } else {
            eonos_rt::eprintln!("env: {}: not set", name);
            return 1;
        }
    }
    0
}

extern crate alloc;
eonos_rt::entry!(main);

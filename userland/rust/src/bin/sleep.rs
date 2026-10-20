#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut values = args;
    let _program = values.next();
    let milliseconds = match values.next().and_then(|value| value.to_str().ok()) {
        Some(value) => match value.parse::<usize>() {
            Ok(milliseconds) => milliseconds,
            Err(_) => {
                eonos_rt::eprintln!("sleep: expected milliseconds");
                return 2;
            }
        },
        None => {
            eonos_rt::eprintln!("usage: sleep <milliseconds>");
            return 2;
        }
    };
    match eonos_rt::sleep_ms(milliseconds) {
        Ok(()) => 0,
        Err(error) => {
            eonos_rt::eprintln!("sleep: error {}", error.0);
            1
        }
    }
}

eonos_rt::entry!(main);

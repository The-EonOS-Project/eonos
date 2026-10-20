#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(_: eonos_rt::Args<'_>) -> i32 {
    match eonos_user::current_dir() {
        Ok(path) => {
            eonos_rt::println!("{}", path);
            0
        }
        Err(error) => {
            eonos_rt::eprintln!("pwd: {}", error);
            1
        }
    }
}

eonos_rt::entry!(main);

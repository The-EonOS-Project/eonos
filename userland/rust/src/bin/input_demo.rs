#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(_: eonos_rt::Args<'_>) -> i32 {
    let _ = eonos_rt::write_all(1, b"input demo> ");
    match eonos_user::read_line(0) {
        Ok(Some(line)) => {
            eonos_rt::println!("read: {}", line);
            0
        }
        Ok(None) => {
            eonos_rt::println!("input: end of file");
            0
        }
        Err(error) => {
            eonos_rt::eprintln!("input failed: {}", error);
            1
        }
    }
}

eonos_rt::entry!(main);

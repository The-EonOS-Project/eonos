#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut values = args;
    let _program = values.next();
    let path = match values.next() {
        Some(_) if values.next().is_some() => {
            eonos_rt::eprintln!("usage: ls [directory]");
            return 2;
        }
        Some(path) => path.to_str().unwrap_or(""),
        None => ".",
    };
    match eonos_user::list_directory(path) {
        Ok(files) => {
            eonos_rt::println!("{}", files);
            0
        }
        Err(error) => {
            eonos_rt::eprintln!("ls: {}", error);
            1
        }
    }
}

eonos_rt::entry!(main);

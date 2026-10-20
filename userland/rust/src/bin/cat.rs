#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use core::ffi::CStr;

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut values = args;
    let _program = values.next();
    let paths: alloc::vec::Vec<&CStr> = values.collect();
    if paths.is_empty() {
        return copy_stdin();
    }
    for path in paths {
        let Ok(path) = path.to_str() else {
            eonos_rt::eprintln!("cat: path is not UTF-8");
            return 1;
        };
        let mut file = match eonos_user::File::open(path, eonos_user::flags::READ_ONLY) {
            Ok(file) => file,
            Err(error) => {
                eonos_rt::eprintln!("cat: {}: {}", path, error);
                return 1;
            }
        };
        if copy_file(&mut file).is_err() {
            eonos_rt::eprintln!("cat: {}: read failed", path);
            return 1;
        }
    }
    0
}

fn copy_stdin() -> i32 {
    let mut buffer = [0u8; 512];
    loop {
        match eonos_rt::read(0, &mut buffer) {
            Ok(0) => return 0,
            Ok(count) => {
                if eonos_rt::write_all(1, &buffer[..count]).is_err() {
                    return 1;
                }
            }
            Err(_) => return 1,
        }
    }
}

fn copy_file(file: &mut eonos_user::File) -> Result<(), eonos_user::Error> {
    let mut buffer = [0u8; 512];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        eonos_rt::write_all(1, &buffer[..count]).map_err(eonos_user::Error::from)?;
    }
}

extern crate alloc;
eonos_rt::entry!(main);

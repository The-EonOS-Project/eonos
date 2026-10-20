#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

fn main(_: eonos_rt::Args<'_>) -> i32 {
    let path = "/disk/rust-lib-demo.txt";
    let result = (|| -> Result<alloc::string::String, eonos_user::Error> {
        let mut output = eonos_user::File::open(
            path,
            eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
        )?;
        output.write_all(b"Rust file API works")?;
        output.close()?;

        let mut input = eonos_user::File::open(path, eonos_user::flags::READ_ONLY)?;
        input.read_to_string()
    })();

    match result {
        Ok(contents) => {
            eonos_rt::println!("file demo: {}", contents);
            0
        }
        Err(error) => {
            eonos_rt::eprintln!("file demo failed: {}", error);
            1
        }
    }
}

eonos_rt::entry!(main);

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(_: eonos_rt::Args<'_>) -> i32 {
    match eonos_user::Process::spawn("/hello", &[], &[]) {
        Ok(child) => {
            eonos_rt::println!("process demo: spawned pid {}", child.pid());
            match child.wait() {
                Ok(status) => {
                    eonos_rt::println!("process demo: pid {} exited {}", status.pid, status.code);
                    0
                }
                Err(error) => {
                    eonos_rt::eprintln!("process wait failed: {}", error);
                    1
                }
            }
        }
        Err(error) => {
            eonos_rt::eprintln!("process spawn failed: {}", error);
            1
        }
    }
}

eonos_rt::entry!(main);

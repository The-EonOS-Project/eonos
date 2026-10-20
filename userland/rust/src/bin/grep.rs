#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

mod textutil;

fn main(args: eonos_rt::Args<'_>) -> i32 { textutil::grep(args) }

eonos_rt::entry!(main);

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(_args: eonos_app::Args<'_>) -> i32 {
    let Ok(mut window) = eonos_app::era::Window::open("My EonOS app", 480, 300) else { return 1; };
    let _ = window.fill_rect(0, 0, 480, 300, 0x202A44);
    let _ = window.fill_rect(24, 24, 432, 252, 0xF0F0F0);
    let _ = window.text(48, 52, 0x12377B, "Hello from an Era app!");
    let _ = window.present();
    loop {
        match window.poll_event() {
            Ok(Some(eonos_app::era::Event::Close)) => break,
            Ok(Some(eonos_app::era::Event::Key { usage: 0x29, pressed: true, .. })) => break,
            _ => { let _ = eonos_app::system::sleep_ms(16); }
        }
    }
    let _ = window.close();
    0
}

eonos_app::app!(ERA, main);

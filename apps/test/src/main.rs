#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::String;

fn main(_args: eonos_app::Args<'_>) -> i32 {
    let Ok(mut window) = eonos_app::era::Window::open("EonOS form demo", 480, 300) else { return 1; };
    let mut name = String::new();
    let mut plan = 0usize;
    let plans = ["Starter", "Builder", "Explorer"];
    let mut updates = false;
    let mut status = String::from("Tab between fields; Enter or Space activates controls.");
    loop {
        let mut close = false;
        while let Ok(Some(event)) = window.poll_event() {
            match event {
                eonos_app::era::Event::Close
                | eonos_app::era::Event::Key { usage: 0x29, pressed: true, .. } => close = true,
                _ => {}
            }
        }
        if close { break; }

        let _ = window.fill_rect(0, 0, 480, 300, 0x202A44);
        let _ = window.fill_rect(24, 24, 432, 252, 0xF0F0F0);
        let _ = window.text(44, 40, 0x12377B, "Profile settings");
        let _ = window.text(44, 56, 0x404040, &status);
        let _ = window.text(44, 82, 0x101010, "Name");
        let _ = window.text_input(124, 72, 286, 32, &mut name, "Type your name");
        let _ = window.text(44, 120, 0x101010, "Plan");
        let _ = window.dropdown(124, 112, 286, 24, &plans, &mut plan);
        let _ = window.checkbox(124, 212, "Send me updates", &mut updates);
        if window.button(124, 240, 132, 30, "Save profile").unwrap_or(false) {
            status = alloc::format!("Saved: {} / {}", if name.is_empty() { "(no name)" } else { &name }, plans[plan]);
        }
        let _ = window.present();
        let _ = eonos_app::system::sleep_ms(16);
    }
    let _ = window.close();
    0
}

eonos_app::app!(ERA, main);

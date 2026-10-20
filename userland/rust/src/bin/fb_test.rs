#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn run() -> eonos_rt::Result<()> {
    let mut screen = eonos_rt::graphics::Surface::open()?;
    let (width, height) = screen.dimensions();

    screen.fill_rect(0, 0, width, height, 0x101722)?;
    screen.fill_rect(0, 0, width, 36, 0x26364A)?;
    screen.text(16, 12, 0xFFFFFF, "EonOS framebuffer API test")?;

    let box_w = (width / 5).max(24);
    let box_h = (height / 4).max(24);
    let gap = 12;
    let colors = [0xE06C75, 0xE5C07B, 0x98C379, 0x61AFEF];
    for (index, color) in colors.iter().copied().enumerate() {
        let x = 20 + index as u32 * (box_w + gap);
        let y = 64;
        screen.fill_rect(x as i32, y as i32, box_w, box_h, color)?;
        screen.fill_rect((x + 6) as i32, (y + 6) as i32,
            box_w.saturating_sub(12), box_h.saturating_sub(12), 0x17202B)?;
    }

    let lower_y = 64 + box_h + gap;
    screen.fill_rect(20, lower_y as i32, width.saturating_sub(40), 18, 0x62D9C7)?;
    // Partly off-screen rectangle checks clipping at the left and bottom edges.
    screen.fill_rect(-10, height.saturating_sub(24) as i32, 80, 40, 0xC678DD)?;
    screen.present()?;

    eonos_rt::sleep_ms(2500)?;
    screen.fill_rect(0, 0, width, height, 0x000000)?;
    screen.present()?;
    eonos_rt::sleep_ms(500)?;
    screen.close()
}

fn main(_args: eonos_rt::Args<'_>) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            eonos_rt::eprintln!("fb-test: graphics operation failed ({})", error.0);
            1
        }
    }
}

eonos_rt::entry!(main);

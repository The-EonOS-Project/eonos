#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut values = args;
    let _program = values.next();
    let Some(path) = values.next().and_then(|value| value.to_str().ok()) else {
        eonos_rt::eprintln!("usage: era-wallpaper <png-file>");
        return 2;
    };
    if values.next().is_some() || !path.starts_with("/disk/") || path.len() > 512
        || path.contains('\n') || !path.to_ascii_lowercase().ends_with(".png") {
        eonos_rt::eprintln!("usage: era-wallpaper </disk/path-to-png>");
        return 2;
    }
    if !eonos_user::is_mountpoint("/disk").unwrap_or(false) {
        eonos_rt::eprintln!("era-wallpaper: mount a writable data disk at /disk first");
        return 1;
    }
    let mut image = match eonos_user::File::open(path, eonos_user::flags::READ_ONLY) {
        Ok(file) => file,
        Err(error) => {
            eonos_rt::eprintln!("era-wallpaper: could not open {} ({})", path, error);
            return 1;
        }
    };
    let mut signature = [0u8; 8];
    if image.read(&mut signature).ok() != Some(signature.len())
        || signature != *b"\x89PNG\r\n\x1a\n" {
        eonos_rt::eprintln!("era-wallpaper: {} is not a PNG file", path);
        return 1;
    }
    let _ = image.close();
    let result = (|| {
        let mut file = eonos_user::File::open(
            "/disk/era-wallpaper",
            eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
        )?;
        file.write_all(path.as_bytes())?;
        file.write_all(b"\n")?;
        file.close()?;
        eonos_rt::sync()?;
        Ok::<(), eonos_user::Error>(())
    })();
    match result {
        Ok(()) => {
            eonos_rt::println!("era-wallpaper: saved {}. Restart Era to apply it.", path);
            0
        }
        Err(error) => {
            eonos_rt::eprintln!("era-wallpaper: could not save wallpaper path ({})", error);
            1
        }
    }
}

eonos_rt::entry!(main);

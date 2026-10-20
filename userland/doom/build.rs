use std::env;
use std::path::PathBuf;
use std::process::{Command, Output};

fn run(command: &mut Command, description: &str) {
    let output: Output = command.output().unwrap_or_else(|error| panic!("could not run {description}: {error}"));
    if !output.status.success() {
        panic!("{description} failed:\n{}", String::from_utf8_lossy(&output.stderr));
    }
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..").canonicalize().expect("repository root");
    let doom_dir = manifest.join("../doom");
    let upstream = root.join("vendor/doomgeneric/doomgeneric");
    let include = doom_dir.join("include");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let cc = env::var("EONOS_CC").unwrap_or_else(|_| "gcc".into());
    let ar = env::var("EONOS_AR").unwrap_or_else(|_| "ar".into());
    let probe = Command::new(&cc).arg("-dumpmachine").output().expect("run C compiler -dumpmachine");
    let machine = String::from_utf8_lossy(&probe.stdout);
    if !probe.status.success() || !machine.starts_with("x86_64") {
        panic!("EONOS_CC must target x86_64; {cc} reports {machine}");
    }
    let gcc_include = Command::new(&cc).arg("-print-file-name=include").output()
        .expect("query C compiler builtin headers");
    let gcc_include = String::from_utf8_lossy(&gcc_include.stdout).trim().to_owned();

    let sources = [
        "dummy.c", "am_map.c", "doomdef.c", "doomstat.c", "dstrings.c", "d_event.c",
        "d_items.c", "d_iwad.c", "d_loop.c", "d_main.c", "d_mode.c", "d_net.c",
        "f_finale.c", "f_wipe.c", "g_game.c", "hu_lib.c", "hu_stuff.c", "info.c",
        "i_cdmus.c", "i_endoom.c", "i_joystick.c", "i_scale.c", "i_sound.c", "i_system.c",
        "i_timer.c", "memio.c", "m_argv.c", "m_bbox.c", "m_cheat.c", "m_config.c",
        "m_controls.c", "m_fixed.c", "m_menu.c", "m_misc.c", "m_random.c", "p_ceilng.c",
        "p_doors.c", "p_enemy.c", "p_floor.c", "p_inter.c", "p_lights.c", "p_map.c",
        "p_maputl.c", "p_mobj.c", "p_plats.c", "p_pspr.c", "p_saveg.c", "p_setup.c",
        "p_sight.c", "p_spec.c", "p_switch.c", "p_telept.c", "p_tick.c", "p_user.c",
        "r_bsp.c", "r_data.c", "r_draw.c", "r_main.c", "r_plane.c", "r_segs.c", "r_sky.c",
        "r_things.c", "sha1.c", "sounds.c", "statdump.c", "st_lib.c", "st_stuff.c",
        "s_sound.c", "tables.c", "v_video.c", "wi_stuff.c", "w_checksum.c", "w_file.c",
        "w_file_stdc.c", "w_main.c", "w_wad.c", "z_zone.c", "i_input.c", "i_video.c",
        "doomgeneric.c",
    ];
    let mut objects = Vec::new();
    let mut all_sources = vec![doom_dir.join("libc.c"), doom_dir.join("eonos_backend.c")];
    all_sources.extend(sources.iter().map(|source| upstream.join(source)));

    for source in all_sources {
        println!("cargo:rerun-if-changed={}", source.display());
        let stem = source.file_stem().unwrap().to_string_lossy();
        let object = out.join(format!("{stem}.o"));
        let mut command = Command::new(&cc);
        command.args([
            "-std=gnu11", "-O2", "-ffreestanding", "-fno-builtin", "-fno-stack-protector",
            "-fno-pic", "-mno-red-zone", "-march=x86-64", "-mno-avx", "-ffunction-sections", "-fdata-sections",
            "-DNORMALUNIX", "-DEONOS", "-D_DEFAULT_SOURCE", "-DDOOMGENERIC_RESX=320",
            "-DDOOMGENERIC_RESY=200",
        ]);
        command.arg("-nostdinc").arg("-isystem").arg(&gcc_include);
        command.arg("-I").arg(&include).arg("-I").arg(&upstream);
        command.arg("-c").arg(&source).arg("-o").arg(&object);
        run(&mut command, &format!("compile {}", source.display()));
        objects.push(object);
    }

    let archive = out.join("libdoomgeneric_eonos.a");
    let mut command = Command::new(&ar);
    command.arg("crs").arg(&archive);
    for object in objects { command.arg(object); }
    run(&mut command, "archive doomgeneric objects");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=doomgeneric_eonos");
    println!("cargo:rerun-if-env-changed=EONOS_CC");
    println!("cargo:rerun-if-env-changed=EONOS_AR");
    println!("cargo:rerun-if-changed={}", include.display());
    println!("cargo:rerun-if-changed={}", root.join("vendor/doomgeneric/UPSTREAM_REVISION").display());
}

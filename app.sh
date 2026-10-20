#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${EONOS_BUILD_DIR:-$ROOT/build}"
TARGET="$ROOT/config/targets/x86_64-eonos-user.json"
SDK="$ROOT/userland/app-sdk"

usage() {
    cat <<'EOF'
Usage:
  ./app.sh new <directory> <SHELL|ERA>
  ./app.sh build <app-directory>

`new` creates a small no_std Rust application project. `build` compiles it for
EonOS and writes a typed package under build/apps/<id>. Rebuild the OS image to
include packages in the initramfs.
EOF
}

valid_id() { [[ "$1" =~ ^[a-z][a-z0-9_-]{0,31}$ ]]; }

new_app() {
    local destination="${1:-}" kind="${2:-}" id sdk_rel
    if [[ -z "$destination" || ( "$kind" != SHELL && "$kind" != ERA ) ]]; then usage; exit 2; fi
    id="$(basename "$destination")"
    if ! valid_id "$id"; then echo "App directory name must be a 1-32 character lowercase id." >&2; exit 2; fi
    if [[ -e "$destination" ]]; then echo "Destination already exists: $destination" >&2; exit 2; fi
    mkdir -p "$destination/src"
    sdk_rel="$(python3 - "$destination" "$SDK" <<'PY'
import os, sys
print(os.path.relpath(os.path.abspath(sys.argv[2]), os.path.abspath(sys.argv[1])))
PY
)"
    cat > "$destination/Cargo.toml" <<EOF
[package]
name = "$id"
version = "0.1.0"
edition = "2021"

[package.metadata.eonos]
id = "$id"
type = "$kind"
name = "${id//-/ }"
description = "An EonOS $kind application"

[dependencies]
eonos-app = { path = "$sdk_rel" }


[profile.release]
panic = "abort"
strip = true
lto = true
opt-level = 2
EOF
    if [[ "$kind" == SHELL ]]; then
        cat > "$destination/src/main.rs" <<'EOF'
#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

fn main(_args: eonos_app::Args<'_>) -> i32 {
    let _ = eonos_app::system::print("Hello from an EonOS Shell app!\n");
    0
}

eonos_app::app!(SHELL, main);
EOF
    else
        cat > "$destination/src/main.rs" <<'EOF'
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
EOF
    fi
    echo "Created $destination. Build it with: ./app.sh build $destination"
}

build_app() {
    local source="${1:-}" metadata kind id name description bin package_dir binary marker
    if [[ -z "$source" ]]; then usage; exit 2; fi
    source="$(cd "$source" && pwd)"
    metadata="$(python3 - "$source/Cargo.toml" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    package = tomllib.load(f)["package"]
meta = package.get("metadata", {}).get("eonos", {})
for key in ("id", "type", "name", "description"):
    value = meta.get(key, "")
    if "\n" in str(value) or "\r" in str(value):
        raise SystemExit(f"invalid newline in metadata field {key}")
    print(value)
print(package["name"])
PY
)"
    mapfile -t fields <<< "$metadata"
    id="${fields[0]:-}"; kind="${fields[1]:-}"; name="${fields[2]:-}"; description="${fields[3]:-}"; bin="${fields[4]:-}"
    if ! valid_id "$id" || [[ "$kind" != SHELL && "$kind" != ERA ]] || [[ -z "$name" || -z "$description" || -z "$bin" ]]; then
        echo "Cargo.toml must define [package.metadata.eonos] id, type (SHELL or ERA), name, and description." >&2
        exit 2
    fi
    mkdir -p "$BUILD_DIR/app-target"
    RUSTFLAGS="-C link-arg=-T$ROOT/userland/linker.ld -C link-arg=-z -C link-arg=max-page-size=4096 -C link-arg=-z -C link-arg=noexecstack" \
        cargo build --manifest-path "$source/Cargo.toml" --target "$TARGET" \
            --target-dir "$BUILD_DIR/app-target" --release -Zjson-target-spec \
            -Zbuild-std=core,alloc,compiler_builtins \
            -Zbuild-std-features=compiler-builtins-mem --bin "$bin"
    binary="$BUILD_DIR/app-target/x86_64-eonos-user/release/$bin"
    marker="EONOSAPP1:$kind"
    if ! grep -a -F -q "$marker" "$binary"; then
        echo "The ELF app type marker does not match Cargo metadata ($kind). Use eonos_app::app!($kind, main)." >&2
        exit 1
    fi
    package_dir="$BUILD_DIR/apps/$id"
    rm -rf -- "$package_dir"
    mkdir -p "$package_dir"
    cp "$binary" "$package_dir/program.elf"
    cat > "$package_dir/manifest.eapp" <<EOF
EONOSAPP1
$id
$kind
$name
$description
EOF
    echo "Built $kind app '$name': $package_dir"
}

case "${1:-}" in
    new) shift; new_app "${1:-}" "${2:-}" ;;
    build) shift; build_app "${1:-}" ;;
    *) usage; exit 2 ;;
esac

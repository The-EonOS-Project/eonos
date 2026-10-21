#!/usr/bin/env bash
# EonOS build script.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

PROFILE="dev"
DEBUG_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --debug) DEBUG_BUILD=1 ;;
        dev|release) PROFILE="$arg" ;;
        *) echo "Unknown build option: $arg" >&2; exit 2 ;;
    esac
done

BUILD_DIR="${EONOS_BUILD_DIR:-$ROOT/build}"
if [ -z "$BUILD_DIR" ] || [ "$BUILD_DIR" = "/" ] || [ "$BUILD_DIR" = "$ROOT" ]; then
    echo "Refusing unsafe build directory: '$BUILD_DIR'" >&2
    exit 2
fi
ISO_ROOT="$BUILD_DIR/iso_root"
ISO_OUTPUT="$BUILD_DIR/iso_output"

LIMINE_DIR="$ROOT/limine"
USERLAND_DIR="$ROOT/userland"
INITRAMFS_SRC="$ROOT/initramfs"
DOOM_WAD="$ROOT/assets/doom/freedoom1.wad"

INITRAMFS_ROOT="$BUILD_DIR/initramfs_root"
INITRAMFS_NAME="initramfs.tar"
if [ "$DEBUG_BUILD" = "1" ]; then
    ISO_NAME="eonos_debugbuild.iso"
else
    ISO_NAME="eonos.iso"
fi
ISO_PATH="$ISO_OUTPUT/$ISO_NAME"

if [ ! -s "$DOOM_WAD" ]; then
    echo "Required Freedoom IWAD is missing: $DOOM_WAD" >&2
    exit 1
fi

TARGET_DIR="$ROOT/target/x86_64-eonos"

echo "==> Cleaning generated build artifacts (preserving eonos_disk.img)"
rm -rf "$ISO_ROOT" "$ISO_OUTPUT" "$INITRAMFS_ROOT"
rm -f "$BUILD_DIR/fat32.img"
mkdir -p "$BUILD_DIR" "$ISO_OUTPUT"

echo "==> Building kernel (profile: $PROFILE)"

BUILD_STD_FLAGS=(
    -Zjson-target-spec
    -Zbuild-std=core,alloc,compiler_builtins
    -Zbuild-std-features=compiler-builtins-mem
)
# Setting RUSTFLAGS in the environment replaces target rustflags from
# .cargo/config.toml, so keep the kernel linker script here as well.
KERNEL_RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=-Tconfig/linker.ld -C force-frame-pointers=yes"

if [ "$PROFILE" = "release" ]; then
    if [ "$DEBUG_BUILD" = "1" ]; then
        RUSTFLAGS="$KERNEL_RUSTFLAGS" cargo build --release --features debug-logs "${BUILD_STD_FLAGS[@]}" --target config/targets/x86_64-eonos.json
    else
        RUSTFLAGS="$KERNEL_RUSTFLAGS" cargo build --release "${BUILD_STD_FLAGS[@]}" --target config/targets/x86_64-eonos.json
    fi
    KERNEL_BIN="$TARGET_DIR/release/eonos"
else
    if [ "$DEBUG_BUILD" = "1" ]; then
        RUSTFLAGS="$KERNEL_RUSTFLAGS" cargo build --features debug-logs "${BUILD_STD_FLAGS[@]}" --target config/targets/x86_64-eonos.json
    else
        RUSTFLAGS="$KERNEL_RUSTFLAGS" cargo build "${BUILD_STD_FLAGS[@]}" --target config/targets/x86_64-eonos.json
    fi
    KERNEL_BIN="$TARGET_DIR/debug/eonos"
fi

echo "==> Building userland"

CFLAGS_x86_64_eonos_user="-fno-stack-protector" \
RUSTFLAGS="-C link-arg=-T$USERLAND_DIR/linker.ld -C link-arg=-z -C link-arg=max-page-size=4096 -C link-arg=-z -C link-arg=noexecstack" \
    cargo build \
        --manifest-path "$USERLAND_DIR/rust/Cargo.toml" \
        --target "$ROOT/config/targets/x86_64-eonos-user.json" \
        --target-dir "$BUILD_DIR/rust-target" \
        --release \
        -Zjson-target-spec \
        -Zbuild-std=core,alloc,compiler_builtins \
        -Zbuild-std-features=compiler-builtins-mem
RUST_HELLO_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/rust-hello"
RUST_FILE_DEMO_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/rust-file-demo"
RUST_PROCESS_DEMO_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/rust-process-demo"
RUST_INPUT_DEMO_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/rust-input-demo"
RUST_CURLER_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/curler"
RUST_SHELL_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/rust-shell"
RUST_ECHO_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/echo"
RUST_CAT_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/cat"
RUST_LS_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/ls"
RUST_SLEEP_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/sleep"
RUST_PWD_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/pwd"
RUST_ENV_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/env"
RUST_ERA_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/era"
RUST_FB_TEST_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/fb-test"
RUST_DOOM_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/doom"
RUST_HEAD_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/head"
RUST_WC_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/wc"
RUST_GREP_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/grep"
RUST_TAIL_BIN="$BUILD_DIR/rust-target/x86_64-eonos-user/release/tail"

as --64 "$USERLAND_DIR/init.S" -o "$BUILD_DIR/init.o"

ld -static -nostdlib \
    -z max-page-size=4096 \
    -z noexecstack \
    -T "$USERLAND_DIR/linker.ld" \
    -o "$BUILD_DIR/init.elf" \
    "$BUILD_DIR/init.o"

as --64 "$USERLAND_DIR/hello.S" -o "$BUILD_DIR/hello.o"

ld -static -nostdlib \
    -z max-page-size=4096 \
    -z noexecstack \
    -T "$USERLAND_DIR/linker.ld" \
    -o "$BUILD_DIR/hello.elf" \
    "$BUILD_DIR/hello.o"

echo "==> Creating persistent FAT32 disk image"
for tool in mkfs.fat mcopy; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Required tool '$tool' is missing (install dosfstools and mtools)" >&2
        exit 1
    fi
done
DISK_IMAGE="$BUILD_DIR/eonos_disk.img"
MAX_DISK_BYTES=$((4 * 1024 * 1024 * 1024))
if [ ! -e "$DISK_IMAGE" ]; then
    DISK_IMAGE_TMP="$DISK_IMAGE.new"
    rm -f "$DISK_IMAGE_TMP"
    truncate -s "$MAX_DISK_BYTES" "$DISK_IMAGE_TMP"
    mkfs.fat -F 32 -n EONOS_DATA "$DISK_IMAGE_TMP" >/dev/null
    mcopy -i "$DISK_IMAGE_TMP" "$BUILD_DIR/hello.elf" ::/HELLO.ELF
    mcopy -i "$DISK_IMAGE_TMP" "$ROOT/README.md" ::/README.TXT
    mv "$DISK_IMAGE_TMP" "$DISK_IMAGE"
elif [ ! -f "$DISK_IMAGE" ]; then
    echo "Disk image path exists but is not a regular file: $DISK_IMAGE" >&2
    exit 1
fi
DISK_BYTES=$(stat -c %s "$DISK_IMAGE")
if [ "$DISK_BYTES" -gt "$MAX_DISK_BYTES" ]; then
    echo "Disk image exceeds the 4 GiB maximum: $DISK_IMAGE ($DISK_BYTES bytes)" >&2
    exit 1
fi
if [ "$DISK_BYTES" -lt 512 ]; then
    echo "Disk image is too small to contain a filesystem: $DISK_IMAGE" >&2
    exit 1
fi

echo "==> Packing initramfs"

mkdir -p "$INITRAMFS_ROOT"

cp -r "$INITRAMFS_SRC"/. "$INITRAMFS_ROOT"/
cp "$BUILD_DIR/init.elf" "$INITRAMFS_ROOT/asm-shell"
cp "$BUILD_DIR/hello.elf" "$INITRAMFS_ROOT/hello"
cp "$RUST_HELLO_BIN" "$INITRAMFS_ROOT/rust-hello"
cp "$RUST_FILE_DEMO_BIN" "$INITRAMFS_ROOT/rust-file-demo"
cp "$RUST_PROCESS_DEMO_BIN" "$INITRAMFS_ROOT/rust-process-demo"
cp "$RUST_INPUT_DEMO_BIN" "$INITRAMFS_ROOT/rust-input-demo"
cp "$RUST_SHELL_BIN" "$INITRAMFS_ROOT/rust-shell"
cp "$RUST_SHELL_BIN" "$INITRAMFS_ROOT/init"
mkdir -p "$INITRAMFS_ROOT/bin"
cp "$RUST_SHELL_BIN" "$INITRAMFS_ROOT/bin/rust-shell"
cp "$RUST_ECHO_BIN" "$INITRAMFS_ROOT/bin/echo"
cp "$RUST_CAT_BIN" "$INITRAMFS_ROOT/bin/cat"
cp "$RUST_LS_BIN" "$INITRAMFS_ROOT/bin/ls"
cp "$RUST_SLEEP_BIN" "$INITRAMFS_ROOT/bin/sleep"
cp "$RUST_PWD_BIN" "$INITRAMFS_ROOT/bin/pwd"
cp "$RUST_ENV_BIN" "$INITRAMFS_ROOT/bin/env"
cp "$RUST_ERA_BIN" "$INITRAMFS_ROOT/bin/era"
cp "$RUST_FB_TEST_BIN" "$INITRAMFS_ROOT/bin/fb-test"
cp "$RUST_HEAD_BIN" "$INITRAMFS_ROOT/bin/head"
cp "$RUST_WC_BIN" "$INITRAMFS_ROOT/bin/wc"
cp "$RUST_GREP_BIN" "$INITRAMFS_ROOT/bin/grep"
cp "$RUST_TAIL_BIN" "$INITRAMFS_ROOT/bin/tail"
cp "$RUST_DOOM_BIN" "$INITRAMFS_ROOT/bin/doom"
cp "$RUST_CURLER_BIN" "$INITRAMFS_ROOT/bin/curler"
if [ -d "$BUILD_DIR/apps" ]; then
    mkdir -p "$INITRAMFS_ROOT/packages"
    for package in "$BUILD_DIR/apps"/*.eonapp; do
        [ -s "$package" ] || continue
        cp "$package" "$INITRAMFS_ROOT/packages/"
    done
fi
mkdir -p "$INITRAMFS_ROOT/doom"
cp "$DOOM_WAD" "$INITRAMFS_ROOT/doom/freedoom1.wad"
cp "$ROOT/assets/doom/COPYING.txt" "$INITRAMFS_ROOT/doom/COPYING.txt"
cp "$ROOT/assets/doom/CREDITS.txt" "$INITRAMFS_ROOT/doom/CREDITS.txt"

tar --format=ustar \
    -cf "$BUILD_DIR/$INITRAMFS_NAME" \
    -C "$INITRAMFS_ROOT" .

if [ ! -d "$LIMINE_DIR" ]; then
    echo "==> Fetching Limine (binary branch)"

    git clone \
        https://github.com/limine-bootloader/limine.git \
        --branch v8.x-binary \
        --depth=1 \
        "$LIMINE_DIR"

    make -C "$LIMINE_DIR"
fi

echo "==> Assembling ISO tree"

mkdir -p \
    "$ISO_ROOT/boot/limine" \
    "$ISO_ROOT/EFI/BOOT"

cp "$KERNEL_BIN" \
    "$ISO_ROOT/boot/eonos.elf"

cp "$ROOT/config/boot/limine.conf" \
    "$ISO_ROOT/boot/limine/limine.conf"

cp "$ROOT/config/boot/limine.conf" \
    "$ISO_ROOT/limine.conf"

cp "$BUILD_DIR/$INITRAMFS_NAME" \
    "$ISO_ROOT/boot/$INITRAMFS_NAME"

cp \
    "$LIMINE_DIR/limine-bios.sys" \
    "$LIMINE_DIR/limine-bios-cd.bin" \
    "$LIMINE_DIR/limine-uefi-cd.bin" \
    "$ISO_ROOT/boot/limine/"

cp \
    "$LIMINE_DIR/BOOTX64.EFI" \
    "$ISO_ROOT/EFI/BOOT/"

cp \
    "$LIMINE_DIR/BOOTIA32.EFI" \
    "$ISO_ROOT/EFI/BOOT/" \
    2>/dev/null || true

echo "==> Building ISO"

xorriso -as mkisofs \
    -R -r -J \
    -b boot/limine/limine-bios-cd.bin \
    -no-emul-boot \
    -boot-load-size 4 \
    -boot-info-table \
    -hfsplus \
    -apm-block-size 2048 \
    --efi-boot boot/limine/limine-uefi-cd.bin \
    -efi-boot-part \
    --efi-boot-image \
    --protective-msdos-label \
    "$ISO_ROOT" \
    -o "$ISO_PATH"

echo "==> Installing BIOS stage 1 into ISO"

"$LIMINE_DIR/limine" bios-install "$ISO_PATH"

# Record which kernel profile produced this ISO. run.sh uses this to avoid
# silently booting a dev image when an optimized release image was requested.
printf '%s:%s\n' "$PROFILE" "$DEBUG_BUILD" > "$ISO_PATH.profile"

echo "==> Done: $ISO_PATH"
echo "==> QEMU launch scripts attach virtual USB keyboard and mouse; run ./run.sh or ./runb.sh to test them"

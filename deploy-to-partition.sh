#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DEVICE="/dev/nvme0n1p8"
MOUNTPOINT="/mnt/eonos"
ISO_ROOT="$ROOT/build/iso_root"

fail() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

command -v sudo >/dev/null 2>&1 || fail "sudo is required"
command -v lsblk >/dev/null 2>&1 || fail "lsblk is required"
command -v findmnt >/dev/null 2>&1 || fail "findmnt is required"

[[ -b "$DEVICE" ]] || fail "$DEVICE is not a block device"

read -r partition_type filesystem label < <(
    lsblk -dnro TYPE,FSTYPE,LABEL "$DEVICE"
)
[[ "$partition_type" == "part" ]] || fail "$DEVICE is not a partition"
[[ "$filesystem" == "vfat" ]] || fail "$DEVICE is not FAT32 (vfat)"
[[ "$label" == "EONOS" ]] || fail "$DEVICE does not have the EONOS filesystem label"

for file in \
    "$ISO_ROOT/EFI/BOOT/BOOTX64.EFI" \
    "$ISO_ROOT/boot/eonos.elf" \
    "$ISO_ROOT/boot/initramfs.tar" \
    "$ISO_ROOT/boot/limine/limine.conf"; do
    [[ -f "$file" ]] || fail "Missing build file: $file (run ./build.sh first)"
done

sudo mkdir -p "$MOUNTPOINT"

mounted_here=0
if findmnt -rn --mountpoint "$MOUNTPOINT" >/dev/null 2>&1; then
    mounted_source="$(findmnt -rn -o SOURCE --mountpoint "$MOUNTPOINT")"
    [[ "$mounted_source" == "$DEVICE" ]] \
        || fail "$MOUNTPOINT is already mounted from $mounted_source"
else
    existing_mount="$(findmnt -rn -S "$DEVICE" -o TARGET || true)"
    [[ -z "$existing_mount" ]] \
        || fail "$DEVICE is already mounted at $existing_mount"
    sudo mount "$DEVICE" "$MOUNTPOINT"
    mounted_here=1
fi

cleanup() {
    if [[ "$mounted_here" == 1 ]]; then
        sudo umount "$MOUNTPOINT" || true
    fi
}
trap cleanup EXIT

sudo install -D -m 0644 "$ISO_ROOT/EFI/BOOT/BOOTX64.EFI" \
    "$MOUNTPOINT/EFI/BOOT/BOOTX64.EFI"
sudo install -D -m 0644 "$ISO_ROOT/boot/eonos.elf" \
    "$MOUNTPOINT/boot/eonos.elf"
sudo install -D -m 0644 "$ISO_ROOT/boot/initramfs.tar" \
    "$MOUNTPOINT/boot/initramfs.tar"
sudo install -D -m 0644 "$ISO_ROOT/boot/limine/limine.conf" \
    "$MOUNTPOINT/boot/limine/limine.conf"

sudo sync
if [[ "$mounted_here" == 1 ]]; then
    sudo umount "$MOUNTPOINT"
    mounted_here=0
fi

printf 'EonOS boot files deployed to %s on %s.\n' "$MOUNTPOINT" "$DEVICE"

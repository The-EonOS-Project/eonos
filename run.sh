#!/usr/bin/env bash
# Boot EonOS in QEMU. Serial output goes to your terminal.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

BUILD_DIR="${EONOS_BUILD_DIR:-$ROOT/build}"
DEBUG_BUILD=0
if [ "${1:-}" = "--debug" ]; then
    DEBUG_BUILD=1
elif [ "$#" -ne 0 ]; then
    echo "Usage: $0 [--debug]" >&2
    exit 2
fi

if [ -n "${EONOS_PROFILE:-}" ]; then
    PROFILE="$EONOS_PROFILE"
elif [ "$DEBUG_BUILD" = "1" ]; then
    PROFILE="dev"
else
    PROFILE="release"
fi
case "$PROFILE" in
    dev|release) ;;
    *) echo "EONOS_PROFILE must be dev or release" >&2; exit 2 ;;
esac

if [ "$DEBUG_BUILD" = "1" ]; then
    ISO_PATH="$BUILD_DIR/iso_output/eonos_debugbuild.iso"
else
    ISO_PATH="$BUILD_DIR/iso_output/eonos.iso"
fi
PROFILE_STAMP="$ISO_PATH.profile"

if [ -n "${EONOS_NVME_DISK:-}" ]; then
    DISK_PATH="$EONOS_NVME_DISK"
else
    DISK_PATH="$BUILD_DIR/eonos_disk.img"
fi

UEFI="${UEFI:-1}"
QEMU_VIDEO="${EONOS_QEMU_VIDEO:-auto}"
QEMU_DEVICE_LIST="$(qemu-system-x86_64 -device help 2>&1 || true)"
qemu_display_starts() {
    local model="$1"
    # Device help only reports registered models. Some split QEMU installs
    # advertise a module that crashes during realization, so probe it without
    # booting a guest before selecting it for the real run.
    command -v timeout >/dev/null 2>&1 || return 0
    local status=0
    (
        ulimit -c 0
        timeout --signal=TERM --kill-after=1s 1s qemu-system-x86_64 \
            -M q35 -nodefaults -display none -S -monitor none -serial none \
            -device "$model" >/dev/null 2>&1
    ) || status=$?
    [[ "$status" -eq 124 || "$status" -eq 137 ]]
}
if [ "$QEMU_VIDEO" = "auto" ]; then
    # Prefer the standalone PCI GPU. It avoids the legacy VGA compatibility
    # wrapper used by virtio-vga, which crashes in some QEMU builds.
    QEMU_VIDEO="std"
    for model in virtio-gpu-pci virtio-vga; do
        if [[ "$QEMU_DEVICE_LIST" == *"name \"$model\""* ]]; then
            if qemu_display_starts "$model" 2>/dev/null; then
                QEMU_VIDEO="$model"
                break
            fi
            echo "QEMU reports $model but it failed the startup probe." >&2
        fi
    done
    if [ "$QEMU_VIDEO" = "std" ]; then
        echo "Check that the QEMU virtio-gpu backend module is installed and matches the QEMU version." >&2
        echo "On Arch Linux, install qemu-hw-display-virtio-gpu with the PCI/VGA device package." >&2
    fi
fi
case "$QEMU_VIDEO" in
    virtio-gpu-pci|virtio-vga)
        if [[ "$QEMU_DEVICE_LIST" != *"name \"$QEMU_VIDEO\""* ]]; then
            echo "QEMU does not have the $QEMU_VIDEO device model installed." >&2
            if [ "$QEMU_VIDEO" = "virtio-gpu-pci" ]; then
                echo "On Arch Linux, install qemu-hw-display-virtio-gpu-pci." >&2
            else
                echo "On Arch Linux, install qemu-hw-display-virtio-vga." >&2
            fi
            exit 2
        fi
        if ! qemu_display_starts "$QEMU_VIDEO" 2>/dev/null; then
            echo "QEMU advertises $QEMU_VIDEO, but it crashed or exited during the startup probe." >&2
            echo "On Arch Linux, also install qemu-hw-display-virtio-gpu (the shared backend module)." >&2
            echo "Use EONOS_QEMU_VIDEO=std to boot, then check the host QEMU packages/core dump." >&2
            exit 2
        fi
        # The standalone PCI model is not VGA-compatible. Disable QEMU's
        # implicit VGA adapter so its window follows the device EonOS drives.
        # Use 85% of the former 1600x900 desktop size; keep GTK at 1:1 scaling.
        VIDEO_ARGS=(-vga none -device "$QEMU_VIDEO,xres=1360,yres=765")
        ;;
    std) VIDEO_ARGS=(-device VGA,xres=1360,yres=765,xmax=1360,ymax=765) ;;
    *) echo "EONOS_QEMU_VIDEO must be auto, virtio-gpu-pci, virtio-vga, or std" >&2; exit 2 ;;
esac
echo "QEMU display device: $QEMU_VIDEO"

NEEDS_BUILD=0
if [ ! -f "$ISO_PATH" ]; then
    NEEDS_BUILD=1
elif [ -n "$(find src userland initramfs tools config assets -type f -newer "$ISO_PATH" -print -quit)" ]; then
    NEEDS_BUILD=1
else
    for source in README.md Cargo.toml Cargo.lock build.sh .cargo/config.toml; do
        if [ -f "$source" ] && [ "$source" -nt "$ISO_PATH" ]; then
            NEEDS_BUILD=1
            break
        fi
    done
fi
if [ ! -f "$PROFILE_STAMP" ] || [ "$(cat "$PROFILE_STAMP")" != "$PROFILE:$DEBUG_BUILD" ]; then
    NEEDS_BUILD=1
fi
if [ -z "${EONOS_NVME_DISK:-}" ] && [ ! -f "$DISK_PATH" ]; then
    NEEDS_BUILD=1
fi

if [ "$NEEDS_BUILD" = "1" ]; then
    echo "No current $PROFILE ISO found, building it first..."
    if [ "$DEBUG_BUILD" = "1" ]; then
        ./build.sh "$PROFILE" --debug
    else
        ./build.sh "$PROFILE"
    fi
fi

ARGS=(
    # Disable the emulated i8042 so input must come through USB.
    -M q35,i8042=off
    # Expose host-supported CPU features such as RDRAND, which verified TLS
    # requires for cryptographically secure client nonces.
    -cpu max
    "${VIDEO_ARGS[@]}"
    -display gtk,zoom-to-fit=off
    -rtc base=utc
    -m 2G
    -serial stdio
    -cdrom "$ISO_PATH"
    -boot d
    -device qemu-xhci,id=xhci
    -device usb-kbd,bus=xhci.0
    -device usb-mouse,bus=xhci.0
    -netdev user,id=net0
    -device e1000,netdev=net0
)

if [ "$UEFI" = "1" ]; then
    OVMF_CODE="/usr/share/edk2/x64/OVMF_CODE.4m.fd"
    OVMF_VARS="/usr/share/edk2/x64/OVMF_VARS.4m.fd"

    if [ ! -f "$OVMF_CODE" ]; then
        OVMF_CODE="/usr/share/edk2-ovmf/x64/OVMF_CODE.4m.fd"
        OVMF_VARS="/usr/share/edk2-ovmf/x64/OVMF_VARS.4m.fd"
    fi

    if [ ! -f "$OVMF_CODE" ]; then
        echo "OVMF firmware not found. Install edk2-ovmf:"
        echo "  sudo pacman -S edk2-ovmf"
        exit 1
    fi

    VARS_FILE="$BUILD_DIR/OVMF_VARS.fd"

    if [ ! -f "$VARS_FILE" ]; then
        cp "$OVMF_VARS" "$VARS_FILE"
    fi

    ARGS+=(
        -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE"
        -drive if=pflash,format=raw,file="$VARS_FILE"
    )
fi

if [ ! -f "$DISK_PATH" ]; then
    echo "NVMe disk image not found: $DISK_PATH" >&2
    exit 1
fi
ARGS+=(
    -drive "file=$DISK_PATH,format=raw,if=none,id=eonos_nvme"
    -device nvme,drive=eonos_nvme,serial=EONOSNVME0001
)

exec qemu-system-x86_64 "${ARGS[@]}"

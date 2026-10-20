#!/usr/bin/env bash
# Build and run EonOS in QEMU with USB keyboard and mouse input.
set -euo pipefail

PROFILE="${EONOS_PROFILE:-release}"
if [ "${1:-}" = "--debug" ] && [ -z "${EONOS_PROFILE:-}" ]; then
    PROFILE="dev"
fi
case "$PROFILE" in
    dev|release) ;;
    *) echo "EONOS_PROFILE must be dev or release" >&2; exit 2 ;;
esac

if [ "$#" -eq 1 ] && [ "$1" = "--debug" ]; then
    ./build.sh "$PROFILE" --debug
    export EONOS_PROFILE="$PROFILE"
    exec ./run.sh --debug
fi

if [ "$#" -ne 0 ]; then
    echo "Usage: $0 [--debug]" >&2
    exit 2
fi

./build.sh "$PROFILE"
export EONOS_PROFILE="$PROFILE"
exec ./run.sh

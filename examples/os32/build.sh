#!/usr/bin/env bash
# Build ForgeOS32 into a bootable 1.44 MB floppy image.
#
#   ./build.sh            # everything into build/
#   ./build.sh run        # ...and boot it in QEMU
set -euo pipefail
cd "$(dirname "$0")"

FORGEC=${FORGEC:-../../target/release/forgec}
if [ ! -x "$FORGEC" ]; then
    echo "forgec not found at $FORGEC - run: cargo build --release" >&2
    exit 1
fi

mkdir -p build

echo "== stage 1: boot sector (sector 0) =="
"$FORGEC" src/boot/boot.dev -o build/boot.bin --target x86_16-boot

echo "== stage 2: loader (sectors 1-2, loaded at 0x9000) =="
"$FORGEC" src/boot/loader.dev -o build/loader.raw --linker os32-loader.fld
SIZE=$(stat -c %s build/loader.raw)
if [ "$SIZE" -gt 1024 ]; then
    echo "loader.raw is $SIZE bytes - exceeds two sectors (1024 B) as read by the boot sector" >&2
    exit 1
fi
echo "   loader.raw: $SIZE bytes"

echo "== stage 3: 32-bit kernel (LBA 3-10, loaded at 0x100000) =="
"$FORGEC" src/kernel/kernel.dev -o build/kernel.raw --linker os32-kernel.fld
SIZE=$(stat -c %s build/kernel.raw)
if [ "$SIZE" -gt 4096 ]; then
    echo "kernel.raw is $SIZE bytes - exceeds the 8-sector (4096 B) budget of the loader" >&2
    exit 1
fi
echo "   kernel.raw: $SIZE bytes"

echo "== assemble 1.44 MB floppy =="
# 1.44 MB floppy = 2880 sectors of 512 bytes = 1474560 bytes.
truncate -s 1474560 build/os32.img
dd if=build/boot.bin   of=build/os32.img conv=notrunc status=none
dd if=build/loader.raw of=build/os32.img bs=512 seek=1 conv=notrunc status=none
dd if=build/kernel.raw of=build/os32.img bs=512 seek=3 conv=notrunc status=none
echo "   os32.img: $(stat -c %s build/os32.img) bytes"

if [ "${1:-}" = "run" ]; then
    qemu-system-x86_64 -accel tcg,thread=multi \
        -drive file=build/os32.img,format=raw,if=ide -nographic
fi
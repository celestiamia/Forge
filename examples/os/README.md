# ForgeOS

A tiny 16-bit real-mode operating system written entirely in Forge — no
inline assembly anywhere in the project.  It builds with `forgec` alone:
stage-1 boot sector, stage-2 loader, and stage-3 kernel are all `.dev`
modules compiled by the x86_16 backend.

## Layout

| Stage | Source | Output | Sectors | Load address |
|-------|--------|--------|---------|--------------|
| 1 — boot sector | `src/boot/boot.dev` | `build/boot.bin` | 0 | `0x7C00` (BIOS) |
| 2 — loader | `src/boot/loader.dev` | `build/loader.raw` | 1–2 | `0x9000` |
| 3 — kernel | `src/kernel/kernel.dev` | `build/kernel.raw` | 3–10 | `0x7C00` |

The boot sector reads sectors 1–2 into `0x9000` and jumps to it; the loader
reads sectors 3–10 into `0x7C00` (a fresh segment: `ds`/`es`/`ss` = 0,
`sp` = `0x7C00`) and jumps to `_start`.  Both disk reads use BIOS
`int 13h` CHS via `_dev_bios_disk_read` (ES = address >> 4, BX = address & 0xF).

The stage-2 loader and stage-3 kernel are compiled as `FORMAT raw` images
with `LOAD` set to their load addresses (`os-loader.fld`, `os.fld`), so the
kernel's absolute string references are fixed up against `0x7C00` and the
loader's against `0x9000`.

## Build and run

```sh
cargo build --release

cd examples/os
./build.sh            # build build/os.img
./build.sh run        # ...and boot it in QEMU (floppy drive)
```

The three stages must fit their budgets: boot.bin = 512 B (with `0x55AA`),
loader.raw ≤ 1024 B (2 sectors), kernel.raw ≤ 4096 B (8 sectors).  `build.sh`
enforces the loader and kernel limits.

For reliable operation under QEMU use the IDE disk with multi-thread TCG
(the floppy controller hangs single-thread TCG):

```sh
qemu-system-x86_64 -accel tcg,thread=multi \
  -drive file=build/os.img,format=raw,if=ide -nographic
```

## Driving the shell

The shell reads keys via BIOS `int 16h`.  To script it from the host, run
QEMU with a monitor socket and use `socat` + the `sendkey` command:

```sh
qemu-system-x86_64 -accel tcg,thread=multi \
  -drive file=build/os.img,format=raw,if=ide -nographic \
  -monitor unix:/tmp/mon.sock,server,nowait

(sleep 7; printf 'sendkey c\nsendkey a\nsendkey l\nsendkey c\nsendkey spc\nsendkey 4\nsendkey 2\nsendkey ret\n') \
  | socat - UNIX-CONNECT:/tmp/mon.sock
```

## Commands

```
help    - this list
hello   - greet from an app module
about   - about this OS
mem     - conventional memory size
calc    - square of a number, e.g. `calc 42`
dump    - hex dump of low memory
clear   - clear the screen
reboot  - reboot the machine
```

All commands are implemented in `src/apps/*.dev` (or inline for `clear`);
each is a plain Forge module imported by `src/shell/commands.dev`.
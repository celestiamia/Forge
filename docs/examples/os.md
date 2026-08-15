# ForgeOS — a 16-bit OS in pure Forge

A tiny 16-bit real-mode operating system written entirely in Forge — no
inline assembly anywhere in the project.  It is the reference
implementation of the [x86_16 target](../targets/x86_16.md) and lives in
[`examples/os/`](../../examples/os/).

## Three-stage boot

| Stage | Source | Output | Sectors | Load address | Format |
|-------|--------|--------|---------|--------------|--------|
| 1 — boot sector | `src/boot/boot.dev` | `build/boot.bin` | 0 | `0x7C00` (BIOS) | `flat` |
| 2 — loader | `src/boot/loader.dev` | `build/loader.raw` | 1–2 | `0x9000` | `raw` (`LOAD 0x9000`) |
| 3 — kernel | `src/kernel/kernel.dev` | `build/kernel.raw` | 3–10 | `0x7C00` | `raw` |

- The boot sector reads sectors 1–2 into `0x9000` with
  `_dev_bios_disk_read` (INT 13h CHS) and far-jumps there with `_dev_jump`.
- The loader (running at `0x9000`, clear of the kernel's `0x7C00..0x8C00`
  region) reads sectors 3–10 into `0x7C00`, resets segments, and far-jumps
  to the kernel.
- Each stage's `.fld` declares its `LOAD` address so the compiler fixes up
  absolute string addresses correctly (`os-loader.fld`, `os.fld`).

## Build and run

```sh
cargo build --release
cd examples/os
./build.sh            # build build/os.img
./build.sh run        # ...and boot it in QEMU (floppy drive)
```

`build.sh` enforces the stage budgets: boot.bin = 512 B (with `0x55AA`),
loader.raw ≤ 1024 B, kernel.raw ≤ 4096 B.

For reliable operation under QEMU use the IDE disk with multi-thread TCG
(the floppy controller hangs single-thread TCG):

```sh
qemu-system-x86_64 -accel tcg,thread=multi \
  -drive file=build/os.img,format=raw,if=ide -nographic
```

## Shell

The kernel starts an interactive shell reading keys via BIOS `int 16h`:

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

Each command is a plain Forge module under `src/apps/`, dispatched by
`src/shell/commands.dev`.

## Driving the shell from the host

For scripting, run QEMU with a monitor socket and use `socat` + `sendkey`:

```sh
qemu-system-x86_64 -accel tcg,thread=multi \
  -drive file=build/os.img,format=raw,if=ide -nographic \
  -monitor unix:/tmp/mon.sock,server,nowait

(sleep 7; printf 'sendkey c\nsendkey a\nsendkey l\nsendkey c\nsendkey spc\nsendkey 4\nsendkey 2\nsendkey ret\n') \
  | socat - UNIX-CONNECT:/tmp/mon.sock
```

## Testing

The integration test `os_dev_boots_shell_and_calc` builds all three stages
with `forgec`, assembles `os.img`, boots it in QEMU, types `calc 42` into
the shell over the monitor, and asserts the result (`sq(0x002A) = 0x06E4`).
It needs `qemu-system-x86_64` and `socat` on PATH.
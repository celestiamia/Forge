# ForgeOS32

A minimal 16-bit-to-32-bit boot chain written entirely in Forge — no inline
assembly anywhere in the sources, only `forgec`:

1. A real-mode **boot sector** (`src/boot/boot.dev`, x86_16, 512 bytes) that
   loads the stage-2 loader and jumps to it.
2. A real-mode **stage-2 loader** (`src/boot/loader.dev`, x86_16) that uses
   BIOS `int 13h` AH=42h (LBA extensions) to read the 32-bit kernel into low
   memory, then switches the CPU to **protected mode** and jumps to it.
3. A **32-bit kernel** (`src/kernel/kernel.dev`, x86_32 raw binary) linked at
   `0x100000` that prints a banner + memory probe and halts.

This is the reference target for the x86_16 → x86_32 cross-mode story: the
16-bit backend, the real-mode BIOS stubs, the protected-mode switch, and the
32-bit `raw` codegen all work together via a single `forgec` toolchain.

## Layout

| Stage | Source | Output       | Sectors (LBA) | Loaded at      |
|-------|--------|--------------|---------------|----------------|
| 1 — boot   | `src/boot/boot.dev`  | `build/boot.bin`   | 0        | `0x7C00` (BIOS) |
| 2 — loader | `src/boot/loader.dev` | `build/loader.raw` | 1–2      | `0x9000` |
| 3 — kernel | `src/kernel/kernel.dev` | `build/kernel.raw` | 3–10     | `0x100000` (physical) |

The boot sector reads LBA 1–2 into `0x9000` and far-jumps to `_start`; the
loader reads LBA 3–10 into a low-memory staging buffer at `0x8000` via
`int 13h` AH=42h, then invokes `_dev_enter_pmode` to switch to 32-bit mode.

## The protected-mode switch & the 1 MiB trick

SeaBIOS's AH=42h disk transfer addresses its buffer through a 16-bit
`ES:DI` pair (`ES = buf >> 4`, `DI = buf & 15`), so it physically cannot write
above the low 1 MiB — a kernel buffer at `0x100000` silently wraps and the
data lands elsewhere (it even lands at `0x000000` for a `0x100000` target).

The switch is therefore done in two steps:

1. The loader reads the kernel into the **staging buffer at `0x8000`** (well
   within the low 1 MiB).
2. `_dev_enter_pmode` (see `src/backend/codegen16/program.rs`) enables A20,
   installs a flat 4 GB GDT, sets `CR0.PE`, and far-jumps through selector
   `0x08` to a 32-bit trampoline that **rep movsd's the 4096-byte staging
   buffer from `0x8000` to `0x100000`**, parks the stack at `0x70000`, and
   jumps to the kernel at `0x100000` (passed in as `lo`/`hi` words of the
   32-bit entry address).

The kernel's entry (`_start`) is the `ENTRY` directive in
`os32-kernel.fld`; strings are fixed up against `LOAD 0x100000` so they
resolve correctly at runtime.

## Build

```sh
cargo build --release                     # build the compiler
./examples/os32/build.sh                  # assemble build/os32.img
./examples/os32/build.sh run              # ...and boot it in QEMU
```

## Run

```sh
qemu-system-x86_64 \
    -accel tcg,thread=multi \
    -drive file=examples/os32/build/os32.img,format=raw,if=ide -nographic
```

Expected (the loader's teletype is mirrored to the serial console by SeaBIOS):

```
ForgeOS32: loading kernel: ok
ForgeOS32: hello from 32-bit protected mode
conv mem: 0009FC00 B
```

The machine `cli; hlt`s after printing.

## Sources

```
src/boot/
  boot.dev      # stage-1 boot sector (x86_16 flat, 512 B)
  loader.dev    # stage-2 LBA loader + pmode switch (x86_16 flat/raw)
src/kernel/
  kernel.dev    # 32-bit kernel: serial + VGA init, mem probe, halt (x86_32 raw)
os32-loader.fld # loader descriptor  (ARCH x86_16 / FORMAT flat  / LOAD 0x9000)
os32-kernel.fld # kernel descriptor   (ARCH x86_32 / FORMAT raw  / ENTRY _start / LOAD 0x100000)
```

A standalone, byte-identical descriptor for the kernel alone lives at
[`examples/targets/x86_32-raw.fld`](../../examples/targets/x86_32-raw.fld).

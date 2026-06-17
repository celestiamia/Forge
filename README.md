# Forge

Forge is a systems programming language compiled by `forgec` from `.dev` source files.

`forgec` is **self-contained**: it parses `.dev` source and writes native
executables directly, without calling LLVM, clang, NASM, ld, or any other
external toolchain.

## First milestone

- Targets: **x86_64 Linux ELF64** and **x86_32 (i686) Linux ELF32** (hosted,
  statically linked), plus a flat 512-byte **x86 real-mode boot sector**
- Syntax: Python-like indentation, `def`, `let`/`var`, `if`/`elif`/`else`,
  `for`/`while`/`loop`, `match`/`case`, `break`/`continue`, `unsafe`, `extern`,
  `struct`, `as` casts, `import`/`from ... import`
- Emits native machine code + object format directly from Rust (ELF64 or flat binary)
- Working module system: `std.*` imports resolve to `core/<name>.dev`
- Width-correct volatile memory access and a growing standard library
- Bare-metal boot sector written entirely in Forge (no inline assembly required)

## Quickstart

Compile and run the Hello World example:

```bash
cargo build --release
./target/release/forgec examples/hello.dev -o hello --target x86_64-unknown-linux-gnu
./hello
```

Expected output:

```text
Hello, Forge!
```

## Build instructions

```bash
# Build the compiler
cargo build

# Run the test suite
cargo test
```

## Project layout

- `examples/` - Sample Forge programs, including a hosted hello world, a pointer/unsafe demo, stdlib exercises, and a bare-metal bootloader.
- `core/` - Standard library modules imported as `std.*` (e.g. `core/io.dev` is `std.io`).
- `src/lexer/`, `src/parser/`, `src/ast/` - Python-like frontend.
- `src/lower/` - AST to native backend IR lowering.
- `src/backend/` - x86-64, x86-32, and 16-bit assemblers, code generators, and IR.
- `src/obj/` - ELF64/ELF32 executable and flat binary writers.
- `tests/` - Rust-based unit and integration tests.
- `lib/targets/` - Target specification JSON files for future platforms.

## Standard library modules

- `std.io` — `puts`, `putchar`, `getchar`, `rand`, `exit`
- `std.runtime` — `abort`, `exit`
- `std.volatile` — width-correct signed/unsigned loads/stores and memory barriers
- `std.mem` — `copy_bytes`, `set_bytes`, `zero_bytes`, `compare_bytes`
- `std.string` — `strlen`, `strcmp`, `strncmp`
- `std.math` — `abs_i32`, `min_i32`, `max_i32`, `clamp_i32`
- `std.alloc` — bump allocator (`alloc`, `free`) over a 64 KiB compiler-emitted arena
- `std.fmt` — `format_i32` signed 32-bit decimal formatting into a buffer

## Example: `examples/hello.dev`

```dev
package hello

from std.io import puts

pub def main() -> int32:
    puts("Hello, Forge!\n")
    return 0
```

## Example: `examples/bootloader.dev`

```dev
package bootloader

extern def _dev_bios_teletype(c: char) -> void
extern def _dev_serial_putc(c: char) -> void
extern def _dev_load_char(p: ptr[char]) -> char
extern def _dev_halt() -> void

@freestanding
pub def _start() -> void:
    puts("Hello, Forge bootloader!")
    _dev_halt()

def puts(msg: ptr[char]) -> void:
    unsafe:
        var p: ptr[char] = msg
        var c: char = _dev_load_char(p)
        while c != 0:
            _dev_bios_teletype(c)
            _dev_serial_putc(c)
            p = p + 1
            c = _dev_load_char(p)
```

The `x86_16-boot` backend compiles real Forge code (functions, locals,
`while`, pointers, string literals, and arithmetic) directly to a flat
512-byte real-mode boot sector.  Built-in helpers provide BIOS teletype and
serial output so the message is visible under QEMU.  Raw pointer arithmetic
lives inside an `unsafe` block, which the full type checker now enforces:

```bash
./target/release/forgec examples/bootloader.dev -o boot.bin --target x86_16-boot
qemu-system-x86_64 -fda boot.bin -nographic
# After a few seconds the output will contain:
# Booting from Floppy...
# Hello, Forge bootloader!
```

## Supported targets

| Target triple                     | Description                       | Status        |
|-----------------------------------|-----------------------------------|---------------|
| `x86_64-unknown-linux-gnu`        | 64-bit Linux (ELF64)             | Supported     |
| `x86_32-unknown-linux-gnu`        | 32-bit Linux (ELF32, i686)       | Supported     |
| `x86_16-boot`                     | 16-bit x86 real-mode boot sector  | Supported     |
| `x86_64-apple-darwin`             | 64-bit macOS (Mach-O)             | Planned       |
| `x86_64-pc-windows-gnu`           | 64-bit Windows (COFF)             | Planned       |
| `riscv64-unknown-elf`             | 64-bit RISC-V bare metal (ELF)    | Planned       |

## Roadmap

1. x86_64 Linux ELF64 static executables (current).
2. x86_64 Windows PE/COFF.
3. AArch64 macOS Mach-O and Linux ELF64.
4. RISC-V bare metal / Linux.
5. Self-hosting: rewrite `forgec` in `.dev` and bootstrap.

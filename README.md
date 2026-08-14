<p align="center">
  <img src=".github/assets/ForgeBanner.png" alt="Forge Banner" width="800"/>
</p>

<p align="center">
  <a href="https://github.com/miacelestia/Forge/actions/workflows/ci.yml">
    <img src="https://github.com/miacelestia/Forge/actions/workflows/ci.yml/badge.svg" alt="Build Status"/>
  </a>
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"/>
  </a>
  <a href="https://crates.io/crates/forgec">
    <img src="https://img.shields.io/crates/v/forgec.svg" alt="Crates.io Version"/>
  </a>
  <a href="https://github.com/miacelestia/Forge/issues">
    <img src="https://img.shields.io/github/issues/miacelestia/Forge.svg" alt="GitHub Issues"/>
  </a>
</p>

---

# Forge

Forge is a systems programming language compiled by `forgec` from `.dev` source files.

`forgec` is **self-contained**: it parses `.dev` source and writes native
executables directly, without calling LLVM, clang, NASM, ld, or any other
external toolchain.

> **Mascot** — Say hello to Spark! The forge's flame — small but intense, turning raw code into native metal. You'll find Spark throughout the docs and examples, reminding you that Forge compiles *directly* to machine code, no intermediate layers.
>
> <img src=".github/assets/forgemascot.svg" alt="Forge Mascot" width="120" align="right"/>
>
> Spark represents the forge's flame — small but intense, turning raw code into
> native metal. You'll find Spark throughout the docs and examples, reminding
> you that Forge compiles *directly* to machine code, no intermediate layers.

---

## First milestone

- Targets: **x86_64 Linux ELF64** and **x86_32 (i686) Linux ELF32** (hosted,
  statically linked), plus a flat 512-byte **x86 real-mode boot sector**
- Syntax: Python-like indentation, `def`, `let`/`var`, `if`/`elif`/`else`,
  `for`/`while`/`loop`, `match`/`case`, `break`/`continue`, `unsafe`, `extern`,
  `struct`, `as` casts, `import`/`from ... import`
- Emits native machine code + object format directly from Rust (ELF64 or flat binary)
- Working module system: `std.*` imports resolve to `core/<name>.dev`, and
  user-defined modules resolve to local `.dev` files by walking up from the
  entry source directory
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

## Multi-module projects

Forge supports splitting programs across multiple `.dev` source files.  Imports
are resolved relative to the entry file's directory, walking up the tree — the
same search strategy used for `std.*` modules.

### Import syntax

```dev
import myutils              # import all items from myutils.dev
from myutils import helper # import only `helper`
```

All imported modules are recursively loaded and merged into a single
compilation unit before type checking.  This means `forgec` always takes a
single entry file as input; it resolves and pulls in all transitive imports
automatically.

### Example layout

```
examples/multimod/
├── multimod.dev          # entry file — imports utils.dev
└── utils.dev             # user module — imported by multimod.dev
```

`examples/multimod/utils.dev`:

```dev
package utils

from std.io import puts

pub def is_even(n: int32) -> bool:
    return n % 2 == 0

pub def clamp(v: int32, lo: int32, hi: int32) -> int32:
    if v < lo:
        return lo
    if v > hi:
        return hi
    return v
```

`examples/multimod/multimod.dev`:

```dev
package multimod

import utils
from utils import is_even

pub def main() -> int32:
    if is_even(42) && clamp(100, 0, 50) == 50:
        puts("multimod ok\n")
        return 0
    return 1
```

Compile and run:

```bash
./target/release/forgec examples/multimod/multimod.dev -o multimod \
    --target x86_64-unknown-linux-gnu
./multimod
```

### Limitations

- **Flat namespace**: all items from every imported module are merged into a
  single namespace.  Name conflicts between modules are reported as errors.
- **Single entry point**: `forgec` takes one `.dev` file; all other modules are
  pulled in via `import` / `from ... import`.
- **Directory-based module paths**: `import pkg.sub` resolves to
  `pkg/sub.dev` relative to the entry file's directory.

### Makefile template

Because `forgec` resolves all imports at compile time and emits a single
binary, a Makefile only needs to list the entry file:

```makefile
FORGEC = ./target/release/forgec
TARGET = x86_64-unknown-linux-gnu

all: main

main: main.dev
	$(FORGEC) main.dev -o main --target $(TARGET)

clean:
	rm -f main

.PHONY: all clean
```

For build-dependency tracking, list all `.dev` files the entry file imports:

```makefile
main: main.dev utils.dev helpers.dev
	$(FORGEC) main.dev -o main --target $(TARGET)
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

## Roadmap & Ideas

The roadmap has moved to **GitHub Issues** for better tracking and collaboration:

🔗 **[Roadmap & Ideas Tracking Issue](https://github.com/miacelestia/Forge/issues/1)**

This issue contains checklists for:
- Compiler improvements (optimizer, diagnostics, memory management, language features)
- Standard library expansion
- Backend targets (ARM64, RISC-V, WASM, Windows, macOS, bare-metal ARM)
- Developer tooling (LSP, formatter, package manager, build system, debugger)
- Project ideas buildable with Forge (OS kernels, system utilities, web services, games, dev tools, data processing, security, education, creative)

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on:
- Development workflow
- Code style and testing requirements
- Pull request process
- Commit message format

## License

Forge is licensed under the **MIT License** — see [LICENSE](LICENSE) for details.
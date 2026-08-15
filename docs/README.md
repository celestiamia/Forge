# Forge Documentation

Welcome to the official documentation for **Forge** — a systems programming
language with a self-contained compiler (`forgec`) that compiles `.dev` source
files directly to native machine code, with no external toolchain.

## What is Forge?

Forge is a systems programming language designed for direct compilation to
native code. The compiler `forgec` is a single binary that:

- Parses `.dev` source files with Python-like indentation syntax
- Performs type checking and semantic analysis
- Emits native machine code directly (x86-64, x86-32, x86-16)
- Writes ELF64, ELF32, or flat binary (boot sector) executables

There is no LLVM, clang, NASM, or ld involved — `forgec` is written in Rust and
does everything itself.

## Key Features

| Feature | Description |
|---------|-------------|
| **Self-contained compiler** | No external toolchain dependencies |
| **Multiple targets** | x86_64 (ELF64), x86_32 (ELF32), x86_16 (flat 512-byte boot sector) |
| **Python-like syntax** | Indentation-based, `def`/`let`/`var`, `if`/`elif`/`else`, `match`/`case` |
| **Static typing** | Explicit type annotations with inference for literals |
| **Memory control** | Raw pointers, `unsafe`, heap `alloc`/`free`, optional GC on x86_64 |
| **Bare metal** | 512-byte boot sector written entirely in Forge |

## Quick Links

- [Installation](getting-started/installation.md) — How to get `forgec`
- [Quickstart](getting-started/quickstart.md) — Compile your first program
- [Language Syntax](language/syntax.md) — Complete syntax reference
- [Standard Library](stdlib/README.md) — `std.io`, `std.mem`, `std.string`, etc.
- [Targets](targets/README.md) — x86_64, x86_32, x86_16 details
- [Known Issues & Limitations](language/known-issues.md) — What doesn't work yet

## Example: Hello World

```dev
package hello

from std.io import puts

pub def main() -> int32:
    puts("Hello, Forge!\n")
    return 0
```

```bash
# Compile and run
forgec hello.dev -o hello --target x86_64-unknown-linux-gnu
./hello
# Output: Hello, Forge!
```

## Example: Bare-Metal Bootloader

```dev
@freestanding
pub def _start() -> void:
    puts("Hello, Forge bootloader!")
    _dev_halt()
```

```bash
forgec bootloader.dev -o boot.bin --target x86_16-boot
qemu-system-x86_64 -fda boot.bin -nographic
```

## Next Steps

- [Installation](getting-started/installation.md) — Get `forgec` running
- [Quickstart](getting-started/quickstart.md) — First program in 30 seconds
- [Language Reference](language/README.md) — Complete syntax & semantics
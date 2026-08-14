# Forge Documentation

Welcome to the official documentation for **Forge** — a systems programming language with a self-hosting compiler (`forgec`) that compiles `.dev` source files directly to native machine code.

## What is Forge?

Forge is a systems programming language designed for direct compilation to native code without external toolchains (no LLVM, clang, NASM, ld, etc.). The compiler `forgec` is a single binary that:

- Parses `.dev` source files with Python-like indentation syntax
- Performs type checking and semantic analysis
- Emits native machine code directly (x86-64, x86-32, x86-16)
- Writes ELF64, ELF32, or flat binary (boot sector) executables

## Key Features

| Feature | Description |
|---------|-------------|
| **Self-contained compiler** | No external toolchain dependencies |
| **Multiple targets** | x86_64 (ELF64), x86_32 (ELF32), x86_16 (flat 512-byte boot sector) |
| **Python-like syntax** | Indentation-based, `def`/`let`/`var`, `if`/`elif`/`else`, `match`/`case` |
| **Type safety** | Static typing with inference, generics, sum types |
| **Memory control** | Pointers, `unsafe`, manual alloc/free, optional GC |
| **Bare metal** | 512-byte boot sector from pure Forge code |

## Quick Links

- [Installation](getting-started/installation.md) — How to get `forgec`
- [Quickstart](getting-started/quickstart.md) — Compile your first program
- [Language Syntax](language/syntax.md) — Complete syntax reference
- [Standard Library](stdlib/README.md) — `std.io`, `std.mem`, `std.string`, etc.
- [Targets](targets/README.md) — x86_64, x86_32, x86_16 details

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
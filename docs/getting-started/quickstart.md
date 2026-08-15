# Quickstart

Get your first Forge program running in under a minute.

## Prerequisites

- `forgec` installed ([Installation](installation.md))
- Linux x86_64 (native) or cross-compilation setup

## Hello World

Create a file `hello.dev`:

```dev
package hello

from std.io import puts

pub def main() -> int32:
    puts("Hello, Forge!\n")
    return 0
```

## Compile and Run

```bash
# Compile for native x86_64 Linux
forgec hello.dev -o hello --target x86_64-unknown-linux-gnu

# Run
./hello
```

Output:
```
Hello, Forge!
```

## What Just Happened?

| Step | Command | Description |
|------|---------|-------------|
| 1 | `forgec hello.dev` | Parse, type-check, compile |
| 2 | `-o hello` | Output binary name |
| 3 | `--target x86_64-unknown-linux-gnu` | Target triple (ELF64 Linux) |
| 4 | `./hello` | Execute the native binary |

## Understanding the Source

```dev
package hello              # Package name (optional, for namespace)

from std.io import puts    # Import `puts` from standard library

pub def main() -> int32:   # Public function `main` returning 32-bit int
    puts("Hello, Forge!\n") # Call imported function
    return 0               # Return exit code
```

Key syntax points:
- **Indentation matters** — Python-style blocks
- `pub` = public (visible to other modules)
- `def` = function definition
- `-> int32` = return type annotation
- `from ... import` = module imports

## Next: Try the Bootloader

Forge can compile a 512-byte boot sector:

```bash
forgec examples/bootloader.dev -o boot.bin --target x86_16-boot
qemu-system-x86_64 -fda boot.bin -nographic
```

Output:
```
SeaBIOS...
Booting from Floppy...
Hello, Forge bootloader!
```

## Common Flags

```bash
forgec --help                    # Show all options
forgec --version                 # Show version
forgec -o <output> <input.dev>   # Specify output
forgec --target <triple> <input> # Target: x86_64, x86_32, x86_16-boot
forgec --freestanding <input>    # No stdlib, custom entry point
```

## Common Issues

| Problem | Solution |
|---------|----------|
| `command not found: forgec` | [Install forgec](installation.md) or add to PATH |
| `Error: parse error in ...: unknown identifier ...` | Check module imports — the namespace is flat; every name must be imported or defined |
| `Error: parse error in ...: expected X, found Y` | Check indentation (4 spaces, no tabs) and syntax |
| `type checking failed: ...` | Type mismatch — use explicit `as` casts (no implicit conversions) |
| Compilation fails on x86_32 with float errors | The x86_32 target does not support `float32`/`float64` — avoid them and modules that use them |

## Next Steps

- [Building from Source](building.md) — Compile `forgec` yourself
- [Language Syntax](language/syntax.md) — Complete syntax reference
- [Examples](examples/README.md) — More sample programs
# CLI Reference

Reference for the `forgec` command-line interface.

## Usage

```bash
forgec [OPTIONS] <SOURCE>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<SOURCE>` | Path to the `.dev` source file (required) |

## Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--target <TRIPLE>` | `-t` | Target triple | `x86_64-unknown-linux-gnu` |
| `--output <PATH>` | `-o` | Output file path | `<source>` with extension stripped |
| `--freestanding` | | No hosted runtime; custom entry point (`_start`) | off |
| `--linker <PATH>` | | Custom target config (`.fld` linker descriptor) instead of a built-in target | built-in |
| `--help` | `-h` | Print help | |
| `--version` | `-V` | Print version | |

### Supported Targets

| Target triple | Description |
|---------------|-------------|
| `x86_64-unknown-linux-gnu` | 64-bit Linux (ELF64) — also `native` |
| `x86_32-unknown-linux-gnu` | 32-bit Linux (ELF32) |
| `x86_16-boot` | 16-bit real-mode boot sector (flat 512-byte binary) |

`--linker` accepts a custom target described by a `.fld` (Forge Linker
Descriptor) file — see the [`.fld` format reference](../targets/fld-format.md)
and the ready-made scripts in `examples/targets/`.  The three triples above are
the built-in presets.  On x86_16, a `.fld` can also select `FORMAT raw` for
bare multi-stage images (no boot signature) and set the stage's `LOAD`
address — see the [ForgeOS example](../examples/os.md).

## Examples

### Basic Compilation

```bash
# Compile hello world (default x86_64)
forgec hello.dev -o hello

# Explicit target
forgec hello.dev -o hello --target x86_64-unknown-linux-gnu

# Cross-compile to x86_32
forgec hello.dev -o hello32 --target x86_32-unknown-linux-gnu

# Boot sector
forgec bootloader.dev -o boot.bin --target x86_16-boot
```

### Freestanding Mode

```bash
# No hosted runtime, custom _start entry
forgec kernel.dev -o kernel.bin --freestanding --target x86_16-boot
```

`x86_16-boot` is implicitly freestanding (boot-sector format with a
`_start` entry); `--freestanding` is only needed to run bare-metal code
on a hosted target (e.g. `--target x86_64-unknown-linux-gnu`).

### Version & Help

```bash
forgec --version
# forgec 0.1.0

forgec --help
```

## Output Files

| Target | Output | Description |
|--------|--------|-------------|
| x86_64 | executable | ELF64, statically linked |
| x86_32 | executable | ELF32, statically linked |
| x86_16 | `.bin` | Flat binary, 512 bytes, 0x55AA signature |

## Compiler Phases

```
forgec input.dev -o output
        │
        ▼
┌──────────────────┐
│ 1. Lexing        │  Tokens with positions
├──────────────────┤
│ 2. Parsing       │  AST (abstract syntax tree)
├──────────────────┤
│ 3. Imports       │  Recursive module loading + merge
├──────────────────┤
│ 4. Type checking │  Typed AST + semantic analysis
├──────────────────┤
│ 5. Lowering      │  AST → IR
├──────────────────┤
│ 6. Codegen       │  Machine code (per target)
├──────────────────┤
│ 7. Object write  │  ELF64 / ELF32 / flat binary
└──────────────────┘
```

## Diagnostics

Errors are reported as plain text with file, line, and column:

```text
Error: parse error in bad.dev: 5:12: expected expression, found Something

type checking failed:
  bad.dev: expected `i32`, found `ptr[char]`
```

See [Diagnostics](diagnostics.md).

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Compilation error |

A Rust panic in the compiler (see [Known Issues](../language/known-issues.md))
aborts with a non-zero exit and a backtrace if `RUST_BACKTRACE=1` is set.

## Configuration

No configuration files — all options are passed on the command line.
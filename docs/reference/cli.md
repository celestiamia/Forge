# CLI Reference

Complete reference for the `forgec` command-line interface.

## Usage

```bash
forgec [OPTIONS] <SOURCE> [-o <OUTPUT>] [--target <TARGET>]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<SOURCE>` | Path to `.dev` source file (required) |

## Options

### Output

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `-o`, `--output` | `-o` | Output file path | `<source>` without extension |

### Target

| Option | Description | Default |
|--------|-------------|---------|
| `--target <TRIPLE>` | Target triple | Host native |

**Supported Targets:**
- `x86_64-unknown-linux-gnu` - 64-bit Linux (ELF64)
- `x86_32-unknown-linux-gnu` - 32-bit Linux (ELF32)
- `x86_16-boot` - 16-bit boot sector (flat binary)

### Mode

| Option | Description |
|--------|-------------|
| `--freestanding` | No runtime, custom entry point (`_start`) |

### Other

| Option | Short | Description |
|--------|-------|-------------|
| `--help` | `-h` | Show help message |
| `--version` | `-V` | Show version |
| `--emit-asm` | | Emit assembly instead of object |

## Examples

### Basic Compilation

```bash
# Compile hello world (native x86_64)
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
# No runtime, custom _start entry
forgec kernel.dev -o kernel --freestanding --target x86_64-unknown-linux-gnu
```

### Version & Help

```bash
forgec --version
# forgec 0.1.0 (abc1234)

forgec --help
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Compilation error |
| 2 | Invalid arguments |
| 3 | I/O error |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_BACKTRACE` | Enable backtraces on panic (1=short, full=full) |
| `FORGEC_CACHE` | Cache directory (default: `~/.cache/forgec`) |

## File Extensions

| Extension | Description |
|-----------|-------------|
| `.dev` | Forge source file |
| `.fld` | Forge Linker Descriptor |

## Output Files

| Target | Output | Description |
|--------|--------|-------------|
| x86_64 | `.o` / executable | ELF64 |
| x86_32 | `.o` / executable | ELF32 |
| x86_16 | `.bin` | Flat binary (512 bytes) |

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
│ 3. Semantic      │  Typed AST + type checking
├──────────────────┤
│ 4. Lowering      │  IR (intermediate representation)
├──────────────────┤
│ 5. Codegen       │  Machine code (per target)
├──────────────────┤
│ 6. Encoding      │  Machine code → bytes
├──────────────────┤
│ 7. Object Write  │  ELF / Flat binary
└──────────────────┘
```

## Diagnostics

Errors include:
- File, line, column
- Error code (E001, E002, ...)
- Human-readable message
- Source snippet with caret

```bash
$ forgec bad.dev
error[E0308]: mismatched types
 --> bad.dev:5:12
  |
5 |     let x: int32 = "hello"
  |            ^^^^^^^ expected int32, found ptr[char]
```

## Configuration

No configuration files - all options via CLI.

## Cache

Compiler caches:
- Parsed stdlib modules (`~/.cache/forgec/stdlib/`)
- Incremental compilation not yet implemented

## Shell Completion

Not yet implemented. Planned for bash/zsh/fish.
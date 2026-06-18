# forgec — AGENTS.md

## Build & test

```sh
cargo build                # debug build
cargo build --release      # recommended for running compiled Forge programs
cargo test                 # full suite (unit + integration)
```

No formatter, linter, or typechecker config exists. No pre-commit hooks.

## CLI usage

```sh
# Compile and run the default x86_64 target
./target/release/forgec examples/hello.dev -o hello --target x86_64-unknown-linux-gnu
./hello

# 32-bit Linux
./target/release/forgec examples/hello.dev -o hello --target x86_32-unknown-linux-gnu

# 16-bit real-mode boot sector (512 bytes, 0x55AA signature)
./target/release/forgec examples/bootloader.dev -o boot.bin --target x86_16-boot
qemu-system-x86_64 -fda boot.bin -nographic
```

`-o` defaults to `<source>` with extension stripped. `--freestanding` flag available.

## Architecture

Source in `src/`. Single Rust crate (`forgec`), edition 2024.

**Pipeline:** `lexer/` → `parser/` → `sema/` (type check) → `lower.rs` (AST→IR) → `backend/` (codegen) → `obj/` (ELF/flat writer)

`mir/` directory is empty (not yet wired).

**Target smux** in `driver/mod.rs:80-90`:
- `x86_64-unknown-linux-gnu` / `native` → hosted, x86_64, ELF64
- `x86_32-unknown-linux-gnu` → hosted, x86_32, ELF32
- `x86_16-boot` → freestanding, x86_16, flat

## Forge language quirks

- Python-like indentation; `.dev` extension
- Type names are dual-spelled: `i32`/`int32`/`int`, `u32`/`uint32`/`uint`, `byte`/`u8`, `f64`/`float64`/`float`. Single table in `ty/prims.rs` governs both sema and lowerer.
- `let` = immutable, `var` = mutable
- `pub def main()` → mangled to `_forge_main` in hosted targets (runtime `_start` calls it)
- Hosted runtime helpers (`_dev_puts`, `_dev_exit`, etc.) declared `extern` in `core/*.dev`; the compiler emits them in `backend/codegen.rs`
- `@freestanding` attribute bypasses hosted runtime requirements

## Module system

`std.<name>` imports resolve to `core/<name>.dev` by walking up from source directory. Only `std.*` modules are supported. See `driver/loader.rs`.

Stdlib modules: `io`, `runtime`, `volatile`, `mem`, `string`, `math`, `alloc`, `fmt`.

## Testing quirks

- Integration tests compile examples to temp dirs, run the native binaries, and check stdout/exit code
- Bootloader test spawns QEMU (`qemu-system-x86_64` must be on PATH), sleeps 2s, then kills it
- `getchar` and `guess` tests write to stdin
- `tests/lexer_tests.rs` and `tests/parser_tests.rs` contain only placeholder tests

## Codegen notes

- Local variables are 64-bit stack slots regardless of declared type
- Width-correct load/store (8/16/32-bit) only used for pointer deref and field access
- No optimizer — every unsafe deref emits a real memory access (effectively volatile)
- Struct fields are laid out sequentially without padding in the first milestone
- x86_64: System V AMD64 ABI (args in RDI, RSI, RDX, RCX, R8, R9)

## Inline assembly

`asm!()` template accepted by the parser; x86_16 backend assembles verbatim template text. Other targets error at codegen.

## What not to touch

- `mir/` is scaffolding for future work; not wired into the pipeline
- `ty/prims.rs` primitive_kind() is the **single source of truth** for type names — sema and lowerer both route through it
- Type names `i128`/`uint128` are intentionally unmapped (no backend supports them)
- `.gitignore` comments warn: do NOT gitignore bare `core` (would exclude the `core/` stdlib dir)

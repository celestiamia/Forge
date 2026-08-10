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

**Pipeline:** `src/lexer/` → `src/parser/` → `src/sema/` (type check) → `src/lower/` (AST→IR) → `src/backend/codegen/` (codegen) → `src/backend/ir.rs` (IR) → `src/obj/` (ELF/flat writer)

`src/mir/` directory is empty (not yet wired).

**Module structure:** Single Rust crate (`forgec`), edition 2024, in `src/`.

**Type system:** `src/ty/mod.rs` defines the `Type` enum (not `ty/prims.rs`). `src/backend/ir.rs` defines a parallel IR `Type` enum and `BinOp` enum with methods like `is_integer()`, `is_float()`, `is_signed()`.

**Target classification** in `src/driver/mod.rs:79`:
- `x86_64-unknown-linux-gnu` / `native` → hosted, x86_64, ELF64
- `x86_32-unknown-linux-gnu` → hosted, x86_32, ELF32 (no float support)
- `x86_16-boot` → freestanding, x86_16, flat

## Forge language quirks

- Python-like indentation; `.dev` extension
- Type names are dual-spelled: `i32`/`int32`/`int`, `u32`/`uint32`/`uint`, `byte`/`u8`, `f64`/`float64`/`float`. Single table in `ty/mod.rs` governs both sema and lowerer.
- `let` = immutable, `var` = mutable
- `pub def main()` → mangled to `_forge_main` in hosted targets (runtime `_start` calls it)
- Hosted runtime helpers (`_dev_puts`, `_dev_exit`, etc.) declared `extern` in `core/*.dev`; the compiler emits them in `src/backend/codegen/runtime.rs`
- `@freestanding` attribute bypasses hosted runtime requirements
- Power operator (`**`) requires integer operands; desugars to `__forge_pow` runtime call (loop-based, integer-only)
- Floor division (`//`) is floor-toward-negative-infinity; floor division by zero panics at runtime
- `unsafe` blocks bypass pointer safety checks
- `as` is a postfix cast operator in the expression grammar

## Module system

`from std.<name> import ...` resolves to `core/<name>.dev` by walking up from source directory. Only `std.*` modules are supported. See `src/driver/loader.rs`.

Stdlib modules: `io`, `runtime`, `volatile`, `mem`, `string`, `math`, `alloc`, `fmt`.

## Testing quirks

- Integration tests compile examples to temp dirs, run the native binaries, and check stdout/exit code
- Bootloader test spawns QEMU (`qemu-system-x86_64` must be on PATH), sleeps 2s, then kills it
- `getchar` and `guess` tests write to stdin
- `tests/lexer_tests.rs` and `tests/parser_tests.rs` contain only placeholder tests
- Importing any `std.*` module compiles the **entire** module file — x86_32 tests fail if the stdlib uses `float64` (not supported on x86_32)
- `obj::tests::tiny_static_elf*` tests can occasionally fail due to test parallelism; run isolation if needed
- Example `.dev` files are also integration test fixtures — adding a new example used as a test fixture is fine

## Codegen notes

- Local variables are 64-bit stack slots regardless of declared type
- Width-correct load/store (8/16/32-bit) only used for pointer deref and field access
- No optimizer — every unsafe deref emits a real memory access (effectively volatile)
- Struct fields are laid out sequentially without padding in the first milestone
- x86_64: System V AMD64 ABI (args in RDI, RSI, RDX, RCX, R8, R9)
- Float values are stored as 64-bit integer bit patterns in 64-bit slots/RAX
- Integer-to-float and float-to-integer casts use **XMM7** (scratch), not XMM0 — XMM0 is used by `eval_float_bin` for binary operations and will be clobbered
- Parser `parse_type` and `parse_type_atom` call `skip_newlines()` internally; use `parse_type_noskip()` from the `as` handler in `parse_postfix` to avoid consuming newlines that the postfix loop needs to see

## Inline assembly

`asm!()` template accepted by the parser; x86_16 backend assembles verbatim template text. Other targets error at codegen.

## What not to touch

- `src/mir/` is scaffolding for future work; not wired into the pipeline
- `src/ty/mod.rs` `Type` enum is the source of truth for type names — sema and lowerer both route through it
- Type names `i128`/`uint128` are intentionally unmapped (no backend supports them)
- `.gitignore` comments warn: do NOT gitignore bare `core` (would exclude the `core/` stdlib dir)

## Parser gotchas

- `as` casts in `var` or assignment declarations inside `while`/`else` blocks can trigger parser bugs — prefer `var x: int64 = expr` then `(x as int32)` in expressions, or hoist `var` declarations outside loops
- Consecutive `var` declarations in while-loop bodies inside `unsafe` blocks are fragile

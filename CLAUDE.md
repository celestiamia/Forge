# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working on the Forge compiler and standard library.

## Project overview

`forgec` is a self-contained compiler for the Forge systems programming language. It reads `.dev` source files and writes native x86_64 Linux ELF64 executables directly, without calling LLVM, clang, NASM, ld, or any other external toolchain.

Only **x86_64-unknown-linux-gnu** is supported in the current milestone. macOS, Windows, and RISC-V are planned.

## Common commands

Build the compiler:

```bash
cargo build
```

Build release binary:

```bash
cargo build --release
```

Run the full test suite:

```bash
cargo test
```

Run only the integration tests (these verify that `.dev` sources compile to working native binaries):

```bash
cargo test --test integration
```

Run a single test by name:

```bash
cargo test --test integration hello_dev_compiles_and_runs
```

Compile and run the hello-world example:

```bash
./target/release/forgec examples/hello.dev -o hello --target x86_64-unknown-linux-gnu
./hello
```

Run the rock-paper-scissors game:

```bash
./target/release/forgec examples/rps.dev -o rps --target x86_64-unknown-linux-gnu
printf 'r\np\ns\n' | ./rps
```

Compile and run the `match`/`break`/`continue` example:

```bash
./target/release/forgec examples/match_bc.dev -o match_bc --target x86_64-unknown-linux-gnu
./match_bc   # prints "thirteen"
```

Compile and run the new stdlib examples:

```bash
./target/release/forgec examples/volatile.dev -o volatile --target x86_64-unknown-linux-gnu
./volatile
./target/release/forgec examples/mem.dev -o mem --target x86_64-unknown-linux-gnu
./mem
./target/release/forgec examples/string.dev -o string --target x86_64-unknown-linux-gnu
./string
./target/release/forgec examples/math.dev -o math --target x86_64-unknown-linux-gnu
./math
```

Compile and run the bump-allocator + formatter example on both hosted targets:

```bash
./target/release/forgec examples/bump_fmt.dev -o bump_fmt64 --target x86_64-unknown-linux-gnu
./bump_fmt64
# 0
# 42
# -7
# 100
# 2147483647
# -2147483647

./target/release/forgec examples/bump_fmt.dev -o bump_fmt32 --target x86_32-unknown-linux-gnu
./bump_fmt32   # same output, i686 ELF32 executable
```

Compile and run the bare-metal bootloader example (output is visible in the
emulated display / terminal):

```bash
./target/release/forgec examples/bootloader.dev -o boot.bin --target x86_16-boot
qemu-system-x86_64 -fda boot.bin -nographic
# After a few seconds the output will contain:
# Booting from Floppy...
# Hello, Forge bootloader!
```

Inspect a generated binary:

```bash
objdump -d hello
```

## High-level architecture

The compiler pipeline is:

```text
.dev source
  → lexer (src/lexer/)
  → parser (src/parser/)
  → AST (src/ast/)
  → module loader / import merger (src/driver/loader.rs)
  → semantic analyzer (src/sema/check.rs) + type system (src/ty/mod.rs)
  → lowerer (src/lower.rs)
  → backend IR (src/backend/ir.rs)
  → x86-64 code generator (src/backend/codegen.rs)
  → ELF64 writer (src/obj/elf.rs)
  → native executable

The hosted x86_32 target forks the back half of the pipeline: the same IR is
consumed by `src/backend/codegen32.rs` (32-bit cdecl codegen) using the
`src/backend/x86/` 32-bit encoder, then written by `src/obj/elf32.rs` as an
ELF32 i686 executable.

For the `x86_16-boot` target the IR is consumed by a separate 16-bit
real-mode backend (`src/backend/codegen16.rs`) that emits a flat 512-byte
boot sector directly.
```

Key components:

- **`src/lexer/lexer.rs`** — Tokenizes `.dev` source, including indentation-based block handling.
- **`src/parser/parser.rs`** — Recursive-descent parser producing the AST in `src/ast/mod.rs`.
- **`src/driver/loader.rs`** — Resolves `std.*` imports (e.g. `std.io` → `core/io.dev`) and merges imported public items into the compilation unit.
- **`src/sema/check.rs`** — Full semantic analyzer: name resolution, type inference/checking, mutability, unsafe rules, generic monomorphization metadata.
- **`src/ty/mod.rs`** — Forge type system with interning (`Type`, `TypeCtx`).
- **`src/lower.rs`** — Lowers the Python-like AST to the backend IR. This is the pragmatic bridge used by the current milestone. `match` desugars to an if-chain; `match` expressions lower to a zero-init temp plus an if-chain inside a `Block`.
- **`src/backend/ir.rs`** — Small typed IR close to the machine model: functions, statements, expressions, types. Includes `Break`/`Continue` and `ExprKind::Block`.
- **`src/backend/x64/`** — Hand-written x86-64 instruction encoder (registers, ModR/M, SIB, REX prefixes, immediates, labels, jump fixups).
- **`src/backend/x86/`** — Hand-written 32-bit x86 encoder (EAX..EDI, 32-bit ModR/M, no REX) shared by the `x86_32-unknown-linux-gnu` target.
- **`src/backend/codegen.rs`** — Emits machine code from the IR for the x86_64 target. In hosted mode it also emits a tiny runtime (`_start`, `_dev_write`, `_dev_puts`, `_dev_getchar`, `_dev_putchar`, `_dev_rand`, `_dev_alloc`, `_dev_free`, `_dev_exit`) using Linux syscalls; `_dev_alloc`/`_dev_free` are only emitted when the program imports a module that declares them (e.g. `std.alloc`).
- **`src/backend/codegen32.rs`** — 32-bit cdecl codegen for the `x86_32-unknown-linux-gnu` target (args pushed right-to-left, return in EAX, caller cleans up). Emits the same hosted runtime as the x86_64 backend.
- **`src/backend/codegen16.rs`** — 16-bit real-mode backend for the `x86_16-boot` target. Emits x86 machine code for a BP-based stack frame, supports locals, control flow, pointer loads/stores, arithmetic, and string literals. Built-ins (`_dev_bios_teletype`, `_dev_serial_putc`, `_dev_load_char`, `_dev_halt`) provide testable output without inline assembly.
- **`src/obj/elf.rs`** — Hand-written ELF64 executable writer. Produces RX `.text`/`.rodata` and RW `.data` PT_LOAD segments.
- **`src/obj/elf32.rs`** — Hand-written ELF32 (i686) executable writer with a 64 KiB `.bss` arena placed just past `.data` in the virtual address space.
- **`src/driver/mod.rs`** — Compiler driver. Reads source, runs parse → load imports → type-check → lower → codegen → ELF write, and sets the output executable bit.
- **`src/main.rs`** — CLI entry point using `clap`.

## Active vs. legacy code

The earlier LLVM-text-generation path has been removed. Files that were previously unused leftovers are no longer in the repository.

- `src/sema/` and `src/ty/` — Full semantic analyzer and type system, now wired into `driver::compile`. Every program is type-checked before lowering; errors are reported with file and message.

When adding new language features, prefer extending the active path:

1. Update `src/ast/mod.rs` if the syntax changes.
2. Update `src/parser/parser.rs` to parse it.
3. Update `src/sema/check.rs` and `src/ty/mod.rs` for type-checking rules.
4. Update `src/lower.rs` to emit the corresponding backend IR.
5. Update `src/backend/ir.rs` if a new IR construct is needed.
6. Update `src/backend/codegen.rs` and/or `src/backend/x64/` to emit machine code. If the feature is target-agnostic, mirror the change in `src/backend/codegen32.rs` (and `src/backend/x86/` for 32-bit encoding) so the `x86_32-unknown-linux-gnu` target stays in parity.
7. Update `core/` and `examples/` to exercise the feature.

## First-milestone syntax and limitations

Supported:

- `package`, `import`, `from ... import`
- `extern def`, `pub def`, `let`/`var`, `return`
- `if`/`elif`/`else`, `while`, `for i in a..b`, `loop`, `match`/`case`/`_`, `break`/`continue`
- `unsafe { ... }` blocks
- integer arithmetic and comparisons, logical `&&`/`||`
- `as` casts between numeric types (narrowing keeps the low bits)
- function calls, pointers (`ptr[T]`), address-of (`&x`), dereference (`*p`), struct field access
- `struct` definitions with brace or indentation syntax

Imported standard library modules:

- `std.io` — `puts`, `putchar`, `getchar`, `rand`, `exit` (see `core/io.dev`)
- `std.runtime` — `abort`, `exit` (see `core/runtime.dev`)
- `std.volatile` — width-correct signed/unsigned loads and stores plus memory barriers (see `core/volatile.dev`)
  - `load_u8/u16/u32/u64`, `load_i8/i16/i32/i64`, `load_ptr`
  - `store_u8/u16/u32/u64`, `store_i8/i16/i32/i64`, `store_ptr`
  - `read_barrier`, `write_barrier`, `full_barrier`
- `std.mem` — `copy_bytes`, `set_bytes`, `zero_bytes`, `compare_bytes` (see `core/mem.dev`)
- `std.string` — `strlen`, `strcmp`, `strncmp` (see `core/string.dev`)
- `std.math` — `abs_i32`, `min_i32`, `max_i32`, `clamp_i32` (see `core/math.dev`)
- `std.alloc` — bump allocator over a 64 KiB compiler-emitted arena: `alloc(size) -> ptr[char]`, `free(p)` (no-op); backed by the `_dev_alloc`/`_dev_free` runtime helpers (see `core/alloc.dev`)
- `std.fmt` — integer formatting: `format_i32(buf, value) -> uint64` writes a signed 32-bit decimal into `buf` (null-terminated) and returns the length; `INT32_MIN` is not handled (see `core/fmt.dev`)

Supported targets and output formats:

- `x86_64-unknown-linux-gnu` / `native` — ELF64 x86-64 Linux executable
- `x86_32-unknown-linux-gnu` — hosted ELF32 i686 Linux executable (cdecl ABI, separate 32-bit encoder in `src/backend/x86/`, codegen in `src/backend/codegen32.rs`, ELF32 writer in `src/obj/elf32.rs`)
- `x86_16-boot` — flat 512-byte x86 real-mode boot sector

Not yet lowered to machine code (parsed but will fail in `src/lower.rs` or `src/backend/codegen.rs`):

- inline `asm` on the x86_64 hosted target
- `sizeof` / `offsetof`
- slices, arrays as values
- floating point

## Testing tips

- `cargo test --test integration` is the fastest end-to-end check.
- The full `cargo test` suite now passes.
- To debug generated code, use `objdump -d <binary>`.

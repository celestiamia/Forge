# Roadmap & Ideas

This is a living, preliminary list of features and improvements for Forge.
The items below are a starting point for discussion — **these ideas may
fluctuate and change over time** as Forge matures, priorities shift, and the
implementation approach evolves. Checkbox status (`/ [ ]`) tracks general
interest, not hard commitments.

Open to feedback: if something is missing, mis-prioritized, or already
started, please open a pull request or discussion.

## How to read this

- **Theme** = a feature area or milestone focus.
- **[x]** = actively in progress / recently tackled. Items I just landed (Tier 1
  stability work) are checked off as reference for what's already done.
- Priority ordering inside each theme is suggestive, not strict.

## Recently completed

These were tackled to stabilize the first milestone; recorded here so we don't
re-litigate them:

- [x] Compiler panics → clean diagnostics: generic functions / structs, `impl`
  blocks, nested struct fields, `int128`/`uint128`, recursive-by-value structs
      ([`docs/language/known-issues.md`](./docs/language/known-issues.md) for
      current status)
- [x] Postfix continuation across newlines no longer swallows statements that
      start with `(`, `[`, `{`
- [x] `parse_type_noskip()` for `as` casts
- [x] `__forge_pow` (integer power `**`) + floor division `//`
- [x] `@freestanding`, `.fld` linker descriptors, ForgeOS example + x86_16
       multi-stage boot
- [x] **Block expressions** `let x = { ... }` and `unsafe { ... }` as expressions —
       end-to-end across all three targets (x86_64, x86_32, x86_16)
- [x] **Enum variants** end-to-end — construction (`Color.Red`,
       `Option.Some(x)`), `match`/`case` on variant values, payload
       destructuring, and discriminants across x86_64 and x86_32

## Compiler stability & diagnostics

- [ ] Source spans on every error (file:line:col), not just sema
- [ ] Consistent diagnostic format; color/TTY detection
- [ ] `--emit-ir` / `--dump-ast` flags for compiler introspection
- [ ] `cargo-fuzz` harness for the parser/lexer to shake out remaining panics
- [ ] `--quiet` / exit status semantics for non-hosted/freestanding builds
- [ ] Stable `--help` / `--version`

## Language completeness (parsed but not yet functional)

These are accepted by the grammar/type-checker today but rejected at lowering
or codegen:

- [ ] **Tuples** — literals, types, destructuring, returns
- [ ] **Compound assignment** `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`,
       `<<=`, `>>=`
- [ ] **Fixed-size array types** `[T; N]` and repeat literals `[0; N]`
- [ ] **`@packed` / `@align(N)`** actually affect struct layout
- [ ] **Slices** `slice[T]`, `&arr[..]`, `&arr[1..3]`
- [ ] **Function types** `fn(...) -> ...` (and function-pointer values)
- [ ] **`refmut[T]` / `&mut x`**, **`own[T]`** ownership model
- [ ] **Byte-string literals** `b"..."`
- [ ] **`@export`, `@inline`, `@noreturn`, `@naked`** attributes
- [ ] **Generic functions & structs** (monomorphization exists in sema but is
      not yet wired to lowering/codegen)

## Generics & ADT evolution

- [ ] Finish generics: monomorphize at lowering, instantiate per call site,
      support generic methods on `impl` blocks
- [ ] Default type parameters, associated types
- [ ] Trait/`interface` system (method resolution on `impl`) — method calls
      are not yet lowered even though impls type-check
- [ ] Struct update syntax / `..rest`

## Optimizer & codegen quality

- [ ] **Optimizer passes** on the IR — the pipeline is effectively `-O0`; every
      unsafe deref is a real memory access:
  - constant folding (incl. `**` / `//`)
  - common subexpression elimination
  - dead-store / dead-code elimination
  - trivial copy propagation
- [ ] Struct field padding/alignment (currently sequential, no padding)
- [ ] `--freestanding` flag available on all targets
- [ ] Function args beyond the ABI register limit (>6 for x86_64; >1 for
      x86_16) currently rejected
- [ ] `asm!()` for x86_64 / x86_32 (x86_16 only today)
- [ ] Float support on **x86_32** (currently x86_64-only)
- [ ] Struct-by-value returns & copies on x86_32 (currently truncates to first
      slot) — see `docs/language/known-issues.md`

## Standard library

- [ ] `std.gc` on x86_32 / 16 (x86_64-only today)
- [ ] `std.alloc` on x86_32: a real `free` instead of a no-op bump allocator
- [ ] Bounds checking in `std.string` (null-terminated, unchecked)
- [ ] Deterministic `rand` (currently a 31-bit LCG) + better PRNG
- [ ] Qualified module access `mymodule.foo()` / re-exports
- [ ] `std.io`: `read` into buffers, buffered I/O
- [ ] More `std.fmt` formatters (width/precision, `%x`, `%b`)
- [ ] `getchar` non-blocking / EOF handling improvements

## Module system

- [x] `from std.<name> import ...` (loads `core/<name>.dev`)
- [ ] **Qualified access** (`std.io.puts`) — flat namespace today
- [ ] Re-exports / `pub use`
- [ ] Versioning / conditional compilation gates (`@cfg`-like)
- [ ] Local (non-`std.*`) module support is already present; formalize it

## Targets

- [x] `x86_64-unknown-linux-gnu` (hosted, ELF64)
- [x] `x86_32-unknown-linux-gnu` (hosted, ELF32)
- [x] `x86_16-boot` / raw stage images (flat, real-mode)
- [ ] Custom targets via `.fld` (parsed) — apply `MEMORY`/`SECTIONS` to layout
- [ ] macOS `x86_64-apple-darwin` (Mach-O) — **planned**
- [ ] Windows `x86_64-pc-windows-gnu` (COFF) — **planned**
- [ ] `riscv64-unknown-elf` — **planned**
- [ ] `aarch64-unknown-linux-gnu`, `wasm32-unknown-unknown` — **planned**

> Note: the `lib/targets/*.json` rustc-style target files were removed; targets
> are configured through builtin presets ([`src/linker/config.rs`](./src/linker/config.rs))
> or `.fld` linker descriptors ([`docs/targets/fld-format.md`](./docs/targets/fld-format.md)).

## Environment & portability

- [ ] macOS and Windows host builds (`forgec` currently builds/runs on Linux
      x86_64; i686 hosts for x86_32 output)
- [ ] QEMU-based bootloader tests require `qemu-system-x86_64` on PATH; make
      them opt-out / auto-skip gracefully where missing

## Tooling

- [ ] Language Server Protocol (LSP) server for editor integration
- [ ] Syntax highlighting / tree-sitter grammar
- [ ] `cargo test --profile=release` CI path for faster integration tests
- [ ] CI matrix: build+test across x86_64, x86_32, x86_16 (where tools exist)
- [ ] Test isolation hardening (some ELF tests still flake under parallel runs)

---

*None of this is a commitment. Priorities are shaped by what the compiler needs
to become self-hosting, what the stdlib gaps are, and where contributors want
to dig in. Suggest a change any time.*

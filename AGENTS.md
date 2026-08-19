# forgec — AGENTS.md

## Build & test

```sh
cargo build                # debug build
cargo build --release      # recommended for running compiled Forge programs
cargo test                 # full suite (unit + integration)
cargo test --test integration              # integration tests only (fastest end-to-end check)
cargo test --test integration <name>       # single integration test by name
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

# Custom target via a .fld linker descriptor (see docs/targets/fld-format.md;
# ready-made scripts in examples/targets/)
./target/release/forgec examples/hello.dev -o hello --linker examples/targets/x86_64-linux.fld
```

`-o` defaults to `<source>` with extension stripped. `--freestanding` flag available.

`.fld` (Forge Linker Descriptor): `ARCH`/`FORMAT`/`HOSTED`/`ENTRY`/`LOAD`/`HEAP`/`MEMORY`/`SECTIONS`/`RUNTIME` in `src/linker/`. Honored: ARCH, FORMAT (elf/elf32/flat/raw; flat = 512-byte boot sector with 0x55AA, raw = bare image for multi-stage loads), HOSTED, ENTRY, LOAD (x86_16 load address, default 0x7C00, drives imm16 string fixups; for x86_32 `FORMAT raw` it is the kernel link base, default 0x100000, and is where the stage-2 loader places the image — see `examples/os32`), HEAP (GC arena on x86_64 / free-list heap on x86_32, default 4 MiB), RUNTIME float gate. MEMORY/SECTIONS are parsed but not yet applied to layout. Helper emission stays reference-driven (importing a `_dev_*` symbol emits it), not flag-driven.

## Architecture

Single Rust crate (`forgec`), edition 2024, source in `src/`.

**Pipeline:** `src/lexer/` → `src/parser/` → `src/ast/` → `src/driver/loader.rs` (import merge) → `src/sema/` (type check) → `src/lower/` (AST→IR) → `src/backend/codegen/` (x86_64 codegen) → `src/obj/` (ELF/flat writer)

`src/mir/` does not exist (future scaffolding only).

**Backend layout:**
- `src/backend/ir.rs` — IR definition (`Type`, `BinOp`, `Expr`, `Stmt`, `Program`); shared by lower + all codegen backends
- `src/backend/codegen/` — x86_64 code generator, split across `mod.rs` (entry/frame), `expr.rs`, `runtime.rs`, `gc.rs`, `layout.rs`
- `src/backend/codegen32/` — x86_32 codegen (cdecl ABI)
- `src/backend/codegen16/` — x86_16 real-mode codegen
- `src/backend/x64/` — x86_64 encoder (REX, ModR/M, SIB)
- `src/backend/x86/` — x86_32 encoder (no REX)
- `src/backend/x16/` — x86_16 encoder
- `src/obj/` — `elf.rs` (ELF64), `elf32.rs` (ELF32), `flat.rs` (boot sector)

**Type system:** `src/ty/mod.rs` defines the `Type` enum (no `ty/prims.rs` exists). Sema (`src/sema/check/typing.rs`) maps dual-spelled names to it. `src/backend/ir.rs` defines a parallel IR `Type` enum with `is_integer()`, `is_float()`, `is_signed()`.

**Target classification** in `src/linker/config.rs` (`builtin_target`), resolved in `src/driver/mod.rs:57`:
- `x86_64-unknown-linux-gnu` / `native` → hosted, x86_64, ELF64
- `x86_32-unknown-linux-gnu` → hosted, x86_32, ELF32 (no float support)
- `x86_16-boot` → freestanding, x86_16, flat
 - `x86_32` raw binary → freestanding, x86_32, `FORMAT raw` (boot-to-32-bit chain, e.g. `examples/os32`; `LOAD` is the kernel link base, default `0x100000`)
- `x86_64` raw binary → freestanding, x86_64, `FORMAT raw` (boot-to-64-bit chain, e.g. `examples/os64`; `LOAD` is the kernel link base, default `0x100000`)

## Forge language quirks

- Python-like indentation; `.dev` extension
- Type names are dual-spelled: `i32`/`int32`/`int`, `u32`/`uint32`/`uint`, `byte`/`u8`, `f64`/`float64`/`float`. Single table in `ty/mod.rs` governs both sema and lowerer.
- `let` = immutable, `var` = mutable
- `pub def main()` → mangled to `_forge_main` in hosted targets (runtime `_start` calls it)
- Hosted runtime helpers (`_dev_puts`, `_dev_exit`, etc.) declared `extern` in `core/*.dev`; the compiler emits them in `src/backend/codegen/runtime.rs`. The GC heap helpers (`_dev_alloc`, `_dev_free`, `_dev_gc_*`) live in `src/backend/codegen/gc.rs`
- `@freestanding` attribute bypasses hosted runtime requirements
- codegen16 runtime stubs (in `src/backend/codegen16/program.rs`, `BUILTIN_FUNCS`) are inline machine code, not calls into `core/`; the stage-1 boot sector uses them directly: `_dev_bios_teletype`, `_dev_serial_putc`, `_dev_load_char`, `_dev_bios_key`, `_dev_bios_disk_reset`, `_dev_bios_disk_read` (CHS), `_dev_bios_disk_read_lba` (INT 13h AH=42h), `_dev_jump`, and the mode-switch stubs `_dev_enter_pmode` (16→32) and `_dev_enter_long_mode` (16→32→64). `_dev_enter_long_mode(lo, hi)` runs the whole switch itself: A20, 16→32 GDT + CR0.PE, a 32-bit trampoline that relocates the kernel staging buffer (0x8000→0x100000) and stashes the entry at 0x8FF8, 4-level identity page tables covering 0..1 GiB (PDPT[0]=0x83, which also satisfies the PAE 3-level transition window), CR3/CR4.PAE/EFER.LME via `wrmsr`, a 64-bit GDT + `lgdt`, CR0.PG, a `far jmp` to a 64-bit code segment, and the trampoline `mov rax,0x90000; mov rax,[0x8FF8]; jmp rax`. It is exercised by `examples/os64`. Before switching, it probes CPUID for long-mode support: it reads leaf `0x80000000` and only tests leaf `0x80000001` if its max-extended-leaf is `>= 0x80000001` (a 32-bit-only CPU omits that leaf, whose absent EDX returns leaf-0/vendor data with a spuriously-set bit 29), then checks the `LM` bit (EDX bit 29). On a 32-bit-only CPU it emits `No 64-bit CPU\r\n` over COM1 (port 0x3F8) -- the os64 boot chain's console under `-nographic` -- and halts, rather than triple-faulting.
- Power operator (`**`) requires integer operands; desugars to `__forge_pow` runtime call (loop-based, integer-only)
- Floor division (`//`) is floor-toward-negative-infinity; floor division by zero panics at runtime
- `unsafe` blocks bypass pointer safety checks
- `as` is a postfix cast operator in the expression grammar

## Module system

`from std.<name> import ...` resolves to `core/<name>.dev` by walking up from source directory, falling back to the stdlib embedded in the binary (`src/embed.rs`, `include_str!` per module) when no on-disk `core/` exists — the packaged `forgec` binary is self-contained. Only `std.*` modules are supported. See `src/driver/loader.rs`.

Stdlib modules: `io`, `runtime`, `volatile`, `mem`, `string`, `math`, `alloc`, `gc`, `fmt`. (All except `gc` are cross-target; `gc` is x86_64-only and rejected on x86_32 at codegen.)

Compilation is deterministic: `merge_modules` in `src/driver/loader.rs` merges transitive imports in sorted module-path order (the load graph is a `HashMap`, whose iteration order is randomized).

## Testing quirks

- Integration tests compile examples to temp dirs, run the native binaries, and check stdout/exit code
- Bootloader/kernel/OS tests spawn QEMU (`qemu-system-x86_64` must be on PATH), sleep, then kill it
- `os_dev_boots_shell_and_calc` additionally needs `socat` (drives the QEMU monitor to type `calc 42` into the ForgeOS shell); boots the full three-stage OS image built from `examples/os/`
- `getchar` and `guess` tests write to stdin
- `tests/lexer_tests.rs` and `tests/parser_tests.rs` contain only placeholder tests
- Importing any `std.*` module compiles the **entire** module file — x86_32 tests fail if the stdlib uses `float64` (not supported on x86_32)
- `obj::tests::tiny_static_elf*` tests can occasionally fail due to test parallelism; run isolation if needed
- Example `.dev` files are also integration test fixtures — adding a new example used as a test fixture is fine

## Codegen notes

- Local variables are 64-bit stack slots regardless of declared type (x86_64);
  on x86_32, real struct locals are allocated with their full byte size (min 4)
- Width-correct load/store (8/16/32-bit) only used for pointer deref and field access
- No optimizer — every unsafe deref emits a real memory access (effectively volatile)
- Struct fields are laid out sequentially without padding in the first milestone
- **Real structs are address-bearing on both backends**: evaluating a struct-
  typed expression yields the struct's *address* (inline struct var → LEA; call
  result / block / pointer var → pointer), never its contents inline.  Struct
  returns use return-by-pointer: the caller allocates a scratch slot and passes
  it as a hidden first argument; the callee copies the value there and returns
  that pointer.  Synthetic `__enum_*` structs are the exception — their values
  are 4-byte pointers and keep scalar semantics everywhere (`is_enum_struct` in
  `codegen/layout.rs` / `codegen32/layout.rs`).
- x86_64: System V AMD64 ABI (args in RDI, RSI, RDX, RCX, R8, R9; struct-return
  hidden arg shifts the rest up by one)
- Float values are stored as 64-bit integer bit patterns in 64-bit slots/RAX
- Integer-to-float and float-to-integer casts use **XMM7** (scratch), not XMM0 — XMM0 is used by `eval_float_bin` for binary operations and will be clobbered
- Parser `parse_type` and `parse_type_atom` call `skip_newlines()` internally; use `parse_type_noskip()` from the `as` handler in `parse_postfix` to avoid consuming newlines that the postfix loop needs to see
- x86_32 structs (codegen32): real struct locals are allocated with their full byte size (min 4) and are **address-bearing** — `Var` of a struct leaves the slot address in EAX; copies/initializers/assignments copy the full byte width (`copy_struct_bytes`/`copy_ptr_to_slot`/`copy_mem_to_mem` in `codegen32/`). All struct returns use the i386 **sret** convention (hidden first arg = caller-allocated struct pointer at `EBP+8`; callee copies the struct there and returns that pointer in EAX; named params shift up by one 4-byte arg slot). Struct *arguments* pass the struct's address (full value accessible to the callee). Synthetic `__enum_*` structs keep scalar semantics (`is_enum_struct` in `codegen32/layout.rs`).

## Codegen16 notes (x86_16 flat/raw images)

- Arguments are pushed left-to-right by the caller, so `emit_func` reads param *i* at `4 + (n-1-i)*2` (deepest-first) on the frame.
- Short jumps are widened to near form when their target falls out of ±128 bytes: `EB` becomes `E9 <rel16>`, conditional jumps become `op^1, 3, E9 <rel16>` (jump over the E9). `into_bytes` fixes displacement by iterating until fixpoint — every patched displacement (short, rel16, imm16) must be computed at the **shifted** position (`offset + delta_before(offset, &widened)`); anchoring at the pre-widening offset lands every jump `delta_before(site)` bytes past its target.
- `LOAD` (default `0x7C00`) is the base added to absolute string addresses (imm16 fixups); stages loaded elsewhere must declare it in their `.fld`.

## GC heap (x86_64 hosted target only)

- `std.alloc`/`std.gc` map to `_dev_alloc`/`_dev_free`/`_dev_gc_*`, emitted in
  `src/backend/codegen/gc.rs` whenever the program references any of them
  (`gc_enabled`).  The 4 MiB heap is `.bss`; `gc_state` (96 B) is in `.data`.
- Allocator: first-fit free list with splitting, 8-byte header before each
  payload (bit 0 = USED, bit 1 = MARK, rest = size).  `free` prepends.
- Collector: conservative mark-and-sweep.  Roots = stack `[rbp, stack_top]`
  (stack_top captured at `_start`) + `.rodata`.  Automatic collection runs when
  the free list is exhausted (`_dev_alloc` retries once after collecting).
- **Every function frame is zeroed on return** (`emit_func` emits the clearing
  before `leave`) so dead frames cannot act as GC roots — this is what makes
  `leak_check()` detect dropped references across calls.
- x86_32 has its own free-list allocator (`_dev_alloc`/`_dev_free` emitted in
  `src/backend/codegen32/runtime.rs`): first-fit with splitting, 4-byte header
  before each payload (bit 0 = USED, rest = size), lazy arena init, `HEAP`
  directive honored (default 4 MiB).  `free` returns blocks to the list.
  There is no collector — importing `std.gc` on x86_32 is a clean codegen
  error (`not supported on the x86_32 target`).

## Generics

- Generic functions (`def f[T]`) and generic structs (`struct Pair[T]`)
  are monomorphized per instantiation.  Sema registers concrete instances
  (`register_mono_struct`, `infer_generic_params`, `finalize_struct_apps` in
  `src/sema/check/mod.rs`); the lowerer mirrors this with `generic_defs`,
  `pending_instances`, `ensure_mono_struct`, and `pattern_type` in
  `src/lower/mod.rs`.  Both sides agree on names via
  `ty::mono_struct_name` (`Pair[int64]` → `Pair$i64`) and `sanitize_mangle`
  in `src/ty/mod.rs`; `sema/typed::MonoInstance` mangles generic *function*
  instances.
- `Type::StructApp { base, args }` (in `src/ty/mod.rs`) is a generic struct
  application that still contains type parameters — it exists only inside a
  generic function body and is resolved once the instance is known.
- Call sites infer type arguments from argument types (no explicit
  `f[int64](...)` syntax); inference unifies `T` patterns and recovers
  `Pair[T]` arguments by matching field types against the concrete struct.
- Parser disambiguation: `Name[Type, ...] { .. }` is a generic struct
  literal.  `parse_postfix` scans for a matching `]`, tries to parse a type
  list, and only treats it as a type application if every argument is a
  definite type (primitive, in-scope generic, or compound); anything else
  falls back to an index expression.  Function `[T]` parameters are tracked
  in `Parser::scope_generics` (pushed per `def`).

## Inline assembly

Forge intentionally does **not** implement `asm!()` or any inline assembly
syntax.  Hardware primitives (port I/O, interrupt control, PIC arbitration)
are exposed entirely through compiler-emitted `_dev_*` runtime helpers that
are declared `extern` in `core/hal.dev` and wrapped by typed `pub` functions
in the `std.hal` module.  The parser still accepts `asm!()` calls (for
forward compatibility), but any `asm!()` expression produces a compile-time
error on all targets.

## What not to touch

- `src/mir/` is scaffolding for future work; not wired into the pipeline
- `src/ty/mod.rs` `Type` enum is the source of truth for type names — sema and lowerer both route through it
- Type names `i128`/`uint128` are mapped in sema (`typing.rs`) but rejected at lowering with "128-bit integers are not supported by any backend yet"
- `.gitignore` comments warn: do NOT gitignore bare `core` (would exclude the `core/` stdlib dir)
- Inline assembly (`asm!()`) — not implemented and not planned; use the `std.hal` module's `int()`, `outb()`, `inb()`, etc. wrappers instead

## Cross-target parity

When adding a feature to the x86_64 backend, mirror it in `codegen32/` + `x86/` so the `x86_32-unknown-linux-gnu` target stays in parity. The 32-bit target does not support floats — stdlib modules using `f64` will break x86_32 tests.

The x86_64 freestanding runtime (`src/backend/codegen/runtime.rs`, `FREESTANDING_FUNCS` in `src/backend/codegen/mod.rs`) mirrors `codegen32`'s: only the `_dev_*` helpers a freestanding `FORMAT raw` image references get emitted (reference-driven). Both backends expose `_dev_outb`, `_dev_inb`, `_dev_outw`, `_dev_inw`, `_dev_outl`, `_dev_inl`, `_dev_iret`, `_dev_sti`, `_dev_cli`, and `_dev_halt`.  The x86_16 backend exposes the same byte/word helpers via `BUILTIN_FUNCS` in `src/backend/codegen16/program.rs` but does not support `_dev_outl`/`_dev_inl` (32-bit types are rejected by `type_info`).  `INT nn` is emitted inline via `ExprKind::IntImm` rather than as a runtime call — the lowerer desugars `_dev_int(<literal>)` calls.

## Parser gotchas

- Postfix expressions may continue across a newline only via `.` or `as` at the same indentation. A following statement starting with `(`, `[`, or `{` is never absorbed — `var x: int64 = i as int64` followed by `(p)[0] = i` parses as two statements. Keep the `.`/`as` continuation lines at the same indent as the expression they continue (deeper indent yields an `Indent` token and fails).
- Struct layouts are computed recursively (`compute_struct_layouts` in `codegen/layout.rs` + `codegen32/layout.rs`) — nested structs work; by-value struct cycles are a clean error.

# Known Issues & Limitations

This page documents the current limitations of Forge's first milestone. Some
features are parsed or type-checked but do not work end-to-end. Everything here
is expected to land in future milestones.

## Clean Errors (no compiler crashes)

These constructs are rejected with a clean diagnostic instead of panicking the
compiler:

| Construct | Error |
|-----------|-------|
| `impl` blocks: `impl Point: ...` | Methods type-check; the impl itself is accepted (method calls not yet lowered) |
| Nested struct fields: `Outer { inner: Inner { ... } }` | Supported; layouts are computed recursively |
| Recursive structs by value: `struct A: a: A` | `struct \`A\` contains itself by value` |
| `int128` / `uint128` | `128-bit integers are not supported by any backend yet` |
| `std.gc` on x86_32 | `` `_dev_gc_collect` (std.gc) is not supported on the x86_32 target; std.alloc's alloc/free work, but there is no garbage collector `` (at codegen) |

## Parsed But Not Functional

| Construct | Status |
|-----------|--------|
| `union` | "not a struct type" error; no codegen |
| Tuples | Supported end-to-end on x86_64 and x86_32: literals, annotations, indexed field access, destructuring, and **return types** |
| Generic signatures with `union`/`enum` types | Lowering error — `union/enum types are not supported in generic signatures yet` |
| Fixed-size array annotations `[int32; 5]` and repeat literals `[0; 3]` | Parse error (plain `[1, 2, 3]` literals work) |
| Slices `slice[T]`, `&arr[..]`, `&arr[1..3]` | Type error |
| Function types `fn(int32) -> int32` | Parse error |
| `refmut[T]` / `&mut x` | `&mut` is not parsed; `refmut` annotations cannot be satisfied |
| `own[T]` | Type annotation parses, but there is no `own(...)` constructor |
| Byte-string literals `b"bytes"` | Not supported |
| Block expressions `let x = { ... }` | Supported end-to-end |
| `@export`, `@inline`, `@noreturn`, `@naked` | Unknown attribute errors (only `@freestanding`, `@packed`, `@align`, `@c_enum`, `@extern` are accepted) |
| `@packed`, `@align(N)`, `@c_enum` | Accepted but have **no effect** on layout or codegen yet |
| Compound assignment `+=`, `-=`, etc. | Supported end-to-end (desugar `x += 1` to `x = x + 1`; see [Syntax](syntax.md)) |
| `asm!()` | Compile-time error on **all** targets — inline assembly is intentionally not supported (the parser accepts the syntax for forward compatibility only); use the `std.hal` module's port-I/O, interrupt, and PIC wrappers instead |

## Parser Notes

- An expression ending with `as` (or any expression) is never merged with a
  following statement that starts with `(`, `[`, or `{` — those can begin a new
  statement. Multi-line continuation still works for `.` and `as` at the same
  indentation.

## Backend Limitations

- **x86_32 has no float support** — any `float32`/`float64` usage fails at
  codegen. Stdlib modules that use floats cannot be imported on x86_32.
- **Struct values are address-bearing on both backends** — a struct-typed
  expression evaluates to the struct's address, not its contents.
  x86_64: struct arguments are passed by pointer (hidden sret slot for
  returns) so the callee sees the full struct; x86_32 uses the same scheme
  with the i386 cdecl convention (hidden first arg = caller-allocated struct
  pointer for returns; named parameters shift up by one 4-byte slot).
  Synthetic `__enum_*` structs are the exception — their values are 4-byte
  pointers and keep scalar semantics everywhere.
- **No optimizer** — every unsafe deref emits a real memory access
  (effectively volatile); codegen is roughly `-O0` quality.
- **Struct fields are laid out without padding** — alignment is not
  inserted between fields.
- **`--freestanding` is required for `x86_16-boot`** programs; hosted runtime
  helpers (`_dev_puts`, `_dev_exit`, ...) are only emitted for hosted targets.

## Standard Library Limitations

- `std.gc` is **x86_64 only** — importing it on x86_32 fails at codegen with a
  clean diagnostic (the free-list allocator works, there is just no
  collector).
- `std.alloc` on x86_32 is a free-list allocator (first-fit with splitting,
  4-byte block headers); `free` returns blocks to the list for reuse, but
  there is no automatic reclamation.
- `std.string` functions require null-terminated strings; no bounds checking.
- `getchar` returns `-1` on EOF; `rand` is a 31-bit LCG (not cryptographic).
- Importing a `std.*` module compiles the **entire** module file, so importing
  a module that uses `float64` on x86_32 will fail.

## Module System

- **Flat namespace**: all imported items merge into one namespace; qualified
  access (`mymodule.foo()`) is not supported.
- **No re-exports, no versioning**, no conditional compilation.
- Only `std.*` and local `.dev` modules are supported.

## Environment

- `forgec` builds and runs on **Linux x86_64** (and i686 hosts for x86_32
  output). macOS and Windows targets are planned but not implemented.
- The bootloader tests require QEMU (`qemu-system-x86_64`) to run.
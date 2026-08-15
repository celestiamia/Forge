# Known Issues & Limitations

This page documents the current limitations of Forge's first milestone. Some
features are parsed or type-checked but do not work end-to-end. Everything here
is expected to land in future milestones.

## Clean Errors (no compiler crashes)

These constructs are rejected with a clean diagnostic instead of panicking the
compiler:

| Construct | Error |
|-----------|-------|
| Generic functions: `def identity[T](x: T) -> T` | `generic function \`identity\` is not supported yet` (at lowering) |
| Generic structs: `struct Pair[T]: ...` | `generic struct \`Pair\` is not supported yet` |
| `impl` blocks: `impl Point: ...` | Methods type-check; the impl itself is accepted (method calls not yet lowered) |
| Nested struct fields: `Outer { inner: Inner { ... } }` | Supported; layouts are computed recursively |
| Recursive structs by value: `struct A: a: A` | `struct \`A\` contains itself by value` |
| `int128` / `uint128` | `128-bit integers are not supported by any backend yet` |

## Parsed But Not Functional

| Construct | Status |
|-----------|--------|
| `enum` — variants (`Color.Red`, `Red`, `case Red:`) | Declaration compiles; variants cannot be referenced or matched |
| `union` | "not a struct type" error; no codegen |
| Tuples `(a, b)` (type annotations, literals, return types) | Fails at lowering: "tuples are not supported in the first milestone" |
| Tuple destructuring `let (a, b) = t` | Parse error |
| Fixed-size array annotations `[int32; 5]` and repeat literals `[0; 3]` | Parse error (plain `[1, 2, 3]` literals work) |
| Slices `slice[T]`, `&arr[..]`, `&arr[1..3]` | Type error |
| Function types `fn(int32) -> int32` | Parse error |
| `refmut[T]` / `&mut x` | `&mut` is not parsed; `refmut` annotations cannot be satisfied |
| `own[T]` | Type annotation parses, but there is no `own(...)` constructor |
| Byte-string literals `b"bytes"` | Not supported |
| Block expressions `let x = { ... }` | Error: "block expressions are not supported in the first milestone" |
| `@export`, `@inline`, `@noreturn`, `@naked` | Unknown attribute errors (only `@freestanding`, `@packed`, `@align`, `@c_enum`, `@extern` are accepted) |
| `@packed`, `@align(N)`, `@c_enum` | Accepted but have **no effect** on layout or codegen yet |
| Compound assignment `+=`, `-=`, etc. | Parse error — write `x = x + 1` |
| `asm!()` on x86_64 / x86_32 | Codegen error — inline assembly works only on the `x86_16-boot` target |

## Parser Notes

- An expression ending with `as` (or any expression) is never merged with a
  following statement that starts with `(`, `[`, or `{` — those can begin a new
  statement. Multi-line continuation still works for `.` and `as` at the same
  indentation.

## Backend Limitations

- **x86_32 has no float support** — any `float32`/`float64` usage fails at
  codegen. Stdlib modules that use floats cannot be imported on x86_32.
- **x86_32 struct-by-value copies copy only the first 4 bytes** — locals are
  4-byte slots, so a multi-field struct assigned to another variable (or a
  nested struct value) loses every field after the first. Field-by-field
  access works; whole-struct copies are x86_64-only for now.
- **No optimizer** — every unsafe deref emits a real memory access
  (effectively volatile); codegen is roughly `-O0` quality.
- **Struct fields are laid out without padding** — alignment is not
  inserted between fields.
- **`--freestanding` is required for `x86_16-boot`** programs; hosted runtime
  helpers (`_dev_puts`, `_dev_exit`, ...) are only emitted for hosted targets.

## Standard Library Limitations

- `std.gc` is **x86_64 only**.
- `std.alloc` on x86_32 is a bump allocator — `free` is a no-op.
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
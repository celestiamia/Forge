# Type System

Forge has a static type system with explicit annotations and inference for
literals.

## Primitive Types

### Integers

| Type | Aliases | Size | Range |
|------|---------|------|-------|
| `int8` | `i8` | 8-bit signed | -128 to 127 |
| `int16` | `i16` | 16-bit signed | -32,768 to 32,767 |
| `int32` | `i32`, `int` | 32-bit signed | -2^31 to 2^31-1 |
| `int64` | `i64` | 64-bit signed | -2^63 to 2^63-1 |
| `uint8` | `u8`, `byte` | 8-bit unsigned | 0 to 255 |
| `uint16` | `u16` | 16-bit unsigned | 0 to 65,535 |
| `uint32` | `u32`, `uint` | 32-bit unsigned | 0 to 2^32-1 |
| `uint64` | `u64` | 64-bit unsigned | 0 to 2^64-1 |
| `usize` | — | pointer-sized unsigned (maps to `uint64`) | platform-dependent |
| `isize` | — | pointer-sized signed (maps to `int64`) | platform-dependent |

> `int128` / `uint128` are accepted by the type checker but unsupported by all
> backends — compilation fails at codegen. See [Known Issues](known-issues.md).

### Floats

| Type | Aliases | Size | Notes |
|------|---------|------|-------|
| `float32` | `f32` | 32-bit | IEEE 754 single; x86_64 only |
| `float64` | `f64`, `float` | 64-bit | IEEE 754 double; x86_64 only |

> Floats are **not supported on the x86_32 target**. On x86_64, float values
> are stored as 64-bit integer bit patterns in registers.

### Other Primitives

| Type | Description |
|------|-------------|
| `bool` | `true` or `false` |
| `char` | 8-bit character (like a byte) |
| `void` | No value (function return type only) |

## Pointer Types

```dev
ptr[T]       # Pointer to T
```

```dev
let x: int32 = 42
let p: ptr[int32] = &x        # Address of a local
let q = 0x1000 as ptr[int32]  # Integer literal as pointer
```

Pointers:
- Can be created with `&local`, integer casts, `alloc()`, or string literals (`ptr[char]`)
- Dereferencing (`*p`) and pointer arithmetic (`p + n`) require `unsafe`
- `&x` is implicitly coerced to `ptr[T]` when passed as a function argument
- Can be cast to/from integers with `as`

There is no separate `mut ptr[T]` spelling — mutability is expressed with
`var` bindings and enforced by the `unsafe` rules.

## Composite Types

### Structs

```dev
struct Point:
    x: int32
    y: int32
```

- Constructed with a struct literal: `Point { x: 1, y: 2 }`
- Fields are laid out sequentially **without padding**
- Field access on a pointer parameter auto-dereferences: `p.x` where `p: ptr[Point]`
- Nested structs are not yet supported (see [Known Issues](known-issues.md))

### Arrays

```dev
let arr = [1, 2, 3]   # Array literal
let x = arr[0]
```

Array literals and indexing work. Fixed-size type annotations
(`[int32; 5]`) and repeat literals (`[0; 3]`) are **not** supported yet.

### Enums & Unions

```dev
enum Color:
    Red
    Green
    Blue
```

`enum` and `union` declarations are accepted by the parser and type checker,
but enum variants cannot be referenced and unions cannot be constructed —
neither has codegen support. See [Known Issues](known-issues.md).

## Type Inference

Types are inferred when possible:

```dev
let x = 42        # int32 (default integer)
let y = 3.14      # float64 (default float)
let z = true      # bool
let s = "hello"   # ptr[char] (string literal)
```

Explicit annotation is used when needed:

```dev
let x: int64 = 42
let f: float64 = 1.5
```

## Casts

There are no implicit numeric conversions — explicit `as` casts are required:

```dev
let y = x as int64      # Integer widening/narrowing
let f = 42 as float64   # Integer to float
let i = 3.7 as int32    # Float to integer (truncating)
let p = 0x1000 as ptr[int32]  # Integer to pointer
let addr = p as int64   # Pointer to integer
```

## sizeof / offsetof

```dev
let size = sizeof(u32)         # Size of a type in bytes
let size2 = sizeof(MyStruct)
let off = offsetof(Point, y)   # Byte offset of a field
```

Both return `usize`-typed values and work on primitives and struct types.

## Unsupported Types

The following type forms parse but are not functional yet — see
[Known Issues](known-issues.md):

- `ref[T]` / `refmut[T]` — references (only `ref[T]` annotation + deref work)
- `own[T]` — owned pointers (declaration only; no constructor)
- `(T1, T2, ...)` — tuples (fail at lowering)
- `[T; N]` — fixed-size array annotations
- `slice[T]` — slices
- `fn(T) -> T` — function types
# Type System

Forge has a static type system with type inference.

## Primitive Types

### Integers

| Type | Size | Range |
|------|------|-------|
| `int8` / `i8` | 8-bit signed | -128 to 127 |
| `int16` / `i16` | 16-bit signed | -32,768 to 32,767 |
| `int32` / `i32` / `int` | 32-bit signed | -2^31 to 2^31-1 |
| `int64` / `i64` | 64-bit signed | -2^63 to 2^63-1 |
| `uint8` / `u8` / `byte` | 8-bit unsigned | 0 to 255 |
| `uint16` / `u16` | 16-bit unsigned | 0 to 65,535 |
| `uint32` / `u32` / `uint` | 32-bit unsigned | 0 to 2^32-1 |
| `uint64` / `u64` | 64-bit unsigned | 0 to 2^64-1 |
| `isize` | Pointer-sized signed | Platform-dependent |
| `usize` | Pointer-sized unsigned | Platform-dependent |

### Floats

| Type | Size | Standard |
|------|------|----------|
| `float32` / `f32` | 32-bit | IEEE 754 single |
| `float64` / `f64` / `float` | 64-bit | IEEE 754 double |

### Other Primitives

| Type | Description |
|------|-------------|
| `bool` | `true` or `false` |
| `char` | Unicode code point (32-bit) |
| `void` | No value (unit type) |

## Type Aliases

| Alias | Canonical Type |
|-------|----------------|
| `int` | `int32` |
| `uint` | `uint32` |
| `byte` | `uint8` |
| `float` | `float64` |

## Composite Types

### Pointers

```dev
ptr[T]       # Immutable pointer to T
mut ptr[T]   # Mutable pointer to T
```

```dev
let x: int32 = 42
let p: ptr[int32] = &x        # Immutable pointer
let mp: mut ptr[int32] = &mut x  # Mutable pointer
```

### References

```dev
ref[T]       # Immutable reference (borrow)
refmut[T]    # Mutable reference (exclusive borrow)
```

```dev
let x = 42
let r: ref[int32] = &x
let mr: refmut[int32] = &mut x
```

### Own Pointers

```dev
own[T]       # Unique ownership (like Box<T>)
```

```dev
let o: own[int32] = own(42)
```

### Arrays

```dev
[T; N]       # Fixed-size array of N elements
```

```dev
let arr: [int32; 5] = [1, 2, 3, 4, 5]
let zeroed: [int32; 10] = [0; 10]
```

### Slices

```dev
slice[T]     # Fat pointer: (ptr, len)
```

```dev
let arr: [int32; 5] = [1, 2, 3, 4, 5]
let s: slice[int32] = &arr[..]  # Full slice
let s2 = &arr[1..3]             # Subslice
```

### Tuples

```dev
(T1, T2, ...)  # Fixed-size heterogeneous collection
```

```dev
let t: (int32, float64, char) = (42, 3.14, 'a')
let (x, y, z) = t  # Destructuring
```

### Structs

```dev
struct Point:
    x: int32
    y: int32

# With methods
impl Point:
    pub def new(x: int32, y: int32) -> Point:
        return Point { x: x, y: y }
```

### Enums

```dev
enum Result<T, E>:
    Ok(T)
    Err(E)

# Pattern matching
match result:
    case Ok(v): puts("success")
    case Err(e): puts("error")
```

### Unions

```dev
union IntOrFloat:
    i: int32
    f: float64

# Requires unsafe to access
unsafe:
    let u = IntOrFloat { i: 42 }
    puts(u.i)
```

### Function Types

```dev
fn(T1, T2) -> T3
```

```dev
let f: fn(int32, int32) -> int32 = add
```

## Type Constructors

```dev
# Pointer
let p = &x as ptr[int32]

# Cast
let y = x as int64

# Array literal
let a = [1, 2, 3]
let a: [int32; 3] = [1, 2, 3]

# Struct literal
let p = Point { x: 1, y: 2 }

# Tuple
let t = (1, 2.0)

# Slice
let s = &arr[..]
let s = &arr[1..3]
```

## Type Inference

Types are inferred when possible:

```dev
let x = 42        # int32 (default integer)
let y = 3.14      # float64 (default float)
let z = true      # bool
let s = "hello"   # ptr[char] (string literal)
```

Explicit annotation when needed:

```dev
let x: int64 = 42
let f: fn(int32) -> int32 = add
```

## Subtyping & Variance

- Pointers are invariant
- References are covariant in lifetime
- Function types: contravariant in args, covariant in return

## Size & Alignment

| Type | Size | Alignment |
|------|------|-----------|
| `int8` / `uint8` | 1 | 1 |
| `int16` / `uint16` | 2 | 2 |
| `int32` / `uint32` / `float32` | 4 | 4 |
| `int64` / `uint64` / `float64` | 8 | 8 |
| `ptr[T]` | 8 (64-bit) / 4 (32-bit) | pointer size |
| `slice[T]` | 16 / 8 | pointer size |

## Type Compatibility

- No implicit numeric conversions
- Explicit casts required: `x as int64`
- Pointer to integer: `p as usize`
- Integer to pointer: `x as ptr[T]`

## Generics

```dev
pub def identity<T>(x: T) -> T:
    return x

pub def first<T>(a: T, b: T) -> T:
    return a

# Monomorphized at call sites
let x = identity(42)      # T = int32
let y = identity(3.14)    # T = float64
```

Constraints not yet supported (planned: `where T: Add`).
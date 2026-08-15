# Memory Model

Forge provides low-level memory control: stack locals, raw pointers, manual
heap allocation, and an optional garbage collector on x86_64.

## Pointers

### Creating Pointers

```dev
let x = 42
let p: ptr[int32] = &x          # Address of a local

let q = 0x1000 as ptr[int32]    # Integer literal as pointer (MMIO, etc.)
let addr = p as int64           # Pointer to integer
```

`&local` is implicitly coerced to `ptr[T]` when passed to a function:

```dev
def value_at(p: ptr[int32]) -> int32:
    unsafe:
        return *p

var n = 7
let v = value_at(&n)   # v == 7
```

String literals have type `ptr[char]` (null-terminated, stored in `.rodata`).

### Dereferencing

Pointer dereference and pointer arithmetic are **only** allowed inside
`unsafe` blocks:

```dev
unsafe:
    let v = *p          # Read through pointer
    *p = 42             # Write through pointer
    p = p + 1           # Pointer arithmetic (byte offset)
```

There is no bounds checking — the caller is responsible for validity.

### Volatile Access

`&x`-style access through plain deref is effectively volatile (the compiler
emits real memory accesses), but for explicit hardware access the `std.volatile`
module provides width-correct loads and stores:

```dev
from std.volatile import load_u32, store_u32

var reg: uint32 = 0
store_u32(&reg, 0x1234)
let v = load_u32(&reg)
```

## Stack Allocation

```dev
let x = 42              # Stack local
var y = 10
let arr = [1, 2, 3]     # Stack array
```

All locals are 64-bit stack slots regardless of declared type.

## Heap Allocation

`std.alloc` provides `alloc`/`free` over a compiler-emitted heap:

```dev
from std.alloc import alloc, free

let buf: ptr[char] = alloc(64)
unsafe:
    *buf = (65 as char)
    *(buf + 1) = (0 as char)
free(buf)
```

- **x86_64**: 4 MiB heap, first-fit free list with 8-byte block headers.
  When the free list is exhausted, a conservative mark-and-sweep collection
  runs automatically and allocation retries once.
- **x86_32**: bump allocator; `free` is a no-op.

See [std.alloc](../stdlib/alloc.md) and [std.gc](../stdlib/gc.md).

## Garbage Collection (x86_64 only)

The GC heap backs `std.alloc`. `std.gc` exposes collection and statistics:

```dev
from std.gc import collect, leak_check, heap_live

collect()                 # Force a mark-and-sweep pass
let live = heap_live()    # Bytes currently live
let leaks = leak_check()  # Bytes dropped without free
```

The collector is conservative: it scans the stack `[rbp, stack_top]` and
`.rodata` for words that look like heap pointers. Every function frame is
zeroed on return so dead frames cannot act as GC roots.

## sizeof / offsetof

```dev
let size = sizeof(Point)      # Struct size (fields, no padding)
let off = offsetof(Point, y)  # Byte offset of field y
```

## Struct Layout

Fields are laid out sequentially **without padding**:

```dev
struct Point:
    x: int32  # offset 0
    y: int32  # offset 4
# sizeof(Point) == 8
```

## Copying and Zeroing

```dev
from std.mem import copy_bytes, set_bytes, zero_bytes, compare_bytes

copy_bytes(dst, src, 100)        # memcpy (handles overlap)
set_bytes(ptr, 0, 100)           # memset
zero_bytes(ptr, 64)              # secure zero
compare_bytes(a, b, 100) == 0    # memcmp
```

All take `ptr[uint8]` arguments and a `uint64` byte count.

## Target Differences

| Feature | x86_64 | x86_32 | x86_16 |
|---------|--------|--------|--------|
| Pointer size | 8 bytes | 4 bytes | 2 bytes (16-bit) |
| Heap (`std.alloc`) | First-fit + GC (4 MiB) | Bump | None |
| GC (`std.gc`) | Yes | No | No |
| Floats | Yes | No | No |

## Unsupported (see Known Issues)

- `ref[T]`/`refmut[T]`/`own[T]` — only partially parsed; `refmut` and `own`
  cannot be constructed
- Slices (`slice[T]`), tuples, `fn` types
- Bounds-checked indexing and borrow checking
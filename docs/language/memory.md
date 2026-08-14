# Memory Model

Forge provides low-level memory control with safety guarantees through its type system.

## Pointer Types

### Raw Pointers

```dev
ptr[T]           # Immutable pointer
mut ptr[T]       # Mutable pointer
```

```dev
let x = 42
let p: ptr[int32] = &x          # Immutable
let mp: mut ptr[int32] = &mut x  # Mutable
```

Raw pointers:
- No lifetime tracking
- No null checks (can be null)
- Require `unsafe` to dereference
- Can be cast to/from integers

### References (Safe Pointers)

```dev
ref[T]       # Immutable borrow
refmut[T]    # Exclusive mutable borrow
```

```dev
let x = 42
let r: ref[int32] = &x      # Immutable borrow
let mr: refmut[int32] = &mut x  # Mutable borrow
```

References:
- Tracked by borrow checker
- Cannot be null
- Lifetimes inferred (no explicit syntax yet)
- No `unsafe` needed

### Own Pointers (Owned Memory)

```dev
own[T]     # Unique ownership, auto-free on drop
```

```dev
let o: own[int32] = own(42)
let val = *o  # Auto-dereferences
```

Auto-frees when goes out of scope.

## Memory Allocation

### Stack Allocation

```dev
let x = 42              # Stack
let arr = [1, 2, 3]     # Stack array
```

Automatic lifetime management.

### Heap Allocation (Bump Allocator)

```dev
from std.alloc import alloc, free

let ptr = alloc(100)    # Allocate 100 bytes
# ... use ptr ...
free(ptr)               # Manual free (bump allocator)
```

Bump allocator: 64 KiB arena, no individual free (entire arena freed at once).

### Heap Allocation (GC)

```dev
from std.gc import gc_alloc, gc_free, gc_collect

let ptr = gc_alloc(100)
# ... use ptr ...
gc_free(ptr)
gc_collect()            # Force collection
```

Conservative mark-and-sweep GC (x86_64 only).

## Pointer Arithmetic

```dev
let ptr = alloc(100) as mut ptr[byte]
ptr = ptr + 10          # Offset by 10 bytes
let diff = ptr2 - ptr1  # Pointer difference (in elements)
```

Only valid within same allocation.

## Dereferencing

### Raw Pointers (Unsafe)

```dev
let ptr = &x as ptr[int32]
unsafe:
    let val = *ptr        # Read
    *mut_ptr = 42         # Write
```

### References (Safe)

```dev
let r = &x
let val = *r              # Auto-deref on use
```

## Casts

```dev
# Pointer to integer
let addr = ptr as usize

# Integer to pointer
let ptr = 0x1000 as ptr[int32]

# Between pointer types
let p: ptr[int32] = ...
let q: ptr[byte] = p as ptr[byte]

# Pointer to function
let fn_ptr = func as fn() -> int32
```

## Memory Safety

### Borrow Rules (References)

1. Multiple immutable borrows allowed: `ref[T]`
2. One mutable borrow: `refmut[T]`
3. No mutable + immutable simultaneously
4. References cannot outlive referent

```dev
let mut x = 42
let r1 = &x           # OK: immutable borrow
let r2 = &x           # OK: another immutable
let mr = &mut x       # Error: mutable borrow while immutable exists
```

### Ownership Transfer

```dev
let o1 = own(42)
let o2 = o1           # Move: o1 no longer valid
# o1 is now invalid
```

### Drop Semantics

- `own[T]` frees on scope exit
- References don't free
- Raw pointers never auto-free

## Unsafe Operations

```dev
unsafe:
    *ptr = 42              # Raw pointer write
    let val = *ptr         # Raw pointer read
    asm!("nop")            # Inline assembly
    extern_fn()            # Extern without wrapper
    union.field = val      # Union field access
```

### When Unsafe is Required

| Operation | Safe Alternative |
|-----------|------------------|
| `*raw_ptr` | Use `ref`/`refmut` |
| `extern_fn()` | Wrap in safe function |
| `union.field` | Use enum instead |
| `asm!()` | Use intrinsics |
| `ptr + offset` | Use slice indexing |

## Memory Layout

### Struct Layout

Fields laid out sequentially, no padding (current):

```dev
struct Point:
    x: int32  # offset 0
    y: int32  # offset 4
# Size: 8 bytes
```

### Alignment

| Type | Alignment |
|------|-----------|
| `int8`/`uint8` | 1 |
| `int16`/`uint16` | 2 |
| `int32`/`uint32`/`float32` | 4 |
| `int64`/`uint64`/`float64` | 8 |
| Pointers | 8 (64-bit) / 4 (32-bit) |

## Volatile Memory

```dev
from std.volatile import volatile_load, volatile_store

let ptr = 0xFFFF0000 as mut ptr[uint32]
volatile_store(ptr, 0x12345678)
let val = volatile_load(ptr)
```

Prevents compiler optimization of memory accesses.

## Memory Barriers

```dev
from std.volatile import mfence, lfence, sfence

mfence()  # Full memory barrier
lfence()  # Load barrier
sfence()  # Store barrier
```

## Zeroing Memory

```dev
from std.mem import zero_bytes

let buf = alloc(100)
zero_bytes(buf, 100)  # Securely zero
```

## Copying Memory

```dev
from std.mem import copy_bytes, set_bytes

copy_bytes(dst, src, 100)  # memcpy
set_bytes(ptr, 0, 100)     # memset
```

## Comparing Memory

```dev
from std.mem import compare_bytes

if compare_bytes(a, b, 100) == 0:
    puts("equal")
```

## Stack vs Heap

| Aspect | Stack | Heap |
|--------|-------|------|
| Speed | Fast | Slower |
| Size | Limited (~8MB) | Large |
| Lifetime | Scoped | Manual/GC |
| Allocation | Implicit | Explicit |

## Best Practices

1. **Prefer stack** for small, short-lived data
2. **Use references** over raw pointers when possible
3. **Use `own[T]`** for heap-allocated single values
3. **Use slices** for array views
4. **Minimize `unsafe`** - wrap in safe functions
5. **Free what you allocate** - pair `alloc`/`free`
6. **Use GC** for complex object graphs (x86_64 only)

## Target Differences

| Feature | x86_64 | x86_32 | x86_16 |
|---------|--------|--------|--------|
| Pointer size | 8 bytes | 4 bytes | 2 bytes (16-bit) |
| GC | Yes | No | No |
| Bump alloc | Yes | Yes | No |
| `usize` | 64-bit | 32-bit | 16-bit |
# std.mem

Low-level memory operations: copy, set, zero, compare.

## Functions

All functions take `ptr[uint8]` arguments and a `uint64` byte count. `byte`
is an alias for `uint8`.

### copy_bytes

```dev
def copy_bytes(dest: ptr[uint8], src: ptr[uint8], n: uint64) -> void
```

Copy `n` bytes from `src` to `dest`. Regions may overlap (memmove semantics).

```dev
from std.mem import copy_bytes
from std.alloc import alloc

let src: ptr[char] = "hello"
let dst: ptr[char] = alloc(16)
copy_bytes(dst, src, 6)   # copy 5 chars + null terminator
```

### set_bytes

```dev
def set_bytes(p: ptr[uint8], v: uint8, n: uint64) -> void
```

Fill `n` bytes starting at `p` with the value `v` (memset).

```dev
from std.mem import set_bytes

let buf: ptr[char] = alloc(100)
set_bytes(buf, 0xFF, 100)
```

### zero_bytes

```dev
def zero_bytes(p: ptr[uint8], n: uint64) -> void
```

Zero `n` bytes starting at `p`. Not optimized away (safe for clearing
secrets).

```dev
from std.mem import zero_bytes

zero_bytes(buf, 64)
```

### compare_bytes

```dev
def compare_bytes(a: ptr[uint8], b: ptr[uint8], n: uint64) -> int32
```

Lexicographically compare `n` bytes (memcmp). Returns `< 0`, `0`, or `> 0`.

```dev
from std.mem import compare_bytes

if compare_bytes(a, b, 100) == 0:
    puts("equal")
```

## Example: Buffer Operations

```dev
from std.mem import copy_bytes, set_bytes, zero_bytes, compare_bytes
from std.alloc import alloc, free
from std.io import puts

pub def main() -> int32:
    let buf1: ptr[char] = alloc(1024)
    let buf2: ptr[char] = alloc(1024)

set_bytes(buf1, 0x41 as uint8, 1024)   # Fill with 'A'
    copy_bytes(buf2, buf1, 1024)         # Copy

    if compare_bytes(buf1, buf2, 1024) != 0:
        puts("Mismatch!\n")
        return 1

    zero_bytes(buf1, 1024)               # Clear
    free(buf1)
    free(buf2)
    return 0
```

## Target Support

| Target | Implementation |
|--------|----------------|
| x86_64 | Compiler-emitted helpers |
| x86_32 | Compiler-emitted helpers |
| x86_16 | Available (byte loops) |

## Safety

- No bounds checking — the caller must ensure pointers are valid for `n` bytes
- No alignment requirements
- `copy_bytes` handles overlapping regions; `compare_bytes` does not
# std.mem

Low-level memory operations: copy, zero, set, compare.

## Functions

### copy_bytes

```dev
def copy_bytes(dst: mut ptr[byte], src: ptr[byte], n: usize) -> void
```

Copy `n` bytes from `src` to `dst`. Regions may overlap.

```dev
from std.mem import copy_bytes

let src = [1, 2, 3, 4, 5]
let mut dst = [0, 0, 0, 0, 0]
copy_bytes(&mut dst[0], &src[0], 5 * 4)
# dst = [1, 2, 3, 4, 5]
```

Equivalent to `memmove`.

### set_bytes

```dev
def set_bytes(dst: mut ptr[byte], value: byte, n: usize) -> void
```

Fill `n` bytes starting at `dst` with `value`.

```dev
from std.mem import set_bytes

let mut buf = [0; 100]
set_bytes(&mut buf[0], 0xFF, 100)
# buf = [0xFF; 100]
```

Equivalent to `memset`.

### zero_bytes

```dev
def zero_bytes(dst: mut ptr[byte], n: usize) -> void
```

Zero `n` bytes starting at `dst`. Optimized for security (not optimized away).

```dev
from std.mem import zero_bytes

let mut secret = [0; 64]
# ... use secret ...
zero_bytes(&mut secret[0], 64)  # Securely clear
```

### compare_bytes

```dev
def compare_bytes(a: ptr[byte], b: ptr[byte], n: usize) -> int32
```

Lexicographically compare `n` bytes. Returns:
- `< 0` if `a < b`
- `0` if equal
- `> 0` if `a > b`

```dev
from std.mem import compare_bytes

let a = [1, 2, 3]
let b = [1, 2, 4]
let cmp = compare_bytes(&a[0], &b[0], 3)
# cmp < 0 (a < b)
```

Equivalent to `memcmp`.

## Implementation Notes

- Highly optimized inline assembly for x86_64/x86_32
- `zero_bytes` uses volatile writes to prevent compiler optimization
- No alignment requirements (handles unaligned)
- Handles overlapping regions correctly (copy_bytes)

## Example: Buffer Operations

```dev
from std.mem import copy_bytes, set_bytes, zero_bytes, compare_bytes
from std.alloc import alloc, free

pub def main() -> int32:
    let size = 1024
    let buf1 = alloc(size) as mut ptr[byte]
    let buf2 = alloc(size) as mut ptr[byte]
    
    # Fill with pattern
    for i in 0..size:
        (buf1 + i) = (i % 256) as byte
    
    # Copy
    copy_bytes(buf2, buf1, size)
    
    # Verify
    if compare_bytes(buf1, buf2, size) != 0:
        puts("Mismatch!\n")
        return 1
    
    # Clear
    zero_bytes(buf1, size)
    
    free(buf1)
    free(buf2)
    return 0
```

## Target Support

| Target | Implementation |
|--------|----------------|
| x86_64 | Optimized REP MOVSB/STOSB |
| x86_32 | Optimized REP MOVSB/STOSB |
| x86_16 | Byte-by-byte loop |

## Safety

All functions require valid pointers and valid lengths. No bounds checking - caller must ensure:
- Pointers are valid for `n` bytes
- Regions don't wrap around address space
- No concurrent mutation during operation
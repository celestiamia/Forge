# std.alloc

Heap memory allocation.

```dev
from std.alloc import alloc, free
```

## Functions

### alloc

```dev
def alloc(size: uint64) -> ptr[char]
```

Allocate `size` bytes. Returns a `ptr[char]`. Aborts on OOM.

```dev
let buf: ptr[char] = alloc(1024)
unsafe:
    *buf = (65 as char)
free(buf)
```

Cast the result to other pointer types when needed:

```dev
let p = alloc(64) as ptr[int32]
```

### free

```dev
def free(p: ptr[char]) -> void
```

Free a previously allocated block.

- **x86_64**: prepends the block to the free list; memory is reused by future
  allocations
- **x86_32**: no-op (bump allocator — the whole arena is released at exit)

## Allocator Behavior

| Target | Allocator | `free` behavior |
|--------|-----------|-----------------|
| x86_64 | First-fit free list with splitting, 8-byte header per block (bit 0 = used, bit 1 = mark, rest = size) | Block returned to free list |
| x86_32 | Bump allocator | No-op |
| x86_16 | Not available | — |

On x86_64 the heap is a 4 MiB `.bss` region. When the free list is exhausted,
a conservative mark-and-sweep collection runs automatically and the allocation
retries once before aborting. See [std.gc](gc.md).

## Example

```dev
from std.alloc import alloc, free
from std.io import puts

pub def main() -> int32:
    let buf: ptr[char] = alloc(16)
    unsafe:
        *buf = (72 as char)          # 'H'
        *(buf + 1) = (105 as char)   # 'i'
        *(buf + 2) = (0 as char)     # null terminator
    puts(buf)
    free(buf)
    return 0
```

## Safety

- Aborts on OOM (does not return null)
- No bounds checking — writes past the requested size corrupt the heap
- Double-free and use-after-free are undefined behavior
- Pass only `alloc`-returned pointers to `free`
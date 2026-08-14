# std.alloc

Heap memory allocation with bump allocator (and optional GC on x86_64).

## Functions

### alloc

```dev
def alloc(size: usize) -> mut ptr[byte]
```

Allocate `size` bytes. Returns pointer to allocated memory. Aborts on OOM.

```dev
from std.alloc import alloc, free

let buf = alloc(1024) as mut ptr[int32]
buf[0] = 42
# ... use buf ...
free(buf as mut ptr[byte])
```

### free

```dev
def free(ptr: mut ptr[byte]) -> void
```

Free previously allocated memory. For bump allocator, this is a no-op (entire arena freed at once).

```dev
let ptr = alloc(100)
# ... use ...
free(ptr)
```

### bump_alloc (Internal)

```dev
def bump_alloc(size: usize) -> mut ptr[byte]
```

Internal bump allocator. Returns pointer or aborts.

## Allocator Types

### Bump Allocator (Default)

- Single 64 KiB arena
- Fast allocation (pointer bump)
- No individual free (arena freed at once)
- Suitable for short-lived allocations
- Available on all targets

### GC Allocator (x86_64 only)

```dev
from std.gc import gc_alloc, gc_free, gc_collect
```

- Conservative mark-and-sweep
- Individual free supported
- Automatic collection on exhaustion
- 4 MiB heap

## Usage Patterns

### Temporary Buffers

```dev
from std.alloc import alloc, free
from std.io import puts

pub def process_data() -> int32:
    let buf = alloc(4096) as mut ptr[byte]
    # ... process ...
    free(buf)
    return 0
```

### Arena Pattern

```dev
from std.alloc import alloc

def build_structure() -> ptr[Node]:
    let arena = alloc(64 * 1024)  # 64 KiB arena
    let mut offset = 0
    
    def allocate(size: usize) -> mut ptr[byte]:
        let ptr = (arena + offset) as mut ptr[byte]
        offset = offset + size
        return ptr
    
    # ... build using allocate ...
    return root_node  # Arena freed when function returns (if using bump)
```

## Target Differences

| Target | Allocator | Free Behavior |
|--------|-----------|---------------|
| x86_64 | Bump (64 KiB) / GC (4 MiB) | No-op (bump) / Actual free (GC) |
| x86_32 | Bump (64 KiB) | No-op |
| x86_16 | Not available | N/A |

## Safety

- Returns null on OOM (aborts in current implementation)
- Pointer must be from `alloc` to pass to `free`
- Double-free is undefined behavior
- Use-after-free is undefined behavior
- No bounds checking

## Example: Dynamic Array

```dev
from std.alloc import alloc, free
from std.mem import copy_bytes

struct DynArray:
    data: mut ptr[int32]
    len: usize
    cap: usize

impl DynArray:
    pub def new() -> DynArray:
        return DynArray { data: 0 as mut ptr[int32], len: 0, cap: 0 }
    
    pub def push(refmut self, value: int32):
        if self.len == self.cap:
            let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 }
            let new_data = alloc(new_cap * 4) as mut ptr[int32]
            if self.len > 0:
                copy_bytes(new_data, self.data, self.len * 4)
            free(self.data)
            self.data = new_data
            self.cap = new_cap
        
        self.data[self.len] = value
        self.len = self.len + 1
```
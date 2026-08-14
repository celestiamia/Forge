# std.gc

Garbage-collected heap allocator (x86_64 only).

## Overview

The GC provides a managed heap with automatic memory reclamation. Only available on x86_64 Linux.

## Functions

### gc_alloc

```dev
def gc_alloc(size: usize) -> mut ptr[byte]
```

Allocate `size` bytes from GC heap. Returns pointer or aborts on OOM.

```dev
from std.gc import gc_alloc, gc_free

let obj = gc_alloc(100) as mut ptr[MyStruct]
(*obj).field = 42
```

### gc_free

```dev
def gc_free(ptr: mut ptr[byte]) -> void
```

Explicitly free a GC-allocated object. Optional - GC will collect eventually.

```dev
gc_free(obj as mut ptr[byte])
```

### gc_collect

```dev
def gc_collect() -> void
```

Force a full garbage collection cycle.

```dev
gc_collect()  # Force collection now
```

### Statistics

```dev
def gc_alloc_count() -> uint64
def gc_free_count() -> uint64
def gc_collections() -> uint64
def gc_heap_live() -> uint64
def gc_heap_capacity() -> uint64
```

```dev
puts("Total allocated: ")
puts(format_i32(gc_alloc_count() as int32))
puts("\n")
```

### Leak Detection

```dev
def gc_leak_check() -> int32
```

Returns number of potentially leaked blocks (reachable only from dead frames).

```dev
let leaks = gc_leak_check()
if leaks > 0:
    puts("Potential leaks: ")
    puts(format_i32(leaks))
    puts("\n")
```

## Implementation

- **Algorithm**: Conservative mark-and-sweep
- **Heap**: 4 MiB in `.bss`
- **Roots**: Stack `[rbp, stack_top]` + `.rodata`
- **Allocation**: First-fit free list with splitting
- **Collection**: Triggered on allocation failure, retries once
- **Frame zeroing**: Stack frames zeroed on function return

## Usage

```dev
from std.gc import gc_alloc, gc_free, gc_collect, gc_leak_check

struct Node:
    value: int32
    next: ptr[Node]

pub def build_list() -> ptr[Node]:
    let head = gc_alloc(sizeof(Node)) as mut ptr[Node]
    (*head).value = 1
    (*head).next = 0 as ptr[Node]
    
    let mut current = head
    for i in 2..100:
        let node = gc_alloc(sizeof(Node)) as mut ptr[Node]
        (*node).value = i
        (*node).next = 0 as ptr[Node]
        (*current).next = node
        current = node
    
    return head

pub def main() -> int32:
    let list = build_list()
    # ... use list ...
    
    # Optional explicit free
    gc_free(list as mut ptr[byte])
    
    # Or let GC handle it
    gc_collect()
    
    let leaks = gc_leak_check()
    if leaks > 0:
        puts("Leaks detected!\n")
    
    return 0
```

## Conservative GC Details

- **Conservative**: Treats any word on stack as potential pointer
- **False positives possible**: Integers that look like pointers keep objects alive
- **Mitigation**: Stack frames zeroed on return reduces false roots
- **No compaction**: Heap can fragment over time

## Target Support

| Target | Available |
|--------|-----------|
| x86_64 | ✅ Full support |
| x86_32 | ❌ Not implemented |
| x86_16 | ❌ Not available |

## Safety

- **Conservative**: May retain unreachable objects (false positives)
- **Not real-time**: Collection pauses execution
- **Not generational**: Full heap scan each collection
- **No finalizers**: No destructor support

## Tuning

Heap size fixed at 4 MiB. To adjust, modify linker script or rebuild compiler.

## Example: Leak Detection

```dev
from std.gc import gc_alloc, gc_collect, gc_leak_check
from std.io import puts
from std.fmt import format_i32

pub fn leaky_function() -> void:
    let leaked = gc_alloc(100)  # Forgotten
    let not_leaked = gc_alloc(100)
    gc_free(not_leaked as mut ptr[byte])

pub def main() -> int32:
    leaky_function()
    gc_collect()
    
    let leaks = gc_leak_check()
    puts("Leaks: ")
    puts(format_i32(leaks))
    puts("\n")
    return 0
```

## Internals

### Heap Layout

```
┌─────────────────────────────────────┐
│ GC State (96 bytes)                 │
├─────────────────────────────────────┤
│ Free List Head                      │
├─────────────────────────────────────┤
│ Heap (4 MiB)                        │
│ ┌─────────────────────────────────┐ │
│ │ Block Header (8 bytes)          │ │
│ │ - Used/Mark bits                │ │
│ │ - Size                          │ │
│ ├─────────────────────────────────┤ │
│ │ Payload                         │ │
│ │ ...                             │ │
│ └─────────────────────────────────┘ │
└─────────────────────────────────────┘
```

### Block Header

```c
struct BlockHeader {
    uint64_t used:1;      // Bit 0: block in use
    uint64_t mark:1;      // Bit 1: marked during GC
    uint64_t size:62;     // Payload size (8-byte aligned)
}
```

### Collection Algorithm

1. **Mark Phase**: Scan roots (stack, registers, .rodata)
   - Conservative: any word that looks like heap pointer
   - Mark reachable blocks recursively
2. **Sweep Phase**: Walk heap
   - Unmarked + used → free (add to free list)
   - Marked → clear mark bit
3. **Retry Allocation**: If original allocation failed, retry

### Stack Roots

- Conservative scan of `[rbp, stack_top]`
- `stack_top` captured at `_start`
- Each frame zeroed on return (reduces false roots)
- Registers not scanned (conservative: could be in callee-saved)

## Limitations

- No generational collection
- No incremental collection
- No finalizers
- No weak references
- Conservative → false positives
- Single-threaded only
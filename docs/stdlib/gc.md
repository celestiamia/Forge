# std.gc

Garbage-collected heap helpers (x86_64 only).

`std.alloc`'s `alloc`/`free` run over a 4 MiB GC heap with a conservative
mark-and-sweep collector. `std.gc` exposes collection and statistics.

## Functions

### collect

```dev
def collect() -> void
```

Force a full mark-and-sweep collection. Roots are the live stack
(`[rbp, stack_top]`) and `.rodata`.

```dev
from std.gc import collect

collect()
```

### leak_check

```dev
def leak_check() -> uint64
```

Return the number of bytes currently allocated but unreachable from any live
root (i.e. dropped without `free`). Useful for leak detection in tests.

```dev
from std.gc import leak_check

let leaked = leak_check()
```

### Statistics

```dev
def alloc_count() -> uint64     # Total allocation calls
def free_count() -> uint64      # Total free calls
def collections() -> uint64     # Total collections performed
def heap_live() -> uint64       # Bytes currently live
def heap_capacity() -> uint64   # Total heap size (4 MiB)
```

## Example: Leak Detection

```dev
from std.gc import collect, leak_check
from std.alloc import alloc
from std.io import puts
from std.fmt import format_i32
from std.alloc import free

pub def main() -> int32:
    let p = alloc(64)
    # p goes out of scope without free

    let buf: ptr[char] = alloc(16)
    collect()
    format_i32(buf, leak_check() as int32)   # 64
    puts(buf)
    return 0
```

## Implementation

- **Algorithm**: conservative mark-and-sweep
- **Heap**: 4 MiB in `.bss`
- **Roots**: stack `[rbp, stack_top]` + `.rodata`
- **Allocation**: first-fit free list with splitting; 8-byte header before
  each payload (bit 0 = USED, bit 1 = MARK, rest = size)
- **Collection**: automatic when the free list is exhausted; allocation
  retries once after collecting
- **Frame zeroing**: every function frame is zeroed on return so dead frames
  cannot act as GC roots — this is what makes `leak_check()` detect dropped
  references across calls

## Conservative GC Details

- Treats any word on the stack that looks like a heap pointer as a root —
  integers that happen to look like pointers keep objects alive
  (false positives)
- No compaction — the heap can fragment over time
- Not generational, not incremental, not real-time
- No finalizers, no weak references
- Single-threaded only

## Target Support

| Target | Available |
|--------|-----------|
| x86_64 | ✅ Full support |
| x86_32 | ❌ (bump allocator only) |
| x86_16 | ❌ (no heap) |
# Standard Library

Forge's standard library lives in `core/` and is imported as `std.*`.
Modules are written in Forge itself and wrap compiler-emitted runtime helpers
or direct Linux syscalls.

## Module Overview

| Module | Description | Key Functions |
|--------|-------------|---------------|
| [`std.io`](io.md) | Input/output + syscalls | `puts`, `putchar`, `getchar`, `rand`, `exit`, `open`, `read`, `write`, `socket`, ... |
| [`std.mem`](mem.md) | Memory operations | `copy_bytes`, `set_bytes`, `zero_bytes`, `compare_bytes` |
| [`std.string`](string.md) | String manipulation | `strlen`, `strcmp`, `strncmp`, `strstr`, `strchr`, `strcat`, `strncpy` |
| [`std.math`](math.md) | Integer math | `abs_i32`, `min_i32`, `max_i32`, `clamp_i32` |
| [`std.alloc`](alloc.md) | Heap allocation | `alloc`, `free` |
| [`std.fmt`](fmt.md) | Formatting | `format_i32`, `format_f64` |
| [`std.volatile`](volatile.md) | Volatile memory | `load_u8`...`load_u64`, `store_u8`...`store_i64`, barriers |
| [`std.hal`](hal.md) | Hardware I/O (freestanding) | `outb`/`inb`/`outw`/`inw`, inline `INT nn`, `sti`/`cli`/`iret`/`halt`, `pic_init`/`pic_send_eoi` |
| [`std.gc`](gc.md) | Garbage collection (x86_64) | `collect`, `leak_check`, `alloc_count`, `heap_live`, ... |
| `std.runtime` | Runtime | `abort`, `exit` |

## Usage

```dev
from std.io import puts
from std.mem import copy_bytes

puts("hello")
```

Imports merge into a flat namespace — imported names are used directly (no
`std.io.puts(...)` qualified calls).

## Target Availability

| Module | x86_64 | x86_32 | x86_16 |
|--------|--------|--------|--------|
| `std.io` | ✅ | ✅ | ✅ (limited) |
| `std.mem` | ✅ | ✅ | ✅ |
| `std.string` | ✅ | ✅ | ✅ |
| `std.math` | ✅ | ✅ | ✅ |
| `std.alloc` | ✅ | ✅ | ❌ |
| `std.fmt` | ✅ | ✅ | ✅ |
| `std.volatile` | ✅ | ✅ | ✅ |
| `std.hal` | ✅ | ✅ | ✅ (word/byte I/O; no `_dev_outl`/`_dev_inl`) |
| `std.gc` | ✅ | ❌ | ❌ |
| `std.runtime` | ✅ | ✅ | ❌ |

> Importing a `std.*` module compiles the **entire** module file. On x86_32,
> avoid modules that use `float64` (e.g. importing `std.gc`'s helpers or any
> future float-using module) — floats are unsupported on that target.

## Error Handling

Most functions do not return errors — they abort on failure:

```dev
puts("hello")     # Never fails (writes to stdout)
alloc(size)       # Aborts on OOM
```

A few return status codes:

```dev
getchar() -> int32   # Returns -1 on EOF
```

## Extending

Add new modules to `core/`:

```
core/
├── io.dev
├── mem.dev
└── mymodule.dev    # New module
```

Then import with `from std.mymodule import ...`.
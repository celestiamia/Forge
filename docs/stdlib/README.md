# Standard Library

Forge's standard library provides essential modules for systems programming.

## Module Overview

| Module | Description | Key Functions |
|--------|-------------|---------------|
| [`std.io`](io.md) | Input/Output | `puts`, `putchar`, `getchar`, `rand` |
| [`std.mem`](mem.md) | Memory operations | `copy_bytes`, `zero_bytes`, `compare_bytes` |
| [`std.string`](string.md) | String manipulation | `strlen`, `strcmp`, `strncmp` |
| [`std.math`](math.md) | Mathematical functions | `abs_i32`, `min_i32`, `max_i32`, `clamp_i32` |
| [`std.alloc`](alloc.md) | Heap allocation | `alloc`, `free`, `bump_alloc` |
| [`std.fmt`](fmt.md) | Formatting | `format_i32` |
| [`std.volatile`](volatile.md) | Volatile memory | `volatile_load`, `volatile_store`, barriers |
| [`std.gc`](gc.md) | Garbage collection | `gc_alloc`, `gc_free`, `gc_collect` |
| `std.runtime` | Runtime | `abort`, `exit` |

## Usage

```dev
# Import specific functions
from std.io import puts, getchar
from std.mem import copy_bytes

# Or import module (not all modules support this)
import std.io
std.io.puts("hello")
```

## Design Principles

- **Minimal**: Only essential functionality
- **No runtime**: Most functions compile to direct syscalls or inline code
- **Zero-cost abstractions**: No overhead vs manual implementation
- **Target-aware**: Some modules only on specific targets (e.g., `std.gc` x86_64 only)

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
| `std.gc` | ✅ | ❌ | ❌ |
| `std.runtime` | ✅ | ✅ | ❌ |

## Importing

```dev
# Recommended: specific imports
from std.io import puts, putchar
from std.mem import copy_bytes, zero_bytes

# Module import (limited support)
import std.io
std.io.puts("hello")
```

## Naming Conventions

- Functions: `snake_case` (`puts`, `copy_bytes`)
- Types: `PascalCase` (in type definitions)
- Constants: `SCREAMING_SNAKE` (rare in stdlib)

## Error Handling

Most stdlib functions don't return errors - they abort on failure:

```dev
# These abort on failure:
puts("hello")        # Never fails (writes to stdout)
alloc(size)          # Aborts if OOM
gc_alloc(size)       # Aborts if OOM

# These return error codes:
getchar() -> int32   # Returns -1 on EOF
```

## Extending

Add new modules to `core/` directory:

```
core/
├── io.dev
├── mem.dev
└── mymodule.dev    # New module
```

Then import: `from std.mymodule import ...`
# std.fmt

Formatting utilities for converting values to strings.

## Functions

### format_i32

```dev
def format_i32(buf: ptr[char], value: int32) -> uint64
```

Format a signed 32-bit integer as a decimal string into `buf`. Writes a null
terminator. Returns the number of characters written (excluding the null
terminator).

```dev
from std.fmt import format_i32
from std.alloc import alloc
from std.io import puts

pub def main() -> int32:
    let buf: ptr[char] = alloc(16)
    let len = format_i32(buf, -42)
    # buf == "-42\0", len == 3
    puts(buf)
    free(buf)
    return 0
```

### format_f64

```dev
def format_f64(buf: ptr[char], value: float64) -> uint64
```

Format a 64-bit float as a decimal string into `buf`. Writes a null
terminator. Returns the number of characters written. x86_64 only.

```dev
let len = format_f64(buf, 3.14)
```

## Buffer Requirements

- The buffer must be large enough for the result plus the null terminator
- Maximum for `int32`: 12 bytes ("-2147483648" + null)
- Allocate generously (16 bytes is a safe default)

## Implementation Notes

- Pure software implementation (no syscalls)
- Division-based algorithm, not optimized for speed
- No locale support (always decimal, no thousands separator)
- No padding or alignment options

## Planned Extensions

`format_u32`, `format_i64`, `format_u64`, `format_ptr`, and `sprintf`-style
formatting are future work.

## Target Support

All targets: pure software, no syscalls needed (`format_f64` is x86_64 only).
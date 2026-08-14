# std.fmt

Formatting utilities for converting values to strings.

## Functions

### format_i32

```dev
def format_i32(value: int32, buf: mut ptr[char], buf_len: usize) -> usize
```

Format signed 32-bit integer as decimal string into buffer.

Returns number of characters written (excluding null terminator).

```dev
from std.fmt import format_i32
from std.io import puts

pub def main() -> int32:
    let mut buf = [0; 16]
    let len = format_i32(-42, &mut buf[0], 16)
    # buf = "-42\0...", len = 3
    puts(buf)  # prints "-42"
    return 0
```

### format_u32 (Planned)

```dev
def format_u32(value: uint32, buf: mut ptr[char], buf_len: usize) -> usize
```

Format unsigned 32-bit integer.

### format_i64 / format_u64 (Planned)

```dev
def format_i64(value: int64, buf: mut ptr[char], buf_len: usize) -> usize
def format_u64(value: uint64, buf: mut ptr[char], buf_len: usize) -> usize
```

## Usage

```dev
from std.fmt import format_i32
from std.io import puts

def print_number(n: int32):
    let mut buf = [0; 16]
    let len = format_i32(n, &mut buf[0], 16)
    puts(buf)
    puts("\n")

print_number(42)       # "42"
print_number(-123)     # "-123"
print_number(0)        # "0"
```

## Buffer Requirements

- Buffer must be large enough for result + null terminator
- Maximum for `int32`: 12 chars ("-2147483648" + null = 12)
- Minimum buffer: 12 bytes for `int32`
- Function writes null terminator
- Returns actual length written (excl. null)

## Implementation Notes

- Pure software implementation (no syscalls)
- Division-based algorithm (not optimized for speed)
- No locale support (always decimal, no thousands separator)
- No padding/alignment options

## Example: Print Multiple Numbers

```dev
from std.fmt import format_i32
from std.io import puts, putchar

pub def print_array(arr: ptr[int32], len: usize) -> void:
    let mut buf = [0; 16]
    for i in 0..len:
        let len_written = format_i32(arr[i], &mut buf[0], 16)
        puts(buf)
        if i + 1 < len:
            putchar(' ')
    putchar('\n')

pub def main() -> int32:
    let arr = [1, -2, 3, -4, 5]
    print_array(&arr[0], 5)
    return 0
```

## Planned Extensions

| Function | Description |
|----------|-------------|
| `format_u32` | Unsigned 32-bit |
| `format_i64` | Signed 64-bit |
| `format_f32` | Float with precision |
| `format_ptr` | Pointer as hex |
| `sprintf` | Full printf-style formatting |
| `to_string` | Allocating string conversion |

## Target Support

All targets: pure software, no syscalls needed.

## Example: Custom Formatting

```dev
from std.fmt import format_i32
from std.io import puts

def print_hex(n: int32) -> void:
    let mut buf = [0; 16]
    let mut n = n
    let mut i = 0
    if n == 0:
        puts("0x0")
        return
    puts("0x")
    while n != 0:
        let digit = n & 0xF
        let c = if digit < 10 { '0' as int32 + digit } else { 'A' as int32 + digit - 10 }
        buf[i] = c as char
        i = i + 1
        n = n >> 4
    # Reverse
    for j in 0..i/2:
        let tmp = buf[j]
        buf[j] = buf[i - 1 - j]
        buf[i - 1 - j] = tmp
    buf[i] = 0
    puts(buf)

pub def main() -> int32:
    print_hex(255)    # 0xFF
    putchar('\n')
    print_hex(4096)   # 0x1000
    putchar('\n')
    return 0
```
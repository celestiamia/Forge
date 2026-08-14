# std.math

Mathematical functions for integers and floats.

## Integer Functions

### abs_i32

```dev
def abs_i32(x: int32) -> int32
```

Absolute value. Returns `INT32_MIN` for `INT32_MIN` (overflow).

```dev
from std.math import abs_i32

abs_i32(-42)   # 42
abs_i32(42)    # 42
abs_i32(0)     # 0
```

### min_i32 / max_i32

```dev
def min_i32(a: int32, b: int32) -> int32
def max_i32(a: int32, b: int32) -> int32
```

Return minimum/maximum of two values.

```dev
from std.math import min_i32, max_i32

min_i32(10, 20)   # 10
max_i32(10, 20)   # 20
```

### clamp_i32

```dev
def clamp_i32(v: int32, lo: int32, hi: int32) -> int32
```

Clamp value to range `[lo, hi]`. Requires `lo <= hi`.

```dev
from std.math import clamp_i32

clamp_i32(5, 0, 10)    # 5
clamp_i32(-5, 0, 10)   # 0
clamp_i32(15, 0, 10)   # 10
```

## Float Functions (x86_64 only)

Not yet implemented. Planned:
- `abs_f32`, `abs_f64`
- `sqrt_f32`, `sqrt_f64`
- `sin_f32`, `cos_f32`, `tan_f32`
- `floor_f32`, `ceil_f32`
- `pow_f32`, `log_f32`

## Implementation Notes

- Integer functions are pure and inlineable
- No runtime dependencies
- `abs_i32` handles `INT32_MIN` by returning `INT32_MIN` (two's complement)
- `clamp_i32` assumes `lo <= hi` (undefined behavior otherwise)

## Example

```dev
from std.math import abs_i32, min_i32, max_i32, clamp_i32
from std.io import puts
from std.fmt import format_i32

pub def main() -> int32:
    let x = -42
    puts("abs: ")
    puts(format_i32(abs_i32(x)))
    puts("\n")
    
    puts("clamp: ")
    puts(format_i32(clamp_i32(15, 0, 10)))
    puts("\n")
    return 0
```
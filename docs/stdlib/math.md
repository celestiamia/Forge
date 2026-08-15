# std.math

Integer math helpers.

## Functions

### abs_i32

```dev
def abs_i32(x: int32) -> int32
```

Absolute value. Returns `INT32_MIN` for `INT32_MIN` (overflow, two's
complement).

```dev
from std.math import abs_i32

abs_i32(-42)   # 42
abs_i32(42)    # 42
```

### min_i32 / max_i32

```dev
def min_i32(a: int32, b: int32) -> int32
def max_i32(a: int32, b: int32) -> int32
```

Return the minimum/maximum of two values.

```dev
from std.math import min_i32, max_i32

min_i32(10, 20)   # 10
max_i32(10, 20)   # 20
```

### clamp_i32

```dev
def clamp_i32(v: int32, lo: int32, hi: int32) -> int32
```

Clamp `v` to the range `[lo, hi]`. Assumes `lo <= hi`.

```dev
from std.math import clamp_i32

clamp_i32(5, 0, 10)    # 5
clamp_i32(-5, 0, 10)   # 0
clamp_i32(15, 0, 10)   # 10
```

## Float Functions

Not yet implemented. Planned: `abs_f64`, `sqrt_f64`, `sin_f64`, `cos_f64`,
`floor_f64`, `pow_f64`, etc. Floating-point arithmetic itself works on x86_64
via the language operators.

## Implementation Notes

- Pure functions, no runtime dependencies
- `clamp_i32` assumes `lo <= hi` (behavior is otherwise undefined)
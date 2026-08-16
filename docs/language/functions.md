# Functions

Forge functions use Python-like definitions with static parameter types.

## Function Definition

```dev
def name(params) -> return_type:
    body
```

```dev
def add(a: int32, b: int32) -> int32:
    return a + b
```

- Parameters must be annotated
- Return type is optional; omitted means `void`
- Nested function definitions are **not** supported
- Functions cannot be overloaded — each name must be unique

### Visibility

```dev
def private_fn() -> int32:    # No pub
    return 0

pub def public_fn() -> int32: # pub
    return 0
```

`pub` marks an item as public. Note that the module loader currently merges
**all** items from imported modules — private and public alike — so visibility
is informational only and is not enforced at import time.

### Parameters

```dev
# By value (copy)
def by_value(x: int32) -> int32:
    return x

# By pointer (mutable access via unsafe)
def by_ptr(x: ptr[int32]) -> int32:
    unsafe:
        return *x
```

Pass a local's address with `&x` — it coerces to `ptr[T]` at the call site:

```dev
var n = 42
let v = by_ptr(&n)   # v == 42
```

### Return Types

```dev
def returns_int() -> int32:
    return 42

def returns_void() -> void:
    return
```

## Main Function

```dev
# Hosted (default)
pub def main() -> int32:
    return 0
```

In hosted targets the runtime `_start` calls `main` and uses its return value
as the process exit code.

Freestanding targets use a custom entry point with no runtime:

```dev
@freestanding
pub def _start() -> void:
    ...
```

## Extern Functions

Declare foreign functions. The `extern def` declaration has no body and maps
to a symbol the runtime (or the linker) provides:

```dev
extern def _dev_halt() -> void
extern def puts(s: ptr[char]) -> int32
```

`@extern(abi)` sets the ABI annotation; `@extern("c")` is the default.

## Recursion

```dev
def factorial(n: int32) -> int32:
    if n <= 1:
        return 1
    return n * factorial(n - 1)
```

Tail recursion is not optimized — deep recursion may overflow the stack.

## Not Yet Supported

The following are **not** available in this milestone (see
[Known Issues](known-issues.md)):

- **Generics** — `def identity[T](x: T) -> T` parses but panics the compiler at lowering
- **Methods / `impl` blocks** — `impl` panics the compiler; there is no `obj.method()` call syntax
- **Function types** — `fn(int32) -> int32` annotations fail to parse
- **Function pointers** — passing functions as values
- **Closures / lambdas** — `|x| x + 1`
- **Variadic functions** — `extern def printf(fmt, ...)`
- **Default parameters**
- **Tuples as return types** — `-> (int32, int32)` type-checks, but the
  returned tuple type is not propagated through the call (the caller sees
  `int64`); use an intermediate `ref[T]` binding or field access when possible
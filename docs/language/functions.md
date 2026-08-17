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

Struct and tuple return types work end-to-end: the caller allocates a scratch
slot and the callee writes the result through a hidden first argument (return
by pointer). See [Known Issues](known-issues.md) for ABI details.

```dev
def make_point(x: int64, y: int64) -> Point:
    var p: Point
    p.x = x
    p.y = y
    return p

def pair() -> (int64, int64):
    return (1, 2)
```

## Generic Functions

Functions can declare type parameters and are monomorphized per call site:

```dev
def id[T](x: T) -> T:
    return x

def swap[T](a: T, b: T) -> T:
    var t: T = b
    return t

def make_pair[T](a: T, b: T) -> Pair[T]:
    return Pair[T] { first: a, second: b }

def sum_pair[T](p: Pair[T]) -> T:
    return p.first + p.second
```

- Concrete type arguments are **inferred from the argument types** at each
  call site — there is no explicit call-site syntax (`id[int64](x)` is not
  parsed)
- A generic function instantiated with the same type arguments generates one
  monomorphized function (mangled, e.g. `id$i64`); distinct instantiations
  share nothing
- Generic parameters can be used in parameter types, return types, local
  annotations (`var t: T`), and generic struct literals (`Pair[T] { ... }`)
- Generic functions can return and consume generic structs (`make_pair`,
  `sum_pair` above)
- Nested generics work: `Pair[Pair[int64]]`
- Supported on both x86_64 and x86_32 (no floats on x86_32, as usual)
- Limitations: no explicit type arguments at call sites; union/enum types
  are not supported in generic signatures; generic functions cannot be
  overloaded by name

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

- **Methods / `impl` blocks** — `impl` panics the compiler; there is no `obj.method()` call syntax
- **Function types** — `fn(int32) -> int32` annotations fail to parse
- **Function pointers** — passing functions as values
- **Closures / lambdas** — `|x| x + 1`
- **Variadic functions** — `extern def printf(fmt, ...)`
- **Default parameters**
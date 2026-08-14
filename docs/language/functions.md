# Functions

Functions are first-class in Forge with support for generics, methods, and extern declarations.

## Function Definition

```dev
def name(params) -> return_type:
    body
```

```dev
def add(a: int32, b: int32) -> int32:
    return a + b
```

### Visibility

```dev
def private_fn() -> int32:    # Module-private
    return 0

pub def public_fn() -> int32: # Public (exported)
    return 0
```

### Parameters

```dev
# By value (copy)
def by_value(x: int32) -> int32:

# By pointer (mutable)
def by_ptr(x: ptr[int32]):

# By reference (immutable borrow)
def by_ref(x: ref[int32]):

# By mutable reference (exclusive borrow)
def by_mut(x: refmut[int32]):
```

### Return Types

```dev
def returns_int() -> int32:
    return 42

def returns_void() -> void:    # or omit
    return

def returns_tuple() -> (int32, int32):
    return (1, 2)
```

### Generic Functions

```dev
pub def identity<T>(x: T) -> T:
    return x

pub def first<T>(a: T, b: T) -> T:
    return a

# Usage
let x = identity(42)      # T = int32
let y = identity(3.14)    # T = float64
```

Multiple type parameters:

```dev
pub def pair<A, B>(a: A, b: B) -> (A, B):
    return (a, b)
```

## Methods

Methods are defined in `impl` blocks:

```dev
struct Point:
    x: int32
    y: int32

impl Point:
    # Constructor
    pub def new(x: int32, y: int32) -> Point:
        return Point { x: x, y: y }

    # Method with self by value
    pub def distance(self) -> float64:
        return sqrt((self.x * self.x + self.y * self.y) as float64)

    # Method with self by reference
    pub def translate(ref self, dx: int32, dy: int32):
        self.x = self.x + dx
        self.y = self.y + dy
```

### Method Receivers

| Receiver | Description |
|----------|-------------|
| `self` | By value (consumes) |
| `ref self` | Immutable borrow |
| `refmut self` | Mutable borrow |

## Extern Functions

Declare foreign functions (C ABI by default):

```dev
extern def puts(s: ptr[char]) -> int32

@extern("c")
extern def malloc(size: usize) -> ptr[void]

@extern("c")
extern def free(ptr: ptr[void]) -> void
```

### Calling Convention

```dev
@extern("c")      # C calling convention (default)
@extern("sysv64") # System V AMD64 ABI
```

### Variadic Functions

```dev
extern def printf(fmt: ptr[char], ...) -> int32
```

## Higher-Order Functions

Function types: `fn(Args) -> Return`

```dev
let add: fn(int32, int32) -> int32 = add
let f: fn(int32) -> int32 = |x| x + 1  # closure not yet supported
```

Passing functions:

```dev
def apply(f: fn(int32) -> int32, x: int32) -> int32:
    return f(x)

def add_one(x: int32) -> int32:
    return x + 1

let result = apply(add_one, 5)  # 6
```

## Function Attributes

```dev
@inline          # Hint to inline
@export          # Force export symbol
@noreturn        # Function never returns
@naked           # No prologue/epilogue (asm)
```

```dev
@inline
pub def small_fn() -> int32:
    return 1

@export
pub fn c_compatible() -> int32:
    return 0
```

## Recursion

```dev
def factorial(n: int32) -> int32:
    if n <= 1:
        return 1
    return n * factorial(n - 1)
```

Tail recursion not optimized (may stack overflow).

## Variadic Functions (Forge-side)

Not yet supported for user-defined functions. Only `extern`.

## Closures / Lambdas

Not yet implemented. Planned syntax:

```dev
let add = |a, b| a + b
let closure = |x| x + captured_var
```

## Function Pointers

```dev
let ptr: fn(int32) -> int32 = add
ptr(42)

# From extern
extern def callback(cb: fn(int32) -> int32):
    ...
```

## Main Function

```dev
# Hosted (default)
pub def main() -> int32:
    return 0

# Freestanding (boot sector)
@freestanding
pub def _start() -> void:
    ...
```

Hosted mode: `main` called by runtime `_start` with argc/argv.

## Variadic Arguments

Not supported for user functions. Use slices:

```dev
def sum(values: slice[int32]) -> int32:
    var total = 0
    for v in values:
        total = total + v
    return total
```

## Inlining

`@inline` is a hint. Compiler may ignore.

```dev
@inline
pub def hot_path() -> int32:
    return 1
```

## No Overloading

Functions cannot be overloaded by parameter types. Use different names or generics.

## Default Parameters

Not supported. Use option types or builder pattern:

```dev
enum Option<T>:
    Some(T)
    None

def foo(x: int32, y: Option<int32>) -> int32:
    match y:
        case Some(v): return x + v
        case None: return x
```
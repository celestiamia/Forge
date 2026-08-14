# Syntax Overview

Forge uses Python-like indentation-based syntax with static typing.

## Source File Structure

```dev
package <name>                    # Optional package declaration

from <module> import <items>      # Imports
import <module>                   # Import all public items

pub def <name>(<params>) -> <ret>: # Function definitions
    <body>

struct <Name>:                    # Struct definitions
    <field>: <type>
    ...
```

## Lexical Elements

### Identifiers

```dev
my_var          # snake_case for variables/functions
MyStruct        # PascalCase for types
CONSTANT        # SCREAMING_SNAKE for constants
```

Rules: Start with letter/underscore, followed by letters/digits/underscores. Unicode not supported.

### Keywords

```
def      let      var      if       elif     else
for      while    loop     match    case     break
continue return   import   from     as       pub
extern   unsafe   struct   enum     union    const
package  true     false    void     int8     int16
int32    int64    uint8    uint16   uint32   uint64
float32  float64  char     bool     ptr      own
ref      refmut
```

### Literals

```dev
42              # int32 (default)
42i8            # typed integer
42u32           # unsigned
3.14            # float64 (default)
3.14f32         # float32
'a'             # char (single quotes)
"hello"         # string (double quotes)
true / false    # bool
b"bytes"        # byte array
```

### Comments

```dev
# Single line comment

# No multi-line comment syntax yet
```

## Indentation Rules

- **4 spaces per level** (tabs forbidden)
- Consistent indentation required
- Blank lines allowed within blocks
- Dedent closes block

```dev
def foo() -> int32:
    let x = 1
    if x > 0:
        puts("positive")
    else:
        puts("non-positive")
# dedent here ends function
```

## Operators

### Arithmetic

| Operator | Description | Types |
|----------|-------------|-------|
| `+` | Addition | int, float |
| `-` | Subtraction | int, float |
| `*` | Multiplication | int, float |
| `/` | Division (truncating for int) | int, float |
| `//` | Floor division (toward -∞) | int |
| `%` | Modulo | int |
| `**` | Power (int only) | int |

### Comparison

| Operator | Description |
|----------|-------------|
| `==` `!=` | Equality |
| `<` `<=` `>` `>=` | Ordering |

### Logical

| Operator | Description |
|----------|-------------|
| `&&` | Logical AND (short-circuit) |
| `\|\|` | Logical OR (short-circuit) |
| `!` | Logical NOT |

### Bitwise

| Operator | Description |
|----------|-------------|
| `&` `\|` `^` | AND, OR, XOR |
| `<<` `>>` | Shift left/right |
| `~` | Bitwise NOT |

### Assignment

| Operator | Description |
|----------|-------------|
| `=` | Assignment |
| `+=` `-=` `*=` `/=` `%=` | Compound assignment |

### Other

| Operator | Description |
|----------|-------------|
| `.` | Field access |
| `[]` | Index access |
| `as` | Cast: `x as int32` |
| `:` | Type annotation |
| `->` | Return type |
| `::` | Path separator (imports) |

## Precedence (Highest to Lowest)

1. `()` `[]` `.` `as` (postfix)
2. `**` (right-associative)
3. `!` `~` `-` (unary)
4. `*` `/` `//` `%` `<<` `>>` `&` `^` `|`
5. `+` `-`
6. `==` `!=` `<` `<=` `>` `>=`
7. `&&`
7. `\|\|`
8. `=` `+=` `-=` `*=` `/=` `%=` (right-associative)

## Expressions

### Variables

```dev
let x = 42           # Immutable
var y = 10           # Mutable
let z: int32 = 5     # Explicit type
```

### Function Calls

```dev
puts("hello")
max(10, 20)
foo(1, bar(2))
```

### Casts

```dev
let x: int64 = 42
let y: int32 = x as int32
```

### Field Access

```dev
struct Point: x: int32, y: int32
let p = Point { x: 1, y: 2 }
let x = p.x
```

### Indexing

```dev
let arr = [1, 2, 3]
let x = arr[0]
arr[1] = 5
```

### Blocks

```dev
let result = {
    let x = 1
    let y = 2
    x + y  # Last expression is value
}
```

## Statements

### If / Elif / Else

```dev
if x > 0:
    puts("positive")
elif x < 0:
    puts("negative")
else:
    puts("zero")
```

### While Loop

```dev
var i = 0
while i < 10:
    puts("iteration")
    i = i + 1
```

### For Loop

```dev
# Range-based (inclusive..exclusive)
for i in 0..10:
    puts(i)
```

### Loop / Break / Continue

```dev
loop:
    if condition:
        break
    if skip:
        continue
```

### Match Expression

```dev
match x:
    case 0: puts("zero")
    case 1: puts("one")
    case _: puts("other")  # Wildcard
```

### Return

```dev
return 42
return          # void return
```

### Unsafe Block

```dev
unsafe:
    let ptr = 0x1000 as ptr[int32]
    *ptr = 42
```

## Items

### Functions

```dev
# Private
def add(a: int32, b: int32) -> int32:
    return a + b

# Public
pub def main() -> int32:
    return 0

# Extern (foreign function)
extern def puts(s: ptr[char]) -> int32

# Generic
pub def identity<T>(x: T) -> T:
    return x
```

### Structs

```dev
struct Point:
    x: int32
    y: int32

# With methods
impl Point:
    pub def new(x: int32, y: int32) -> Point:
        return Point { x: x, y: y }

    pub def distance(self) -> float64:
        return sqrt((self.x * self.x + self.y * self.y) as float64)
```

### Enums

```dev
enum Option<T>:
    Some(T)
    None

# Pattern matching
match opt:
    case Some(v): puts("got value")
    case None: puts("empty")
```

### Constants

```dev
const PI: float64 = 3.14159265359
const MAX_SIZE: int32 = 1024
```

### Packages

```dev
package mylib

# Items in this file belong to `mylib` namespace
```

## Attributes

```dev
@freestanding          # No stdlib, custom entry point
@extern("c")           # C calling convention
@export                # Export symbol
@inline                # Hint to inline
```

## Module Resolution

```dev
# Standard library
from std.io import puts

# Relative to entry file
import utils
from mylib import helper

# Package-qualified
from mypkg.sub import foo
```

Resolution: Walk up from entry file's directory looking for `.dev` files.

## Type Annotations

```dev
# Basic
let x: int32 = 42

# Pointer
let p: ptr[int32] = &x

# Array
let arr: [int32; 10] = [0; 10]

# Slice
let s: slice[int32] = &arr[..]

# Function
let f: fn(int32) -> int32 = add

# Tuple
let t: (int32, float64) = (1, 2.0)
```
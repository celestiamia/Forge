# Syntax Overview

Forge uses Python-like indentation-based syntax with static typing.

## Source File Structure

```dev
package <name>                    # Optional package declaration

from <module> import <items>      # Import specific items
import <module>                   # Import a module (all items merged)

pub def <name>(<params>) -> <ret>: # Function definitions
    <body>

struct <Name>:                    # Struct definitions
    <field>: <type>
```

## Lexical Elements

### Identifiers

```dev
my_var          # snake_case for variables/functions
MyStruct        # PascalCase for types
```

Rules: start with a letter or underscore, followed by letters, digits, or
underscores. Unicode is not supported.

### Keywords

```
def      let      var      if       elif     else
for      while    loop     match    case     break
continue return   import   from     as       pub
extern   unsafe   struct   enum     union    const
package  true     false    void     asm
```

### Literals

```dev
42              # int32 (default)
42i8            # typed integer suffix
3.14            # float64 (default)
3.14f32         # float32 suffix
0x1A            # hexadecimal
0b1010          # binary
'a'             # char (single quotes)
"hello"         # string literal (ptr[char], null-terminated)
true / false    # bool
```

### Comments

```dev
# Single line comment
```

There is no multi-line comment syntax.

## Indentation Rules

- **4 spaces per level** (tabs forbidden)
- Consistent indentation required
- Blank lines are allowed within blocks
- A dedent closes the block

```dev
def foo() -> int32:
    let x = 1
    if x > 0:
        puts("positive")
    else:
        puts("non-positive")
# dedent here ends the function
```

## Operators

### Arithmetic

| Operator | Description | Operands |
|----------|-------------|----------|
| `+` `-` `*` | Addition, subtraction, multiplication | int, float |
| `/` | Division (truncating for int) | int, float |
| `//` | Floor division (toward -infinity) | int |
| `%` | Modulo | int |
| `**` | Power (integer only) | int |

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

Compound assignment (`+=`, `-=`, etc.) is **supported** — `x += 1` is
desugared to `x = x + 1`.

### Other

| Operator | Description |
|----------|-------------|
| `.` | Field access |
| `[]` | Index access |
| `as` | Postfix cast: `x as int32` |
| `:` | Type annotation |
| `->` | Return type |
| `&` | Address-of (produces a reference to a local) |

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
foo(1, bar(2))
```

### Casts

```dev
let x: int64 = 42
let y: int32 = x as int32
```

### Field Access

```dev
struct Point:
    x: int32
    y: int32

let p = Point { x: 1, y: 2 }
let x = p.x
```

Field access on a `ptr[Struct]` parameter dereferences automatically.

### Indexing

```dev
let arr = [1, 2, 3]
let x = arr[0]
var a = [1, 2, 3]
a[1] = 5
```

### Block Expressions

A brace-enclosed sequence of statements that evaluates to the value of its
trailing expression:

```dev
var x: int32 = {
    var a = 6
    var b = 7
    a * b
}
```

Can be used anywhere an expression is expected — as initializers, function
arguments, conditions, etc.  `unsafe` blocks work the same way as expressions.
See [Control Flow](control-flow.md#block-expressions) for details.

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
# Range-based: start..end (exclusive end)
for i in 0..10:
    puts(i)

# Inclusive end
for i in 0..=10:
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

`break` and `continue` also work in `while` and `for` loops.

### Match Expression

```dev
match x:
    case 0: puts("zero")
    case 1: puts("one")
    case _: puts("other")  # Wildcard
```

Match cases compare against integer/char literal values; the `_` wildcard
catches everything else. Cases are evaluated top-to-bottom.

### Return

```dev
return 42
return          # void return
```

### Unsafe Block

```dev
unsafe:
    let p = 0x1000 as ptr[int32]
    *p = 42
```

Raw pointer dereference and pointer arithmetic must live inside `unsafe`.

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
```

### Structs

```dev
struct Point:
    x: int32
    y: int32
```

Struct fields are laid out sequentially without padding.

### Constants

```dev
const MAX_SIZE: int32 = 1024
```

### Enums

```dev
enum Color:
    Red
    Green
    Blue

enum Option:
    None
    Some(int32)
```

Variants are constructed by field access on the enum type: `Color.Red`,
`Option.Some(42)`. They can be matched with `match`/`case`; payload variants
destructure their payload (`case Option.Some(x):`). See
[Control Flow › Match Expression](control-flow.md#match-expression) and
[Type System › Enums](types.md#enums--unions).

### Attributes

```dev
@freestanding          # No hosted runtime; custom entry point
@packed                # Accepted; no layout effect yet
@align(8)              # Accepted; no layout effect yet
@c_enum                # Accepted; no effect yet
@extern("c")           # ABI annotation on extern functions
```

## Module Resolution

```dev
# Standard library
from std.io import puts

# Relative to entry file
import utils
from mylib import helper
```

All imported modules are merged into a **single flat namespace**. See
[Modules & Imports](modules.md).

## Type Annotations

```dev
let x: int32 = 42
let p: ptr[int32] = &x
```

See [Type System](types.md) for the full type list.
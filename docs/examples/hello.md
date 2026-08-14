# Hello World Example

Simplest Forge program demonstrating basic syntax and stdlib usage.

## Source: `examples/hello.dev`

```dev
package hello

from std.io import puts

pub def main() -> int32:
    puts("Hello, Forge!\n")
    return 0
```

## Breakdown

| Line | Explanation |
|------|-------------|
| `package hello` | Optional package name |
| `from std.io import puts` | Import `puts` from standard library |
| `pub def main() -> int32:` | Public function `main` returning `int32` |
| `puts("Hello, Forge!\n")` | Call imported function |
| `return 0` | Exit code 0 (success) |

## Compile & Run

```bash
# Compile for x86_64 (native)
forgec examples/hello.dev -o hello --target x86_64-unknown-linux-gnu

# Run
./hello
```

Output:
```
Hello, Forge!
```

## For x86_32

```bash
# Requires 32-bit toolchain
forgec examples/hello.dev -o hello32 --target x86_32-unknown-linux-gnu
./hello32
```

## Key Concepts Demonstrated

1. **Package declaration** - Optional namespace
2. **Standard library import** - `from std.io import puts`
3. **Function definition** - `def name(params) -> ret`
4. **Public visibility** - `pub` keyword
4. **String literals** - Double-quoted, null-terminated
5. **Return statement** - Explicit return value

## Extended Example

```dev
package hello

from std.io import puts, putchar
from std.math import abs_i32
from std.fmt import format_i32

pub def main() -> int32:
    # String output
    puts("Hello, Forge!\n")
    
    # Character output
    putchar('A')
    putchar('\n')
    
    # Math + formatting
    let x = -42
    let mut buf = [0; 16]
    let len = format_i32(abs_i32(x), &mut buf[0], 16)
    puts("Absolute value: ")
    puts(buf)
    putchar('\n')
    
    return 0
```

Output:
```
Hello, Forge!
A
Absolute value: 42
```

## Variations

### Without Package

```dev
from std.io import puts

def main() -> int32:
    puts("No package!\n")
    return 0
```

### Multiple Functions

```dev
from std.io import puts

def greet(name: ptr[char]) -> void:
    puts("Hello, ")
    puts(name)
    puts("!\n")

pub def main() -> int32:
    greet("World")
    greet("Forge")
    return 0
```

### Using Standard Library

```dev
from std.io import puts
from std.string import strlen
from std.mem import copy_bytes

pub def main() -> int32:
    let msg = "Hello, Forge!"
    puts("Length: ")
    let mut buf = [0; 16]
    format_i32(strlen(msg) as int32, &mut buf[0], 16)
    puts(buf)
    puts("\n")
    return 0
```
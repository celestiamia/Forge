# Hello World Example

The simplest Forge program, demonstrating basic syntax and stdlib usage.

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
| `package hello` | Optional package name (informational) |
| `from std.io import puts` | Import `puts` from the standard library |
| `pub def main() -> int32:` | Public function `main` returning `int32` |
| `puts("Hello, Forge!\n")` | Call the imported function |
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
# Requires a 32-bit forgec build (see Installation)
forgec examples/hello.dev -o hello32 --target x86_32-unknown-linux-gnu
./hello32
```

## Key Concepts Demonstrated

1. **Package declaration** — optional, informational
2. **Standard library import** — `from std.io import puts`
3. **Function definition** — `def name(params) -> ret`
4. **Public visibility** — `pub` keyword
5. **String literals** — double-quoted, null-terminated (`ptr[char]`)
6. **Return statement** — the return value becomes the exit code

## Extended Example

```dev
package hello

from std.io import puts, putchar
from std.math import abs_i32
from std.fmt import format_i32
from std.alloc import alloc, free

pub def main() -> int32:
    # String output
    puts("Hello, Forge!\n")

    # Character output
    putchar('A' as int32)
    putchar(10)   # newline

    # Math + formatting
    let buf: ptr[char] = alloc(16)
    let len = format_i32(buf, abs_i32(-42))
    puts("Absolute value: ")
    puts(buf)
    putchar(10)
    free(buf)
    return 0
```

Output:

```
Hello, Forge!
A
Absolute value: 42
```

## Variations

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

### Using the Standard Library

```dev
from std.io import puts
from std.string import strlen
from std.fmt import format_i32
from std.alloc import alloc

pub def main() -> int32:
    let msg = "Hello, Forge!"
    let buf: ptr[char] = alloc(16)
    format_i32(buf, strlen(msg) as int32)
    puts("Length: ")
    puts(buf)
    puts("\n")
    return 0
```
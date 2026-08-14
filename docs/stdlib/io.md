# std.io

Input/Output functions for console and basic file operations.

## Functions

### puts

```dev
def puts(s: ptr[char]) -> void
```

Write null-terminated string to stdout. Appends no extra characters.

```dev
from std.io import puts

puts("Hello, World!\n")
```

### putchar

```dev
def putchar(c: char) -> void
```

Write single character to stdout.

```dev
from std.io import putchar

putchar('A')
putchar('\n')
```

### getchar

```dev
def getchar() -> int32
```

Read single character from stdin. Returns `-1` on EOF.

```dev
from std.io import getchar

var c = getchar()
while c != -1:
    putchar(c as char)
    c = getchar()
```

### rand

```dev
def rand() -> int32
```

Generate pseudo-random 31-bit integer (0 to 2^31-1). Linear congruential generator.

```dev
from std.io import rand

let r = rand()          # 0 to 2147483647
let dice = rand() % 6 + 1  # 1 to 6
```

### exit

```dev
def exit(code: int32) -> void
```

Terminate process with exit code. Does not return.

```dev
from std.io import exit

exit(0)   # Success
exit(1)   # Error
```

## Implementation Notes

- **x86_64/x86_32**: Direct Linux syscalls (`write`, `read`, `exit`)
- **x86_16**: BIOS interrupts (INT 10h for teletype, INT 16h for keyboard)
- No buffering - each call is a syscall
- Not thread-safe (no threading support yet)

## Example: Echo Program

```dev
from std.io import getchar, putchar, puts

pub def main() -> int32:
    puts("Echo mode. Type 'q' to quit.\n")
    loop:
        let c = getchar()
        if c == -1:
            break
        if c == 'q' as int32:
            break
        putchar(c as char)
    return 0
```

## Target Differences

| Target | Backend |
|--------|---------|
| x86_64 | Linux syscalls (write=1, read=0, exit=60) |
| x86_32 | Linux syscalls (write=4, read=3, exit=1) |
| x86_16 | BIOS INT 10h/16h, no `rand`/`exit` |
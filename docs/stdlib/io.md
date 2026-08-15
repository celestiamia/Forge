# std.io

Input/output functions and direct Linux syscall wrappers.

## Console Functions

### puts

```dev
def puts(s: ptr[char]) -> int32
```

Write a null-terminated string to stdout. Returns the write status.

```dev
from std.io import puts

puts("Hello, World!\n")
```

### putchar

```dev
def putchar(c: int32) -> int32
```

Write a single character to stdout. Takes the character as `int32`:

```dev
from std.io import putchar

putchar('A' as int32)
putchar(10)   # newline
```

### getchar

```dev
def getchar() -> int32
```

Read a single character from stdin. Returns `-1` on EOF.

```dev
from std.io import getchar, putchar

var c = getchar()
while c != -1:
    putchar(c)
    c = getchar()
```

### rand

```dev
def rand() -> int32
```

Generate a pseudo-random 31-bit integer (0 to 2^31-1). Linear congruential
generator — not cryptographic.

```dev
from std.io import rand

let r = rand()              # 0 to 2147483647
let dice = rand() % 6 + 1   # 1 to 6
```

### exit

```dev
def exit(code: int32) -> void
```

Terminate the process with an exit code. Does not return.

```dev
from std.io import exit

exit(0)   # Success
exit(1)   # Error
```

## Syscall Wrappers

Direct Linux syscall wrappers (x86_64 and x86_32). Returns are `-1` on error
with `errno` in the low bits, matching raw syscall semantics.

| Function | Syscall | Notes |
|----------|---------|-------|
| `open(path, flags, mode) -> int32` | open | Returns fd |
| `read(fd, buf, count) -> int32` | read | |
| `write(fd, buf, count) -> int32` | write | |
| `close(fd) -> int32` | close | |
| `lseek(fd, offset, whence) -> int64` | lseek | |
| `unlink(path) -> int32` | unlink | |
| `fork() -> int32` | fork | |
| `waitpid(pid, status, options) -> int32` | waitpid | |
| `gettimeofday(tv, tz) -> int32` | gettimeofday | `tv`/`tz` are `ptr[char]` buffers |
| `socket(domain, type, protocol) -> int32` | socket | |
| `bind(fd, addr, addrlen) -> int32` | bind | |
| `listen(fd, backlog) -> int32` | listen | |
| `accept(fd, addr, addrlen) -> int32` | accept | `addrlen` is `ptr[int32]` |
| `setsockopt(fd, level, optname, optval, optlen) -> int32` | setsockopt | |
| `fcntl(fd, cmd, arg) -> int64` | fcntl | |

Example — read a file:

```dev
from std.io import open, read, close, puts

pub def main() -> int32:
    let fd = open("/tmp/hello.txt", 0, 0)   # O_RDONLY
    if fd < 0:
        return 1
    var buf: ptr[char] = "                       "
    let n = read(fd, buf, 20)
    close(fd)
    if n > 0:
        puts(buf)
    return 0
```

## Implementation Notes

- **x86_64/x86_32**: direct Linux syscalls (`write` = 1/4, `read` = 0/3,
  `exit` = 60/1)
- **x86_16**: BIOS interrupts (teletype) via compiler-emitted helpers;
  syscalls are unavailable
- No buffering — each call is a syscall
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
        putchar(c)
    return 0
```
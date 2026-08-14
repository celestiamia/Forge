# std.string

String manipulation functions for null-terminated C-style strings.

## Functions

### strlen

```dev
def strlen(s: ptr[char]) -> usize
```

Return length of null-terminated string (excluding null terminator).

```dev
from std.string import strlen

let s = "Hello"
let len = strlen(s)  # 5
```

### strcmp

```dev
def strcmp(a: ptr[char], b: ptr[char]) -> int32
```

Lexicographically compare two strings. Returns:
- `< 0` if `a < b`
- `0` if equal
- `> 0` if `a > b`

```dev
from std.string import strcmp

let a = "apple"
let b = "banana"
let cmp = strcmp(a, b)
# cmp < 0 ("apple" < "banana")
```

### strncmp

```dev
def strncmp(a: ptr[char], b: ptr[char], n: usize) -> int32
```

Compare at most `n` characters. Returns same as `strcmp`.

```dev
from std.string import strncmp

let a = "hello world"
let b = "hello there"
strncmp(a, b, 5)  # 0 (equal for first 5 chars)
strncmp(a, b, 6)  # != 0 (' ' vs ' ')
```

## Implementation Notes

- All functions stop at first null terminator
- No bounds checking beyond null terminator
- `strncmp` stops at `n` chars or null, whichever comes first
- Case-sensitive (ASCII ordering)

## Example: String Parsing

```dev
from std.string import strlen, strcmp
from std.io import puts

def starts_with(s: ptr[char], prefix: ptr[char]) -> bool:
    let len = strlen(prefix)
    return strncmp(s, prefix, len) == 0

pub def main() -> int32:
    let cmd = "help me"
    if starts_with(cmd, "help"):
        puts("Help requested\n")
    return 0
```

## Target Support

All targets: pure software implementation, no syscalls needed.

## Safety

- Requires valid null-terminated strings
- No bounds checking - caller must ensure valid pointers
- `strncmp` with large `n` may read past buffer if no null terminator

## Missing Functions (Planned)

- `strcpy` / `strncpy`
- `strcat` / `strncat`
- `strchr` / `strrchr`
- `strstr`
- `atoi` / `atof`
- `itoa` (see `std.fmt.format_i32`)
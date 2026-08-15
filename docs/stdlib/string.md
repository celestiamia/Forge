# std.string

String manipulation for null-terminated C-style strings.

## Functions

### strlen

```dev
def strlen(s: ptr[char]) -> uint64
```

Return the length of a null-terminated string (excluding the null terminator).

```dev
from std.string import strlen

let len = strlen("Hello")   # 5
```

### strcmp

```dev
def strcmp(a: ptr[char], b: ptr[char]) -> int32
```

Lexicographically compare two strings. Returns `< 0`, `0`, or `> 0`.

```dev
from std.string import strcmp

if strcmp(a, b) == 0:
    puts("equal")
```

### strncmp

```dev
def strncmp(a: ptr[char], b: ptr[char], n: uint64) -> int32
```

Compare at most `n` characters. Same return convention as `strcmp`.

```dev
strncmp("hello world", "hello there", 5)   # 0
```

### strstr

```dev
def strstr(haystack: ptr[char], needle: ptr[char]) -> ptr[char]
```

Find the first occurrence of `needle` in `haystack`. Returns a pointer to the
match, or null if not found.

```dev
let pos = strstr("needle in haystack", "needle")
# pos points at the start of "needle"
```

### strchr

```dev
def strchr(s: ptr[char], c: int32) -> ptr[char]
```

Find the first occurrence of character `c` in `s`. Returns a pointer to the
match, or null if not found. The character is passed as `int32`:

```dev
let ch = strchr("abc", 'b' as int32)
```

### strcat

```dev
def strcat(dest: ptr[char], src: ptr[char]) -> ptr[char]
```

Append `src` to the end of `dest` (which must have room). Returns `dest`.

### strncpy

```dev
def strncpy(dest: ptr[char], src: ptr[char], n: uint64) -> ptr[char]
```

Copy at most `n` characters from `src` to `dest`. Returns `dest`.

## Implementation Notes

- All functions stop at the first null terminator
- No bounds checking beyond the null terminator — the caller must ensure
  destination buffers have room (especially for `strcat`/`strncpy`)
- Case-sensitive (ASCII ordering)

## Example: Prefix Check

```dev
from std.string import strlen, strncmp
from std.io import puts

def starts_with(s: ptr[char], prefix: ptr[char]) -> bool:
    let len = strlen(prefix)
    return strncmp(s, prefix, len) == 0

pub def main() -> int32:
    if starts_with("help me", "help"):
        puts("Help requested\n")
    return 0
```

## Target Support

All targets: pure Forge implementation, no syscalls needed.
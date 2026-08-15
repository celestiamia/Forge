# Control Flow

Forge provides standard control flow constructs with Python-like syntax.

## If / Elif / Else

```dev
if condition:
    # block
elif other_condition:
    # block
else:
    # block
```

- Conditions must be `bool`
- No parentheses around the condition
- Indentation defines blocks (4 spaces)
- `elif` and `else` are optional
- `if` is a statement, not an expression (no `let x = if ...`)

```dev
let x = 10
if x > 0:
    puts("positive")
elif x < 0:
    puts("negative")
else:
    puts("zero")
```

## While Loop

```dev
while condition:
    # body
```

- Condition is checked before each iteration
- Must be `bool`
- `break` and `continue` work inside

```dev
var i = 0
while i < 10:
    if i == 5:
        break
    puts("iteration")
    i = i + 1
```

There is no `while ... else`.

## For Loop

```dev
for i in 0..10:     # start..end: exclusive end (0..9)
    puts(i)

for i in 0..=10:    # inclusive end (0..10)
    puts(i)
```

The range endpoints are integer expressions. Iterating over arrays
(`for x in arr`) is not supported yet — index with a range instead.

## Loop / Break / Continue

```dev
loop:
    # infinite loop
    if condition:
        break
    if skip:
        continue
```

- `break` exits the innermost loop
- `continue` skips to the next iteration
- Works in `while`, `for`, and `loop`

```dev
var i = 0
loop:
    i = i + 1
    if i == 3:
        continue
    if i > 5:
        break
    puts("iteration")
```

## Match Expression

```dev
match expression:
    case 0: puts("zero")
    case 1: puts("one")
    case _: puts("other")
```

- Cases match integer or char literal values
- `_` is the wildcard (catch-all)
- Cases are evaluated top-to-bottom; the first match wins
- There is no fallthrough

```dev
match c:
    case 'a': puts("letter a")
    case 'b': puts("letter b")
    case _: puts("other")
```

Patterns beyond literals and `_` (variable bindings, guards, struct/enum
patterns) are not supported yet.

## Return

```dev
return value    # return with value
return          # void return (or end of function)
```

- Exits the current function immediately
- In `loop`/`while`/`for`, returns from the enclosing function
- Void functions may omit `return`

## Unsafe Blocks

```dev
unsafe:
    *ptr = 42                # Raw pointer write
    let val = *ptr           # Raw pointer read
    p = p + 1                # Pointer arithmetic
```

Unsafe is required for:
- Dereferencing raw pointers (`*p`)
- Pointer arithmetic (`p + n`, `p - q`)

Keep unsafe blocks minimal and document why they are safe.

## Panic / Abort

```dev
from std.runtime import abort
from std.io import exit

abort()   # Immediate termination (SIGABRT)
exit(1)   # Clean exit with code
```

There is no unwinding — `abort` terminates immediately.
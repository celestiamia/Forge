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

- Conditions must be `bool` type
- No parenthesis around condition
- Indentation defines blocks
- `elif` and `else` are optional
- Each block must be indented 4 spaces

```dev
let x = 10
if x > 0:
    puts("positive")
elif x < 0:
    puts("negative")
else:
    puts("zero")
```

### If as Expression

```dev
let result = if x > 0: "positive" else: "non-positive"
```

Both branches must have compatible types.

## While Loop

```dev
while condition:
    # body
```

- Condition checked before each iteration
- Must be `bool` type
- Can use `break` and `continue`

```dev
var i = 0
while i < 10:
    if i == 5:
        break
    puts(i)
    i = i + 1
```

### While with Else

```dev
while condition:
    # body
else:
    # executes when condition becomes false (not on break)
```

## For Loop

```dev
for variable in range:
    # body
```

Range syntax: `start..end` (inclusive start, exclusive end)

```dev
for i in 0..10:
    puts(i)  # 0, 1, ..., 9
```

### Iterating Arrays/Slices

```dev
let arr = [1, 2, 3, 4, 5]
for x in arr:
    puts(x)
```

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
- `continue` skips to next iteration
- Works in `while`, `for`, and `loop`

```dev
var i = 0
loop:
    i = i + 1
    if i == 3:
        continue
    if i > 5:
        break
    puts(i)
# Output: 1, 2, 4, 5
```

## Match Expression

```dev
match expression:
    case pattern1: result1
    case pattern2: result2
    case _: default_result
```

### Patterns

| Pattern | Matches |
|---------|---------|
| `42` | Exact value |
| `x` | Bind variable (irrefutable) |
| `Some(x)` | Enum variant with binding |
| `Point { x: 0, y: _ }` | Struct with field patterns |
| `_` | Wildcard (catch-all) |

```dev
match x:
    case 0: puts("zero")
    case 1: puts("one")
    case n if n > 0: puts("positive")
    case _: puts("other")
```

### Match on Enums

```dev
enum Option<T>:
    Some(T)
    None

match opt:
    case Some(v): puts("value: " + v)
    case None: puts("none")
```

### Match on Structs

```dev
struct Point:
    x: int32
    y: int32

match p:
    case Point { x: 0, y: 0 }: puts("origin")
    case Point { x: x, y: 0 }: puts("on x-axis")
    case Point { x: _, y: y }: puts("other")
```

### Match Guards

```dev
match x:
    case n if n > 0: puts("positive")
    case n if n < 0: puts("negative")
    case _: puts("zero")
```

## Return

```dev
return value    # return with value
return          # void return (or end of function)
```

- Exits current function immediately
- In `loop`/`while`/`for`, returns from enclosing function
- Void functions can omit return

## Unsafe Blocks

```dev
unsafe:
    # dereference raw pointers
    # call extern functions without checking
    # access union fields
```

```dev
let ptr = 0x1000 as ptr[int32]
unsafe:
    let val = *ptr  # dereference
```

### When Unsafe is Required

- Dereferencing raw pointers (`ptr[T]`)
- Calling `extern` functions without wrapper
- Accessing union fields
- Inline assembly
- Transmuting types

### Unsafe Guidelines

- Keep unsafe blocks minimal
- Document why each unsafe block is safe
- Prefer safe abstractions over raw unsafe

## Break / Continue Labels (Not Yet Supported)

Currently only innermost loop can be targeted. Named labels planned.

## Tail Calls

Not currently optimized. Tail recursion may cause stack overflow.

## Short-Circuit Evaluation

- `&&` and `\|\|` short-circuit
- Function arguments evaluated left-to-right
- Match cases evaluated top-to-bottom

## Panic / Abort

```dev
std.runtime.abort()  # Immediate termination
std.runtime.exit(1)  # Clean exit with code
```

No unwinding - `abort` terminates immediately.
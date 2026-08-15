# Multi-Module Example

Demonstrates Forge's module system with multiple source files.

## Project Structure

```
examples/multimod/
├── multimod.dev      # Entry file
└── utils.dev         # Imported module
```

## Source Files

### `utils.dev` - Utility Module

```dev
package utils

from std.io import puts

pub def is_even(n: int32) -> bool:
    return n % 2 == 0

pub def clamp(v: int32, lo: int32, hi: int32) -> int32:
    if v < lo:
        return lo
    if v > hi:
        return hi
    return v
```

### `multimod.dev` - Entry File

```dev
package multimod

import utils
from utils import is_even

pub def main() -> int32:
    if is_even(42) && clamp(100, 0, 50) == 50:
        puts("multimod ok\n")
        return 0
    return 1
```

## Compile & Run

```bash
# Compile the entry file (pulls in utils automatically)
forgec examples/multimod/multimod.dev -o multimod --target x86_64-unknown-linux-gnu

# Run
./multimod
```

Output:

```
multimod ok
```

## Module Resolution

```
examples/multimod/
├── multimod.dev    # Entry point
└── utils.dev       # Found in the same directory
```

Imports resolve relative to the entry file's directory, walking up the tree.
Here `import utils` finds `./utils.dev` immediately.

## Import Styles

Both forms are equivalent — everything merges into one flat namespace:

```dev
import utils            # Import everything from utils.dev
from utils import clamp # Import one name
```

Because the namespace is flat, imported names are used **directly** — there
is no `utils.clamp(...)` qualified call syntax, and
`from utils import clamp as limit` (renaming items) is not supported.

## Visibility

```dev
pub def is_even(n: int32) -> bool:   # Public
    ...

def private_helper() -> int32:       # No pub
    ...
```

The loader merges **all** items from imported modules — `pub` and private
alike — so private items are usable from the entry file too. Visibility is
currently informational only; it is not enforced at import time.

## Limitations

1. **Flat namespace** — all imports merge into a single namespace
2. **Name conflicts** — an error if two modules define the same name
3. **No re-exports** — no `pub use`
4. **No item renaming** — `from m import x as y` is not supported
5. **No circular imports** — compiler reports cycles
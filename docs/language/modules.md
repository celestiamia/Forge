# Modules & Imports

Forge uses a file-based module system with `import` and `from ... import`
syntax. All imported modules are merged into a **single flat namespace**
before type checking.

## Import Syntax

```dev
import myutils              # Import all items from myutils.dev
from myutils import helper  # Import only `helper`
```

```dev
# Standard library
from std.io import puts
import std.io               # Also accepted
```

The `import std.io as io` and `from std.io import *` forms are accepted by the
loader.

> Because the namespace is flat, there is **no qualified access** — you cannot
> write `myutils.helper()` or `std.io.puts()`. Imported names are used
> directly, and name conflicts between modules are reported as errors.

## Resolution Algorithm

Modules resolve relative to the **entry file's directory**, walking up:

```
project/
├── main.dev          # Entry file
├── utils.dev         # Found: ./utils.dev
├── lib/
│   └── math.dev      # import lib.math → ./lib/math.dev
└── sub/
    └── main.dev      # Entry in a subdirectory
```

When compiling `project/sub/main.dev`, `import utils` checks `sub/utils.dev`,
then `../utils.dev`, then `../../utils.dev`, and so on up the tree.

`std.*` imports resolve to `core/<name>.dev` using the same walk-up strategy.
The compiler finds the `core/` directory above the source file.

If no on-disk `core/` directory is found anywhere up the tree, `std.*`
imports fall back to the standard library **embedded in the `forgec`
binary** (`src/embed.rs`). The packaged compiler is fully self-contained —
no `core/` checkout is needed — and an on-disk `core/` always takes
precedence when present.

## Standard Library

Standard library modules live in `core/` and are imported as `std.*`:

```dev
from std.io import puts
from std.mem import copy_bytes
from std.string import strlen
```

Mapping: `std.io` → `core/io.dev`, `std.string` → `core/string.dev`.

## Example Layout

```
examples/multimod/
├── multimod.dev          # entry file — imports utils.dev
└── utils.dev             # user module — imported by multimod.dev
```

`examples/multimod/utils.dev`:

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

`examples/multimod/multimod.dev`:

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

## Package Declaration

`package <name>` at the top of a file is optional and currently informational
only — the flat namespace means the package name is not used for qualified
access.

## Visibility

`pub` marks an item as public. The loader merges **all** items from imported
modules (public and private alike), so visibility is currently informational
and is not enforced at import time.

## Limitations

- **Flat namespace**: all items from every imported module merge into one
  namespace; conflicts are errors
- **Single entry point**: `forgec` takes one `.dev` file; everything else is
  pulled in via imports
- **No re-exports**: there is no `pub use`
- **No import renaming**: `import foo as bar` is parsed but the alias has no
  effect in the flat namespace
- **Circular imports**: reported as errors
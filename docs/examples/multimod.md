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

def private_helper() -> int32:
    return 42
```

### `multimod.dev` - Entry File

```dev
package multimod

import utils
from utils import is_even

pub def main() -> int32:
    if is_even(42) && utils.clamp(100, 0, 50) == 50:
        puts("multimod ok\n")
        return 0
    return 1
```

## Compile & Run

```bash
# Compile entry file (pulls in utils automatically)
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
└── utils.dev       # Found in same directory
```

Forge resolves imports by walking up from entry file's directory:
1. `./utils.dev` ✓
2. `../utils.dev` (not checked, found at 1)

## Import Styles

### Import All Public

```dev
import utils
utils.is_even(42)
utils.clamp(100, 0, 50)
```

### Import Specific Items

```dev
from utils import is_even
is_even(42)  # Direct access
```

### Rename on Import

```dev
from utils import clamp as limit
limit(100, 0, 50)
```

## Visibility

### In `utils.dev`

```dev
pub def is_even(n: int32) -> bool:    # Exported
    ...

pub def clamp(v: int32, lo: int32, hi: int32) -> int32:  # Exported
    ...

def private_helper() -> int32:        # Module-private
    ...
```

- `pub` = visible to importers
- No `pub` = module-private

## Multiple Modules

```
project/
├── main.dev
├── config.dev
├── utils/
│   ├── io.dev
│   └── math.dev
└── features/
    └── auth.dev
```

```dev
# main.dev
package myapp

import config
from utils.io import read_file
from utils.math import clamp
from features.auth import login
```

## Package Declaration

```dev
# utils.dev
package utils

# main.dev
package myapp
from utils import foo  # Resolves to utils.foo
```

- Optional at top of file
- Defaults to filename (without `.dev`)
- Used for namespace qualification

## Limitations

1. **Flat namespace** - All imports merge into single namespace
2. **Name conflicts** - Error if two modules export same name
3. **No re-export** - `pub use` not fully implemented
4. **No circular imports** - Compiler error on cycles

## Best Practices

1. **Use `from ... import`** for clarity
2. **Group imports**: stdlib first, then local
3. **Keep modules small** - Single responsibility
4. **Use packages** for organization
5. **Mark internal functions private** (no `pub`)

## Example: Larger Project

```
myapp/
├── main.dev              # Entry
├── config.dev            # Configuration
├── core/
│   ├── types.dev         # Core types
│   └── errors.dev        # Error handling
├── modules/
│   ├── database.dev      # DB operations
│   └── network.dev       # Networking
└── cli/
    └── commands.dev      # CLI parsing
```

```dev
# main.dev
package myapp

import config
from core.types import User, Config
from core.errors import Result
from modules.database import connect, query
from modules.network import serve
from cli.commands import parse_args

pub def main() -> int32:
    let args = parse_args()
    let cfg = config.load()
    match connect(cfg.db_url):
        case Ok(conn):
            serve(conn, args)
        case Err(e):
            puts("DB error: ")
            puts(e.message)
            return 1
    return 0
```
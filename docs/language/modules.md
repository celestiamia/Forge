# Modules & Imports

Forge uses a file-based module system with `import` and `from ... import` syntax.

## Module Structure

Each `.dev` file is a module. The package name is declared at the top:

```dev
package mylib

pub def foo() -> int32:
    return 42
```

## Import Syntax

### Import All Public Items

```dev
import mymodule
# Access as: mymodule.foo()
```

### Import Specific Items

```dev
from mymodule import foo, bar
# Access as: foo(), bar()
```

### Rename on Import

```dev
from mymodule import foo as my_foo
```

### Package-Qualified Imports

```dev
from mypkg.sub import helper
```

## Resolution Algorithm

Modules are resolved relative to the **entry file's directory**, walking up:

```
project/
├── main.dev          # Entry file
├── utils.dev         # Found: ./utils.dev
├── lib/
│   └── math.dev      # Found: ./lib/math.dev
└── sub/
    └── main.dev      # Entry in subdirectory
```

When compiling `project/sub/main.dev`:
1. Check `./sub/utils.dev`
2. Check `../utils.dev`
3. Check `../../utils.dev`
... up to filesystem root

## Standard Library

Standard library modules live in `core/` and are imported as `std.*`:

```dev
from std.io import puts
from std.mem import copy_bytes
from std.string import strlen
```

Mapping: `std.io` → `core/io.dev`, `std.string` → `core/string.dev`

## Visibility

```dev
# In module file:
pub def public_fn():       # Exported
    ...

def private_fn():          # Module-only
    ...

pub struct PublicStruct:   # Exported
    pub x: int32           # Field visibility follows struct

struct PrivateStruct:      # Not exported
    ...
```

## Re-exports

```dev
# In mylib.dev
from submodule import foo
pub use foo  # Re-export as mylib.foo
```

## Circular Imports

Not allowed. Compiler reports cycle error.

```dev
# a.dev
import b

# b.dev
import a  # Error: circular import
```

## Package Declaration

Optional at top of file:

```dev
package mypackage

# All items in this file belong to `mypackage` namespace
# Imported as: from mypackage import foo
```

If omitted, module name = filename (without `.dev`).

## Import Aliases

```dev
import verylongmodulename as vlm
vlm.foo()
```

## Conditional Imports

Not supported. Use build flags or separate entry files.

## Module Resolution Order

1. Standard library (`std.*` → `core/*.dev`)
2. Current directory
3. Parent directories (walking up)
4. Error if not found

## Examples

### Simple Import

```dev
# utils.dev
pub def helper() -> int32:
    return 42

# main.dev
import utils

pub def main() -> int32:
    return utils.helper()
```

### From Import

```dev
# math.dev
pub def add(a: int32, b: int32) -> int32:
    return a + b

# main.dev
from math import add

pub def main() -> int32:
    return add(1, 2)
```

### Nested Modules

```
project/
├── main.dev
├── lib/
│   ├── __init__.dev   # Package marker
│   └── math.dev
```

```dev
# lib/math.dev
pub def add(a: int32, b: int32) -> int32:
    return a + b

# main.dev
from lib.math import add
```

### Multi-File Project

```
myapp/
├── main.dev           # Entry
├── config.dev         # Imported
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

pub def main() -> int32:
    ...
```

## Limitations

- **Flat namespace**: All imports merge into single namespace
- **No re-export control**: `pub use` not fully implemented
- **No versioning**: No semver for modules
- **No private modules**: All `.dev` files are importable

## Best Practices

1. **Use `from ... import`** for clarity
2. **Group imports**: stdlib first, then local
3. **Avoid `import *`**: Be explicit
4. **Keep modules small**: Single responsibility
5. **Use packages** for organization

## Future Plans

- Private modules (`mod` keyword)
- Re-export control (`pub use`)
- Module aliases (`import foo as bar`)
- Conditional compilation (`#[cfg]`)
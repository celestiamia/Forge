# Diagnostics

Reference for compiler error codes, warnings, and diagnostic format.

## Diagnostic Format

```
error[E0001]: human-readable message
 --> file.dev:line:col
  |
N | source code
  |     ^^^^
  |
  = note: additional context
  = help: suggested fix
```

## Error Codes

### Lexer Errors (E0000-E0099)

| Code | Message | Cause |
|------|---------|-------|
| E0001 | Invalid character | Unrecognized character in source |
| E0002 | Unterminated string | Missing closing quote |
| E0003 | Invalid escape | Unknown escape sequence in string |
| E0004 | Invalid number | Malformed numeric literal |
| E0005 | Indentation error | Not multiple of 4 spaces |
| E0006 | Unexpected EOF | File ended unexpectedly |
| E0007 | Invalid indent/dedent | Mismatched indentation |

### Parser Errors (E0100-E0199)

| Code | Message | Cause |
|------|---------|-------|
| E0100 | Expected token | Unexpected token in grammar |
| E0101 | Unexpected EOF | Incomplete construct |
| E0102 | Invalid type expression | Malformed type annotation |
| E0103 | Expected identifier | Keyword where identifier expected |
| E0104 | Duplicate definition | Same name defined twice |
| E0105 | Invalid pattern | Malformed match pattern |
| E0106 | Expected block | Missing indented block |
| E0107 | Invalid attribute | Unknown or malformed `@attr` |

### Semantic Errors (E0200-E0399)

#### Name Resolution (E0200-E0249)

| Code | Message | Cause |
|------|---------|-------|
| E0200 | Unresolved name | Identifier not in scope |
| E0201 | Unresolved import | Module not found |
| E0202 | Ambiguous name | Multiple matching definitions |
| E0203 | Private access | Accessing non-pub item |
| E0204 | Cyclic import | Import cycle detected |

#### Type Checking (E0250-E0349)

| Code | Message | Cause |
|------|---------|-------|
| E0300 | Type mismatch | Incompatible types in expression |
| E0301 | Expected type | Expression type doesn't match expected |
| E0302 | Integer overflow | Literal too large for type |
| E0303 | Division by zero | Constant division by zero |
| E0304 | Invalid cast | Incompatible types in `as` |
| E0305 | Missing return | Non-void function missing return |
| E0306 | Type not found | Type name not defined |
| E0307 | Generic arity | Wrong number of type args |
| E0308 | Incompatible types | Assignment/function arg mismatch |

#### Borrow Checker (E0350-E0399)

| Code | Message | Cause |
|------|---------|-------|
| E0350 | Use of moved value | Value used after move |
| E0351 | Borrow conflict | Mutable + immutable borrow |
| E0352 | Use after free | Reference outlives referent |
| E0353 | Mutable borrow | Multiple mutable borrows |

### Codegen Errors (E0400-E0499)

| Code | Message | Cause |
|------|---------|-------|
| E0400 | Unsupported target | Target not implemented |
| E0401 | Register allocation failed | Too many live variables |
| E0402 | Unsupported feature | Feature not implemented for target |
| E0403 | Stack overflow | Frame too large |
| E0404 | Relocation overflow | Jump target out of range |

### Linker Errors (E0500-E0599)

| Code | Message | Cause |
|------|---------|-------|
| E0500 | Undefined symbol | Symbol not defined |
| E0501 | Duplicate symbol | Multiple definitions |
| E0502 | Invalid linker script | Malformed .fld file |
| E0503 | Memory overflow | Section exceeds region |

## Warnings (W0000+)

| Code | Message | Cause |
|------|---------|-------|
| W0001 | Unused variable | Variable declared but not used |
| W0002 | Unused import | Import not referenced |
| W0003 | Dead code | Code after return/break/continue |
| W0004 | Deprecated syntax | Feature will be removed |
| W0005 | Unused function | Function never called |

## Diagnostic Levels

| Level | Prefix | Behavior |
|------|--------|----------|
| Error | `error` | Compilation fails |
| Warning | `warning` | Compilation succeeds |
| Note | `note` | Additional context |
| Help | `help` | Suggested fix |

## Suppressing Diagnostics

### Attributes (Future)

```dev
#[allow(unused_variables)]
def foo():
    let x = 1  # No warning
```

### Command Line

```bash
# Not yet implemented
forgec --allow warnings
```

## IDE Integration

### rust-analyzer (for Rust code)

```json
// .vscode/settings.json
{
    "rust-analyzer.checkOnSave": true,
    "rust-analyzer.cargo.target": "x86_64-unknown-linux-gnu"
}
```

### Language Server (Forge)

Not yet implemented. Planned LSP support.

## Debugging Diagnostics

### Verbose Output

```bash
RUST_BACKTRACE=1 forgec input.dev
```

### Internal Debug

```bash
# Set log level
RUST_LOG=debug forgec input.dev

# Specific modules
RUST_LOG=forgec::sema=debug,forgec::codegen=trace forgec input.dev
```

### JSON Output (Planned)

```bash
forgec --json input.dev
# {"level":"error","code":"E0308","message":"...","spans":[...]}
```

## Common Error Patterns

### Type Mismatch

```dev
let x: int32 = "hello"  // E0308
```

Fix: Use correct type or cast.

### Missing Return

```dev
def foo() -> int32:
    if true:
        return 1
// E0305: missing return
```

Fix: Add return on all paths.

### Unresolved Import

```dev
from std.nonexistent import foo  // E0201
```

Fix: Check module name, ensure `core/nonexistent.dev` exists.

### Borrow Error

```dev
let mut x = 42
let r = &x
let mr = &mut x  // E0351
```

Fix: Restructure to avoid simultaneous borrows.

## Diagnostic Customization

Not yet implemented. Planned:

- Custom error codes for project
- Diagnostic severity configuration
- Output format (JSON, SARIF, etc.)
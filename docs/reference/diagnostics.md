# Diagnostics

How `forgec` reports errors.

## Diagnostic Format

Errors are plain-text messages with file, line, and column. There are no
error codes and no source-snippet carets yet.

Parse errors:

```text
Error: parse error in bad.dev: 7:10: expected identifier, found LParen
```

Type-checking errors group all findings together:

```text
type checking failed:
  bad.dev: `let mr` expected `&mut i32`, found `&i32`
  bad.dev: unknown identifier `own`
```

Codegen and lowering errors:

```text
Error: inline assembly is not implemented in the x64 backend
```

## Error Classes

| Class | When it happens | Example message |
|-------|-----------------|-----------------|
| Parse errors | Lexing/parsing | `expected expression, found Def` |
| Type checking | Semantic analysis | `unknown identifier \`Red\``, `expected \`i32\`, found \`ptr[char]\`` |
| Lowering | AST → IR | `tuples are not supported in the first milestone` |
| Codegen | Machine code emission | `inline assembly is not implemented in the x64 backend` |
| Runtime helpers | Missing runtime symbol | name-conflict or missing-symbol errors |

## Compiler Panics

Some constructs crash the compiler instead of producing a clean error (see
[Known Issues](../language/known-issues.md)):

- Generic functions (`def identity[T](...)`)
- `impl` blocks
- Nested struct fields

When a panic occurs, run with `RUST_BACKTRACE=1` to get a backtrace, and
report it as a bug with a minimal reproduction:

```bash
RUST_BACKTRACE=1 forgec repro.dev
```

## Debugging

Forge emits real machine code, so standard tools work on the output:

```bash
gdb ./output
objdump -d ./output | less
strace ./output
```

For the boot sector target, use QEMU:

```bash
qemu-system-x86_64 -fda boot.bin -nographic -s -S   # wait for gdb
gdb -ex "target remote localhost:1234" -ex "set architecture i8086"
```
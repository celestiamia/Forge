# Development Workflow

Guide for contributing to the Forge compiler.

## Getting Started

### 1. Fork & Clone

```bash
git clone https://github.com/your-username/Forge.git
cd Forge
```

### 2. Build

```bash
cargo build --release
```

### 3. Test

```bash
cargo test
```

### 4. Make Changes

```bash
git checkout -b my-feature
# Edit files...
cargo test
git commit -m "feat: description"
git push origin my-feature
```

## Code Style

### Rust Code

```bash
# Format
cargo fmt --all

# Lint
cargo clippy --all-targets -- -D warnings
```

Guidelines:
- Follow standard Rust conventions
- Run `cargo fmt` before committing
- Fix all clippy warnings
- Keep functions small (< 50 lines)
- Document public APIs with `///`

### Forge Code (`.dev` files)

- 4-space indentation (no tabs)
- `snake_case` for variables/functions
- `PascalCase` for types
- `SCREAMING_SNAKE` for constants

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): brief description

Longer explanation if needed.

Fixes #123
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Code restructuring |
| `perf` | Performance improvement |
| `test` | Adding tests |
| `docs` | Documentation |
| `chore` | Maintenance |
| `build` | Build system |
| `ci` | CI configuration |

### Examples

```
feat(codegen): add float64 support for x86_64

Implements float64 codegen using SSE2 registers.
Updates stdlib math functions.

Fixes #42
```

```
fix(parser): handle nested match expressions

The parser incorrectly matched nested match arms.
Added proper nesting depth tracking.

Fixes #15
```

## Branching Strategy

- `main` - Stable releases
- Feature branches: `feat/description`
- Fix branches: `fix/description`
- Release branches: `release/vX.Y.Z`

## Pull Request Process

1. **Open PR** against `main`
2. **Description**: What, why, how
3. **Tests**: All passing (`cargo test`)
4. **Review**: Address feedback
5. **Merge**: Squash and merge

### PR Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] New tests added for new functionality
- [ ] Documentation updated (README, docs/)
- [ ] Commit messages follow convention
- [ ] No unrelated changes

## Code Review Guidelines

### For Reviewers

- Be constructive and specific
- Focus on correctness, then style
- Ask questions instead of demands
- Approve when confident

### For Authors

- Respond to all comments
- Explain reasoning for decisions
- Update PR based on feedback
- Keep PR focused (one feature/fix)

## Adding a New Feature

### 1. Syntax (Frontend)

```bash
# 1. AST: src/sema/ast.rs or src/ast/mod.rs
# 2. Parser: src/parser/expr.rs or src/parser/stmt.rs
# 3. Lexer: src/lexer/lexer.rs (if new tokens)
```

### 2. Type Checking (Semantic Analysis)

```bash
# 1. Type: src/ty/mod.rs
# 2. Check: src/sema/check/expr.rs or src/sema/check/typing.rs
```

### 3. Lowering (AST → IR)

```bash
# src/lower/expr.rs or src/lower/stmt.rs
```

### 4. Codegen (Backend)

```bash
# x86_64: src/backend/codegen/expr.rs
# x86_32: src/backend/codegen32/expr.rs
# x86_16: src/backend/codegen16/expr.rs
```

### 5. Stdlib (if needed)

```bash
# core/<module>.dev
```

### 6. Tests

```bash
# Integration: tests/integration.rs
# Unit: src/*/tests.rs
```

### 7. Documentation

```bash
# docs/language/*.md
# docs/stdlib/*.md
# README.md (if user-facing)
```

## Debugging

### Compiler Debugging

```bash
# Build with debug info
cargo build

# Run with backtrace
RUST_BACKTRACE=1 ./target/debug/forgec input.dev

# GDB
gdb ./target/debug/forgec
```

### Generated Code Debugging

```bash
# Compile with debug info
forgec input.dev -o output --target x86_64-unknown-linux-gnu

# GDB
gdb ./output

# objdump
objdump -d output | less

# strace
strace ./output
```

### QEMU Debugging (Boot Sector)

```bash
qemu-system-x86_64 -fda boot.bin -nographic -s -S
# gdb -ex "target remote localhost:1234" -ex "set architecture i8086"
```

## Performance Profiling

```bash
# Build with profiling
cargo build --release --features profiling

# perf
perf record ./target/release/forgec input.dev
perf report

# Callgrind
valgrind --tool=callgrind ./target/release/forgec input.dev
kcachegrind callgrind.out.*
```

## Common Tasks

### Add New Keyword

1. `src/lexer/lexer.rs` - Add token
2. `src/parser/parser.rs` - Parse keyword
3. `src/sema/check/*.rs` - Type check
4. `src/backend/codegen/*.rs` - Codegen

### Add New Type

1. `src/ty/mod.rs` - Type definition
2. `src/sema/check/typing.rs` - Type checking
3. `src/backend/ir.rs` - IR type
2. `src/backend/codegen/*.rs` - Codegen
3. `src/lower/*.rs` - Lowering

### Add New Target

1. `src/linker/config.rs` - Target config
2. `src/backend/codegen*/` - Codegen backend
3. `src/obj/` - Object writer
4. `src/linker/config.rs` - Builtin config

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust Reference](https://doc.rust-lang.org/reference/)
- [Forge Architecture](architecture.md)
- [Testing Guide](testing.md)
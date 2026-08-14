# Contributing to Forge

Thank you for your interest in contributing to Forge! This document outlines the guidelines for contributing to the compiler and standard library.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/Forge.git`
3. Build the compiler: `cargo build --release`
4. Run the test suite: `cargo test`

## Development Workflow

### Making Changes

1. Create a feature branch: `git checkout -b my-feature`
2. Make your changes following the code style guidelines below
3. Ensure all tests pass: `cargo test`
4. Commit with a clear message (see Commit Messages)
5. Push to your fork and open a Pull Request

### Code Style

- Follow standard Rust conventions (run `cargo fmt` if available)
- Use `cargo clippy` to catch common issues
- Keep functions small and focused
- Document public APIs with rustdoc comments (`///`)
- Match the existing code style in the file you're editing

### Testing

**Required before submitting a PR:**

```bash
# Full test suite
cargo test

# Integration tests (end-to-end compilation + execution)
cargo test --test integration

# Test 32-bit target (if you have i686 toolchain)
cargo test --test integration -- --target x86_32-unknown-linux-gnu

# Test boot sector target (if you have QEMU)
cargo test --test integration -- --target x86_16-boot
```

All tests must pass on the x86_64 target at minimum.

### Commit Messages

Follow conventional commits format:

```
type(scope): brief description

Longer explanation if needed.

Fixes #123
```

Types: `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`, `build`, `ci`

Examples:
- `feat(codegen): add support for float64 on x86_64`
- `fix(parser): handle nested match expressions correctly`
- `docs(readme): add bootloader example to quickstart`

## Pull Request Checklist

- [ ] Code compiles without warnings (`cargo build --release`)
- [ ] All tests pass (`cargo test`)
- [ ] Integration tests pass (`cargo test --test integration`)
- [ ] New functionality has tests (unit or integration)
- [ ] Documentation updated if needed (README, code comments)
- [ ] Commit messages follow conventional format
- [ ] No unrelated changes (whitespace, formatting in untouched files)

## Architecture Overview

See `CLAUDE.md` and `AGENTS.md` for detailed architecture documentation.

Key areas:
- `src/lexer/`, `src/parser/` — Frontend
- `src/sema/`, `src/ty/` — Semantic analysis & type system
- `src/lower/` — AST to IR lowering
- `src/backend/` — Code generation (x86_64, x86_32, x86_16)
- `src/obj/` — ELF/flat binary writers
- `core/` — Standard library modules
- `examples/` — Test programs and examples

## Adding a New Feature

1. **Syntax**: Update `src/ast/mod.rs` and `src/parser/`
2. **Type checking**: Update `src/sema/check/` and `src/ty/mod.rs`
3. **Lowering**: Update `src/lower/` to emit IR
4. **Codegen**: Update `src/backend/codegen/` (and `codegen32/`/`codegen16/` for parity)
5. **Stdlib**: Add to `core/` if needed
6. **Tests**: Add integration test in `tests/integration.rs` and example in `examples/`
7. **Documentation**: Update README if user-facing

## Reporting Issues

Use the issue templates:
- **Bug Report**: For compiler crashes, incorrect codegen, type errors
- **Feature Request**: For new language features, stdlib additions, targets

## Questions?

Open a Discussion or ask in the PR. We're happy to help!
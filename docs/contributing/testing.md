# Testing

Guide for running and writing tests for the Forge compiler.

## Test Suite Overview

```
cargo test                    # All tests
cargo test --lib              # Unit tests only
cargo test --test integration # Integration tests only
cargo test --test integration <name>  # Single test
```

## Test Types

### Unit Tests

Located in `src/*/tests.rs` or `#[cfg(test)]` modules:

```bash
# Run all unit tests
cargo test --lib

# Specific module
cargo test --lib lexer
cargo test --lib parser
cargo test --lib sema
```

### Integration Tests (`tests/integration.rs`)

End-to-end tests that:
1. Compile a `.dev` example
2. Run the resulting binary
3. Check stdout/exit code

```bash
# All integration tests
cargo test --test integration

# Single test
cargo test --test integration hello_dev_compiles_and_runs

# With target
cargo test --test integration hello_dev_compiles_and_runs_x86_32
```

### Test Categories

| Test | Target | Description |
|------|--------|-------------|
| `hello_dev_*` | x86_64, x86_32 | Basic compilation |
| `bootloader_dev_*` | x86_16 | Boot sector |
| `gc_dev_*` | x86_64 | GC functionality |
| `float_*_dev` | x86_64 | Float support |
| `fileio_dev` | x86_64, x86_32 | File I/O |
| `multimod_dev` | x86_64, x86_32 | Multi-module |

## Running Tests

### All Tests

```bash
cargo test
```

### Verbose Output

```bash
cargo test -- --nocapture
cargo test --test integration -- --nocapture
```

### Filter Tests

```bash
# By name pattern
cargo test gc

# Exact match
cargo test gc_dev_compiles_and_runs

# With target filter
cargo test --test integration -- --target x86_32
```

### Parallel Execution

```bash
# Default: parallel
cargo test

# Sequential
cargo test -- --test-threads=1
```

## Writing Tests

### Unit Test Pattern

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // Arrange
        let input = "...";

        // Act
        let result = function(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

### Integration Test Pattern

In `tests/integration.rs`:

```rust
#[test]
fn my_new_feature_compiles_and_runs() {
    let bin = compile_example("my_example");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run binary");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Expected output\n"
    );
}
```

### Adding New Integration Test

1. Add example to `examples/`:

```dev
# examples/my_feature.dev
package my_feature

from std.io import puts

pub def main() -> int32:
    puts("my feature works\n")
    return 0
```

2. Add test case to `tests/integration.rs`:

```rust
#[test]
fn my_feature_dev_compiles_and_runs() {
    let bin = compile_example("my_feature");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run binary");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "my feature works\n"
    );
}
```

3. For x86_32:

```rust
#[test]
fn my_feature_dev_compiles_and_runs_x86_32() {
    let bin = compile_example_with_target("my_feature", "x86_32-unknown-linux-gnu");
    // ... same verification
}
```

## Test Utilities

### `compile_example` (in `tests/integration.rs`)

```rust
fn compile_example(name: &str) -> PathBuf {
    compile_example_with_target(name, "x86_64-unknown-linux-gnu")
}

fn compile_example_with_target(name: &str, target: &str) -> PathBuf {
    let out_dir = temp_dir().join(format!("forge_{}_test", name));
    fs::create_dir_all(&out_dir).unwrap();

    let bin = out_dir.join(name);
    let mut cmd = Command::cargo_bin("forgec").unwrap();
    cmd.arg(format!("examples/{}.dev", name))
        .arg("-o")
        .arg(&bin)
        .arg("--target")
        .arg(target);
    cmd.assert().success();
    bin
}
```

## Test Infrastructure

### Temp Directory

Each test gets isolated temp directory:
```
/tmp/forge_<name>_test_<pid>/<binary>
```

### Cleanup

Automatic on drop (tempfile crate).

### QEMU Tests (Boot Sector)

```rust
#[test]
fn bootloader_dev_compiles_and_runs() {
    let bin = compile_example("bootloader");
    // ... spawn QEMU, capture output, verify "Hello, Forge bootloader"
}
```

Requires `qemu-system-x86_64` installed.

## CI Test Matrix

```yaml
# .github/workflows/ci.yml
strategy:
  matrix:
    include:
      - target: x86_64-unknown-linux-gnu
        rust_target: ""
      - target: x86_32-unknown-linux-gnu
        rust_target: "i686-unknown-linux-gnu"
      - target: x86_16-boot
        rust_target: ""
```

## Debugging Failed Tests

### Run Single Test with Output

```bash
cargo test --test integration hello_dev -- --nocapture
```

### Inspect Generated Binary

```bash
# Compile manually
cargo build --release
./target/release/forgec examples/hello.dev -o hello

# Inspect
objdump -d hello
strings hello
./hello
```

### Debug QEMU

```bash
# With GDB
qemu-system-x86_64 -fda boot.bin -nographic -s -S
# gdb -ex "target remote localhost:1234" -ex "set architecture i8086"
```

## Benchmark Tests

Not yet implemented. Planned:

```rust
#[bench]
fn bench_compile_hello(b: &mut Bencher) {
    b.iter(|| {
        compile_example("hello");
    });
}
```

## Test Coverage

```bash
# Install grcov
cargo install grcov

# Generate coverage
CARGO_INCREMENTAL=0 RUSTFLAGS="-Cinstrument-coverage" \
cargo test --test integration

# Generate report
grcov . -s . --binary-path ./target/debug/deps/ \
    -t html --branch --ignore-not-existing -o coverage/
```

## Continuous Integration

Tests run on every PR:

```yaml
# .github/workflows/ci.yml
- name: Run tests
  run: |
    cargo test --lib
    cargo test --test integration
    cargo test --test integration --target x86_32-unknown-linux-gnu
    cargo test --test integration --target x86_16-boot
```

## Common Issues

| Issue | Solution |
|-------|----------|
| QEMU not found | `sudo apt-get install qemu-system-x86` |
| x86_32 linker errors | Install `gcc-multilib` |
| Temp dir permission | Check `/tmp` permissions |
| Flaky QEMU test | Increase sleep duration |

## Adding Test Coverage

Priority areas needing tests:
- Parser error recovery
- Type inference edge cases
- GC stress tests
- Cross-target parity
- CLI flag combinations
- Error message quality
# Building from Source

Complete guide to building `forgec` from source.

## Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | 1.85+ (edition 2024) | Compiler toolchain |
| Cargo | Included with Rust | Build system |
| Git | Any | Version control |

Install Rust: <https://rustup.rs/>

```bash
# Verify installation
rustc --version
cargo --version
```

## Build Steps

### 1. Clone Repository

```bash
git clone https://github.com/celestiamia/Forge.git
cd Forge
```

### 2. Build Release Binary

```bash
cargo build --release
```

Output: `./target/release/forgec`

### 3. Verify Build

```bash
./target/release/forgec --version
# forgec 0.1.0 (<commit-hash>)

# Test with hello world
./target/release/forgec examples/hello.dev -o hello --target x86_64-unknown-linux-gnu
./hello
# Output: Hello, Forge!
```

### 4. Run Test Suite

```bash
# All tests (unit + integration)
cargo test

# Integration tests only
cargo test --test integration

# Specific target tests
cargo test --test integration -- --target x86_32-unknown-linux-gnu
cargo test --test integration -- --target x86_16-boot
```

## Cross-Compilation

### x86_32 (i686) Target

```bash
# Add target
rustup target add i686-unknown-linux-gnu

# Install 32-bit libraries (Ubuntu/Debian)
sudo apt-get install gcc-multilib g++-multilib libc6-dev-i386

# Build
cargo build --release --target i686-unknown-linux-gnu

# Binary at: target/i686-unknown-linux-gnu/release/forgec
```

### Boot Sector (x86_16)

```bash
# Build using the x86_64 forgec (no Rust target needed)
./target/release/forgec examples/bootloader.dev -o boot.bin --target x86_16-boot

# Test with QEMU
qemu-system-x86_64 -fda boot.bin -nographic
```

## Development Build

For faster iteration during development:

```bash
# Debug build (faster compile, slower runtime)
cargo build

# Binary at: ./target/debug/forgec

# Run with debug logging
RUST_LOG=debug ./target/debug/forgec examples/hello.dev -o hello
```

## Installing System-Wide

```bash
# Using cargo install (builds from local path)
cargo install --path .

# Or manually copy
sudo cp target/release/forgec /usr/local/bin/forgec

# Verify
forgec --version
```

## CI/CD Build

The GitHub Actions workflow (`.github/workflows/ci.yml`) builds all targets:

```yaml
# Matrix builds:
# - x86_64-unknown-linux-gnu (native)
# - i686-unknown-linux-gnu (cross, needs gcc-multilib)
# - x86_16-boot (flat binary via forgec)
```

Local CI simulation:

```bash
# Run all CI steps locally
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --test integration
cargo test --test integration -- --target x86_32-unknown-linux-gnu
cargo test --test integration -- --target x86_16-boot
```

## Build Artifacts

| Artifact | Location | Description |
|----------|----------|-------------|
| `forgec` (x86_64) | `target/release/forgec` | Native compiler |
| `forgec` (x86_32) | `target/i686-unknown-linux-gnu/release/forgec` | 32-bit compiler |
| `boot.bin` | Custom output | 512-byte boot sector |
| Test binaries | `target/debug/deps/` | Integration test outputs |

## Makefile Template

```makefile
FORGEC = ./target/release/forgec
TARGET = x86_64-unknown-linux-gnu

all: hello

hello: examples/hello.dev
	$(FORGEC) $< -o $@ --target $(TARGET)

boot.bin: examples/bootloader.dev
	$(FORGEC) $< -o $@ --target x86_16-boot

test: all
	./hello
	qemu-system-x86_64 -fda boot.bin -nographic

clean:
	rm -f hello boot.bin

.PHONY: all test clean
```

## Troubleshooting Build

| Error | Solution |
|-------|----------|
| `linker 'cc' not found` | Install `build-essential` (Ubuntu) or `base-devel` (Arch) |
| `target not found: i686-unknown-linux-gnu` | Run `rustup target add i686-unknown-linux-gnu` |
| `cannot find -lgcc` | Install `gcc-multilib` / `glibc-devel.i686` |
| `qemu-system-x86_64: command not found` | Install `qemu-system-x86` |
| `error[E0463]: can't find crate for 'std'` | Run `rustup component add rust-src` |

## Profiling the Compiler

```bash
# Build with profiling
cargo build --release --features profiling

# Run with perf
perf record ./target/release/forgec examples/hello.dev -o hello
perf report
```
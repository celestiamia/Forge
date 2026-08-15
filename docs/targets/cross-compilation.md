# Cross Compilation

Building for different targets from a single host.

## Host Requirements

- Linux x86_64 (tested on Ubuntu 20.04+, Fedora 35+)
- Rust 1.85+ (edition 2024)
- Target-specific dependencies

## Target Matrix

| Host → Target | x86_64 | x86_32 | x86_16 |
|---------------|--------|--------|--------|
| Linux x86_64 | ✅ Native | ✅ Cross | ✅ Cross |
| Linux x86_32 | ✅ Native* | ✅ Native | ✅ Cross* |
| macOS x86_64 | ❌ | ❌ | ❌ |
| Windows x86_64 | ❌ | ❌ | ❌ |

* Requires 32-bit host toolchain

## x86_64 → x86_32

```bash
# 1. Add target
rustup target add i686-unknown-linux-gnu

# 2. Install 32-bit libraries
sudo apt-get install gcc-multilib g++-multilib libc6-dev-i386

# 3. Build cross-compiler
cargo build --release --target i686-unknown-linux-gnu

# 4. Compile for x86_32
./target/i686-unknown-linux-gnu/release/forgec input.dev -o output --target x86_32-unknown-linux-gnu
```

### Troubleshooting

| Error | Solution |
|-------|----------|
| `linker 'cc' not found` | Install `build-essential` |
| `cannot find -lgcc` | Install `gcc-multilib` |
| `target not found` | Run `rustup target add i686-unknown-linux-gnu` |

## x86_64 → x86_16

```bash
# Build native compiler
cargo build --release

# Compile boot sector (uses x86_64 forgec)
./target/release/forgec input.dev -o boot.bin --target x86_16-boot
```

No cross-compilation toolchain needed - x86_16 target is handled by Forge's internal assembler.

## x86_32 → x86_64

```bash
# On 32-bit host (rare)
rustup target add x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

## Build Scripts

### Makefile

```makefile
FORGEC_64 = ./target/release/forgec
FORGEC_32 = ./target/i686-unknown-linux-gnu/release/forgec

TARGET_64 = x86_64-unknown-linux-gnu
TARGET_32 = x86_32-unknown-linux-gnu

all: hello_64 hello_32 boot.bin

hello_64: examples/hello.dev
	$(FORGEC_64) $< -o $@ --target $(TARGET_64)

hello_32: examples/hello.dev
	$(FORGEC_32) $< -o $@ --target $(TARGET_32)

boot.bin: examples/bootloader.dev
	$(FORGEC_64) $< -o $@ --target x86_16-boot

clean:
	rm -f hello_64 hello_32 boot.bin
```

### CI/CD (GitHub Actions)

```yaml
# .github/workflows/cross-compile.yml
name: Cross Compile

on: [push, pull_request]

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            name: x86_64
          - target: i686-unknown-linux-gnu
            name: x86_32
            rust_target: i686-unknown-linux-gnu

    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.rust_target }}
      
      - name: Install 32-bit libs
        if: matrix.name == 'x86_32'
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-multilib g++-multilib libc6-dev-i386
      
      - name: Build
        run: cargo build --release --target ${{ matrix.rust_target || '' }}
      
      - name: Test
        run: cargo test --target ${{ matrix.rust_target || '' }}
```

## Docker Cross-Compilation

```dockerfile
# Dockerfile.x86_32
FROM rust:1.70-slim

RUN apt-get update && apt-get install -y \
    gcc-multilib g++-multilib libc6-dev-i386 \
    && rustup target add i686-unknown-linux-gnu

WORKDIR /forge
COPY . .
RUN cargo build --release --target i686-unknown-linux-gnu
```

```bash
docker build -f Dockerfile.x86_32 -t forge-x86_32 .
docker run --rm forge-x86_32 cp /forge/target/i686-unknown-linux-gnu/release/forgec /output/
```

## Target-Specific Code

```dev
# Use target-specific entry files
# build_x86_64.dev
# build_x86_32.dev
# build_boot.dev

# Or use build script to select
# build.sh
#!/bin/bash
TARGET=${1:-x86_64-unknown-linux-gnu}
./target/release/forgec main.dev -o app --target $TARGET
```

## Verification

```bash
# Check binary architecture
file hello
# ELF 64-bit LSB executable, x86-64

file hello32
# ELF 32-bit LSB executable, Intel 80386

file boot.bin
# data (flat binary)

# Check ELF details
readelf -h hello | grep Class
# Class:                             ELF64

readelf -h hello32 | grep Class
# Class:                             ELF32
```

## Limitations

| From → To | Supported | Notes |
|-----------|-----------|-------|
| x86_64 → x86_32 | ✅ | Needs 32-bit libs |
| x86_64 → x86_16 | ✅ | Internal assembler |
| x86_32 → x86_64 | ✅* | Needs 64-bit kernel |
| x86_32 → x86_16 | ✅ | Internal assembler |
| Any → macOS | ❌ | Mach-O not implemented |
| Any → Windows | ❌ | COFF not implemented |
| Any → WASM | ❌ | Not implemented |

## Best Practices

1. **Build on x86_64 Linux** - Most complete support
2. **Use CI matrix** - Test all targets on every commit
3. **Separate binaries** - Don't mix target binaries
4. **Test on target** - Run x86_32 binaries on 32-bit or compat kernel
5. **Boot sector testing** - Always test with QEMU
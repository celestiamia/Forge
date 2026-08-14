# Installation

There are several ways to install `forgec`:

## From GitHub Releases (Recommended)

Download prebuilt binaries from the [releases page](https://github.com/celestiamia/Forge/releases).

### Linux x86_64

```bash
# Download latest release
wget https://github.com/celestiamia/Forge/releases/latest/download/forgec-x86_64-linux

# Make executable and install
chmod +x forgec-x86_64-linux
sudo mv forgec-x86_64-linux /usr/local/bin/forgec

# Verify
forgec --version
```

### Linux x86_32 (i686)

```bash
wget https://github.com/celestiamia/Forge/releases/latest/download/forgec-i686-linux
chmod +x forgec-i686-linux
sudo mv forgec-i686-linux /usr/local/bin/forgec
```

### Boot Sector Binary

```bash
wget https://github.com/celestiamia/Forge/releases/latest/download/forgec-x86_16-boot.bin
# This is a 512-byte boot sector, not an executable
# Use with: qemu-system-x86_64 -fda forgec-x86_16-boot.bin -nographic
```

## From Source

Requires Rust 1.70+.

```bash
# Clone and build
git clone https://github.com/celestiamia/Forge.git
cd Forge
cargo build --release

# The binary is at ./target/release/forgec
./target/release/forgec --version

# Optional: install system-wide
sudo cp target/release/forgec /usr/local/bin/forgec
```

## Cross-Compilation Targets

The compiler supports multiple targets:

```bash
# List available targets
forgec --help

# Build for x86_32 (requires i686 toolchain)
rustup target add i686-unknown-linux-gnu
cargo build --release --target i686-unknown-linux-gnu

# Build boot sector (no extra toolchain needed)
./target/release/forgec examples/bootloader.dev -o boot.bin --target x86_16-boot
```

## Nightly Builds

Automated nightly builds are available from the [nightly releases](https://github.com/celestiamia/Forge/releases?q=nightly&expanded=true).

```bash
# Latest nightly x86_64
wget https://github.com/celestiamia/Forge/releases/download/nightly-latest/forgec-x86_64-linux
```

## Verification

All releases include SHA256 checksums:

```bash
# Download checksums
wget https://github.com/celestiamia/Forge/releases/latest/download/SHA256SUMS

# Verify
sha256sum -c SHA256SUMS --ignore-missing
```

## Requirements

| Target | Requirements |
|--------|--------------|
| x86_64 Linux | glibc 2.17+, Linux kernel 3.2+ |
| x86_32 Linux | glibc 2.17+, i686 libraries (`gcc-multilib`, `libc6-dev-i386`) |
| x86_16 Boot | QEMU for testing (`qemu-system-x86_64`) |

## Building from Source (Development)

```bash
# Clone with submodules (if any)
git clone https://github.com/celestiamia/Forge.git
cd Forge

# Install Rust toolchain (if not installed)
rustup toolchain install stable

# Build
cargo build --release

# Run tests
cargo test

# Install locally
cargo install --path .
```

## Troubleshooting

### x86_32 Link Errors

```bash
# Ubuntu/Debian
sudo apt-get install gcc-multilib g++-multilib libc6-dev-i386

# Fedora
sudo dnf install glibc-devel.i686 libstdc++-devel.i686
```

### QEMU Not Found (Boot Sector Tests)

```bash
# Ubuntu/Debian
sudo apt-get install qemu-system-x86

# macOS
brew install qemu
```

### Permission Denied

```bash
# Make sure the binary is executable
chmod +x forgec

# If installed to /usr/local/bin, ensure it's in PATH
export PATH="/usr/local/bin:$PATH"
```
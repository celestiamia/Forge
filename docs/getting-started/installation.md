# Installation

There are several ways to install `forgec`:

## From the AUR (Arch Linux)

`forgec` is available on the Arch User Repository as
[`forgec-git`](https://aur.archlinux.org/packages/forgec-git), which tracks the
latest commit on `main`. Install it with your favorite AUR helper:

```bash
paru -S forgec-git
# or
yay -S forgec-git
```

The AUR package builds `forgec` from source, and the installed binary is fully
self-contained — the standard library is embedded, so no `core/` directory or
other data files are needed.

## From GitHub Releases

`forgec` is distributed as **nightly prereleases** — one per day, tagged
`nightly-<version>-nightly-<date>-<commit>`, e.g.
`nightly-0.1.0-nightly-20260815-584718c`.

Browse the [releases page](https://github.com/celestiamia/Forge/releases) and
filter by *Pre-release*, or download a specific build:

```bash
# Example: download the nightly from 2026-08-15
wget https://github.com/celestiamia/Forge/releases/download/nightly-0.1.0-nightly-20260815-584718c/forgec-x86_64-linux
```

### Linux x86_64

```bash
# Make executable and install
chmod +x forgec-x86_64-linux
sudo mv forgec-x86_64-linux /usr/local/bin/forgec

# Verify
forgec --version
```

### Linux x86_32 (i686)

```bash
# From the same release page: forgec-i686-linux
chmod +x forgec-i686-linux
sudo mv forgec-i686-linux /usr/local/bin/forgec
```

### Boot Sector Binary

```bash
# forgec-x86_16-boot.bin is a 512-byte boot sector, not an executable
# Use with: qemu-system-x86_64 -fda forgec-x86_16-boot.bin -nographic
```

> There is no stable release yet — `releases/latest` is not populated until
> the first non-prerelease tag. Use a specific nightly tag (above) or build
> from source.

## From Source

Requires Rust 1.85+ (edition 2024).

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

The compiled binary is self-contained: the `std.*` modules are embedded, so it
works from any directory — no `core/` stdlib checkout is required.

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

## Verification

All releases include SHA256 checksums. To verify a downloaded binary, open
the release page's `SHA256SUMS` asset or fetch it for a specific tag:

```bash
wget https://github.com/celestiamia/Forge/releases/download/nightly-0.1.0-nightly-20260815-584718c/SHA256SUMS
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
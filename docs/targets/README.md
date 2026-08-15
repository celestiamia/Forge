# Targets

Forge supports multiple compilation targets with different capabilities.

## Target Overview

| Target Triple | Architecture | Format | Hosted | Use Case |
|---------------|--------------|--------|--------|----------|
| `x86_64-unknown-linux-gnu` | x86_64 | ELF64 | ✅ | Native Linux apps |
| `x86_32-unknown-linux-gnu` | i686 | ELF32 | ✅ | 32-bit Linux apps |
| `x86_16-boot` | 8086 | Flat (512B) / raw stage images | ❌ | Boot sectors, multi-stage OSes |

## Common Flags

```bash
forgec input.dev -o output --target <triple>
```

| Flag | Description |
|------|-------------|
| `--target <triple>` | Target triple (default: `x86_64-unknown-linux-gnu`) |
| `-o <file>` | Output file (default: input name) |
| `--freestanding` | No hosted runtime, custom entry point |
| `--linker <path>` | Custom target config (`.fld` linker descriptor) |
| `--help` | Show all options |

## x86_64 Linux

**Target**: `x86_64-unknown-linux-gnu`
- **Architecture**: x86-64 (AMD64)
- **Format**: ELF64
- **Mode**: Hosted (Linux syscalls)
- **Pointer**: 64-bit
- **Features**: Full stdlib, GC, floats, sockets

```bash
forgec hello.dev -o hello --target x86_64-unknown-linux-gnu
./hello
```

### ABI
- System V AMD64 calling convention
- Args: RDI, RSI, RDX, RCX, R8, R9
- Stack 16-byte aligned at calls
- Red zone: 128 bytes below RSP

### Syscalls
Direct Linux syscalls via `syscall` instruction:
- `write` (1), `read` (0), `exit` (60)
- `mmap` (9), `munmap` (11)
- `socket` (41), `connect` (42)

### Standard Library
Full support: `std.io`, `std.mem`, `std.string`, `std.math`, `std.alloc`, `std.fmt`, `std.volatile`, `std.gc`

## x86_32 Linux

**Target**: `x86_32-unknown-linux-gnu` (Rust target: `i686-unknown-linux-gnu`)
- **Architecture**: i686 (Pentium Pro+)
- **Format**: ELF32
- **Mode**: Hosted (Linux syscalls)
- **Pointer**: 32-bit
- **No floats**: `float32`/`float64` not supported

```bash
# Requires 32-bit toolchain
rustup target add i686-unknown-linux-gnu
sudo apt-get install gcc-multilib g++-multilib libc6-dev-i386

cargo build --release --target i686-unknown-linux-gnu
./target/i686-unknown-linux-gnu/release/forgec hello.dev -o hello --target x86_32-unknown-linux-gnu
./hello
```

### ABI
- cdecl calling convention
- Args on stack (right-to-left)
- Return in EAX
- Stack 4-byte aligned

### Syscalls
- `int 0x80` instruction
- Syscall numbers differ from x86_64
- `write` (4), `read` (3), `exit` (1)

### Limitations
- ❌ No float support (`float32`, `float64`)
- ❌ No GC (`std.gc`)
- ✅ Bump allocator (`std.alloc`)

### Standard Library
| Module | Support |
|--------|---------|
| `std.io` | ✅ |
| `std.mem` | ✅ |
| `std.string` | ✅ |
| `std.math` | ✅ (int only) |
| `std.alloc` | ✅ |
| `std.fmt` | ✅ |
| `std.volatile` | ✅ |
| `std.gc` | ❌ |

## x86_16 Real Mode

**Target**: `x86_16-boot`
- **Architecture**: 8086/8088 (real mode)
- **Format**: Flat boot sector (512 bytes, `0x55AA` signature) or bare `raw`
  stage images via a `.fld` descriptor
- **Mode**: Freestanding (no OS)
- **Pointer**: 16-bit (segmented)
- **Origin**: `0x7C00` (BIOS load address; override per-stage with the
  `.fld` `LOAD` directive)

```bash
# Build boot sector
./target/release/forgec examples/bootloader.dev -o boot.bin --target x86_16-boot

# Build a stage-2 image loaded elsewhere (LOAD 0x9000)
./target/release/forgec src/boot/loader.dev -o loader.raw --linker examples/os/os-loader.fld

# Run in QEMU
qemu-system-x86_64 -fda boot.bin -nographic
```

### Constraints
- **`flat` images: 512 bytes total** (including 0x55AA signature); `raw`
  images are limited only by what the loading stage can read
- **16-bit real mode** (no protection, no paging)
- **Segmented memory** (segment:offset)
- **BIOS interrupts** for I/O
- **No heap** (no `alloc`, `gc`)
- **No syscalls** (no OS)

### Memory Model
```
0x00000 - 0x003FF  : Interrupt Vector Table (IVT)
0x00400 - 0x004FF  : BIOS Data Area
0x00500 - 0x07BFF  : Free (conventional memory)
0x07C00 - 0x07DFF  : Boot sector (load address)
0x07E00 - 0x9FFFF  : Free
```

### Standard Library (Minimal)

| Module | Support |
|--------|---------|
| `std.io` | ✅ (BIOS teletype/serial) |
| `std.mem` | ✅ |
| `std.string` | ✅ |
| `std.math` | ✅ (int only) |
| `std.alloc` | ❌ |
| `std.fmt` | ✅ |
| `std.volatile` | ✅ |
| `std.gc` | ❌ |
| `std.runtime` | ❌ |

### Example: Bootloader

```dev
@freestanding
pub def _start() -> void:
    puts("Hello, Forge bootloader!")
    _dev_halt()

def puts(msg: ptr[char]) -> void:
    unsafe:
        var p = msg
        var c = _dev_load_char(p)
        while c != 0:
            _dev_bios_teletype(c)
            _dev_serial_putc(c)
            p = p + 1
            c = _dev_load_char(p)

extern def _dev_bios_teletype(c: char) -> void
extern def _dev_serial_putc(c: char) -> void
extern def _dev_load_char(p: ptr[char]) -> char
extern def _dev_halt() -> void
```

### Build Process

1. Forge compiles to x86-16 machine code via the internal 16-bit assembler
2. `FORMAT flat`: pads to 510 bytes + adds `0x55AA` signature
3. `FORMAT raw`: emits the image as-is, string addresses fixed up against
   the stage's `LOAD` address

The [ForgeOS example](../examples/os.md) shows all of it in one project: a
`flat` boot sector loading a `raw` loader at `0x9000`, which loads a `raw`
kernel at `0x7C00` — no inline assembly anywhere.

### Testing

```bash
# Build
forgec examples/bootloader.dev -o boot.bin --target x86_16-boot

# Run in QEMU
qemu-system-x86_64 -fda boot.bin -nographic

# Expected output:
# SeaBIOS...
# Booting from Floppy...
# Hello, Forge bootloader!
```

## Cross Compilation

```bash
# x86_64 host → x86_32 target
cargo build --release --target i686-unknown-linux-gnu
./target/i686-unknown-linux-gnu/release/forgec input.dev -o out --target x86_32-unknown-linux-gnu

# x86_64 host → x86_16 target (uses x86_64 forgec)
./target/release/forgec input.dev -o out --target x86_16-boot
```

## Target Selection in Code

```dev
# Conditional compilation not yet supported
# Use separate entry files or build scripts

# build_x86_64.dev
# build_x86_32.dev
# build_boot.dev
```

## Future Targets

| Target | Status |
|--------|--------|
| `x86_64-apple-darwin` | Planned (Mach-O) |
| `x86_64-pc-windows-gnu` | Planned (COFF) |
| `riscv64-unknown-elf` | Planned |
| `aarch64-unknown-linux-gnu` | Planned |
| `wasm32-unknown-unknown` | Planned |
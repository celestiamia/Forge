# Bootloader Example

Bare-metal 512-byte boot sector written entirely in Forge.

## Source: `examples/bootloader.dev`

```dev
package bootloader

extern def _dev_bios_teletype(c: char) -> void
extern def _dev_serial_putc(c: char) -> void
extern def _dev_load_char(p: ptr[char]) -> char
extern def _dev_halt() -> void

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
```

## Key Concepts

| Concept | Explanation |
|---------|-------------|
| `@freestanding` | No standard library, custom entry point |
| `_start` | Entry point (not `main`) |
| `extern` | External functions (BIOS/serial) |
| `unsafe` | Required for raw pointer ops |
| `--target x86_16-boot` | 16-bit real mode, flat binary |

## External Functions

| Function | BIOS Interrupt | Purpose |
|----------|----------------|---------|
| `_dev_bios_teletype` | INT 10h, AH=0Eh | Print char to screen |
| `_dev_serial_putc` | INT 14h | Print char to serial port |
| `_dev_load_char` | N/A | Load char from string |
| `_dev_halt` | HLT instruction | Halt CPU |

## Compile & Run

```bash
# Build (uses x86_64 forgec with internal 16-bit assembler)
forgec examples/bootloader.dev -o boot.bin --target x86_16-boot

# Verify
ls -la boot.bin           # 512 bytes
hexdump -C boot.bin | tail -1  # Check 0x55AA signature

# Run in QEMU
qemu-system-x86_64 -fda boot.bin -nographic
```

Output:
```
SeaBIOS (version Arch Linux 1.17.0-2-2)
iPXE (http://ipxe.org) 00:03.0 C900 PCI2.10 PnP PMM+06FD4040+06F34040 C900
Booting from Hard Disk...
Boot failed: could not read the boot disk
Booting from Floppy...
Hello, Forge bootloader!
```

## Boot Sector Format

```
Offset 0x0000-0x01FD: Code + Data (510 bytes)
Offset 0x01FE-0x01FF: 0x55 0xAA (Boot signature)
Total: 512 bytes
```

## Constraints

| Limit | Value |
|-------|-------|
| Max size | 510 bytes code + 2 bytes signature |
| Memory | Real mode (1MB addressable) |
| Pointers | 16-bit (segmented) |
| Stack | ~64KB conventional memory |
| Heap | None |
| Protection | None (ring 0) |

## BIOS Interrupts Used

| Interrupt | Function | Registers |
|-----------|----------|-----------|
| INT 10h, AH=0Eh | Teletype | AL=char, BH=page |
| INT 14h, AH=01h | Serial write | AL=char, DX=port |
| HLT | Halt CPU | - |

## Memory Setup

```dev
@freestanding
pub def _start() -> void:
    unsafe:
        # Set up segments
        asm!("xor ax, ax")
        asm!("mov ds, ax")
        asm!("mov es, ax")
        asm!("mov ss, ax")
        asm!("mov sp, 0x7C00")
    
    puts("Hello!")
    _dev_halt()
```

## Testing

```bash
# Build
forgec examples/bootloader.dev -o boot.bin --target x86_16-boot

# Verify size
stat -c%s boot.bin  # Should be 512

# Verify signature
xxd boot.bin | tail -1
# 000001f0: 0000 0000 0000 0000 0000 0000 0000 55 aa

# Run with QEMU
qemu-system-x86_64 -fda boot.bin -nographic

# With GDB
qemu-system-x86_64 -fda boot.bin -nographic -s -S
# gdb -ex "target remote localhost:1234" -ex "set architecture i8086"
```

## Debugging Tips

1. **Use serial output** - BIOS teletype may not show in all emulators
2. **Check signature** - Must end with `0x55 0xAA`
3. **Size limit** - Keep under 510 bytes
4. **Segment setup** - Always set DS/ES/SS explicitly

## Extending the Bootloader

### Load Second Stage

```dev
@freestanding
pub def _start() -> void:
    # Read sector 2 from disk
    asm!("mov ah, 0x02")      # Read sectors
    asm!("mov al, 1")         # 1 sector
    asm!("mov ch, 0")         # Cylinder 0
    asm!("mov cl, 2")         # Sector 2
    asm!("mov dh, 0")         # Head 0
    asm!("mov dl, 0")         # Drive 0
    asm!("mov bx, 0x7E00")    # Load to 0x7E00
    asm!("int 0x13")          # BIOS disk interrupt
    jc disk_error
    
    # Jump to loaded code
    asm!("jmp 0x0000:0x7E00")

disk_error:
    puts("Disk error!")
    _dev_halt()
```

### Protected Mode Switch

```dev
@freestanding
pub def _start() -> void:
    # Disable interrupts
    asm!("cli")
    
    # Load GDT
    asm!("lgdt [gdt_ptr]")
    
    # Enable protected mode
    asm!("mov eax, cr0")
    asm!("or eax, 1")
    asm!("mov cr0, eax")
    
    # Far jump to 32-bit code
    asm!("jmp 0x08:pm_entry")
    
    # ... 32-bit code follows ...
```

## Common Issues

| Problem | Cause | Fix |
|---------|-------|-----|
| "Boot failed" | Wrong signature | Verify 0x55AA at offset 510 |
| Blank screen | Wrong video mode | Use INT 10h AH=00h to set mode |
| Hangs | Infinite loop | Check HLT placement |
| Garbage output | Wrong segment | Set DS/ES explicitly |
| QEMU exits | Triple fault | Check for invalid memory access |
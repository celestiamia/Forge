# std.hal

Hardware I/O and interrupt-control primitives for **freestanding** targets
(x86_16 boot sectors, x86_32 kernels, x86_64 kernels).  The functions are
thin, typed wrappers around compiler-emitted `_dev_*` helpers — the
Forge-idiomatic alternative to inline assembly.

> This module is intended for freestanding targets only.  On hosted Linux
> targets the `_dev_*` port-I/O helpers are not emitted, so calling functions
> from this module fails at compile time.  Use `std.io` for hosted I/O.

## Port I/O

```dev
def outb(port: u16, val: u8) -> void    # OUT DX, AL (8-bit)
def inb(port: u16) -> u8                # IN AL, DX (8-bit)
def outw(port: u16, val: u16) -> void   # OUT DX, AX (16-bit)
def inw(port: u16) -> u16               # IN AX, DX (16-bit)
```

```dev
from std.hal import outb, inb

outb(0x3F8, 0x41)     # Write 'A' to COM1
let c = inb(0x3F8)    # Read it back
```

32-bit port I/O (`_dev_outl`/`_dev_inl`, `OUT DX, EAX` / `IN EAX, DX`) is
available on x86_64 and x86_32 freestanding runtimes via the raw `extern`
declarations; the x86_16 backend does not support it.

## Software Interrupts

```dev
extern def _dev_int(n: int32) -> void
```

`_dev_int` is **not a function call** — the compiler desugars calls to it
with a literal argument into an inline `INT nn` instruction (opcode `0xCD`).
The vector must be a compile-time constant in `0..=255`; anything else is a
compile-time error.

```dev
from std.hal import _dev_int

_dev_int(0x80)     # Emits INT 0x80 directly, no call
```

## Interrupt Control

```dev
def iret() -> void   # Return from hardware interrupt handler (IRET)
def sti() -> void    # Enable maskable hardware interrupts (STI)
def cli() -> void    # Disable maskable hardware interrupts (CLI)
def halt() -> void   # Disable interrupts, then spin-halt (CLI; HLT loop)
```

## PIC 8259A Arbitration

```dev
def pic_init() -> void           # Remap both PICs to vectors 0x20-0x2F
def pic_send_eoi(irq: u8) -> void # End-of-interrupt for the given IRQ line
```

`pic_init` remaps the master and slave 8259 PICs so IRQs 0–15 map to
interrupt vectors `0x20`–`0x27` (master) and `0x28`–`0x2F` (slave) — the
standard x86 layout that avoids the CPU's reserved exception vectors.  Call
it after entering protected or long mode, before enabling interrupts with
`sti()`.  It is implemented in pure Forge via `outb`.

`pic_send_eoi` acknowledges the interrupt.  On slave IRQs (≥ 8) an
additional EOI is sent to the slave PIC.

## Port Constants

```dev
const COM1_BASE: u16 = 0x3F8
const COM2_BASE: u16 = 0x2F8
const COM3_BASE: u16 = 0x3E8
const COM4_BASE: u16 = 0x2E8

const PIC_MASTER_CMD: u16 = 0x20
const PIC_MASTER_DATA: u16 = 0x21
const PIC_SLAVE_CMD: u16 = 0xA0
const PIC_SLAVE_DATA: u16 = 0xA1
```

## Example: serial console init

```dev
from std.hal import outb

# Initialize COM1 at 115200 baud (divisor = 1)
pub def serial_init() -> void:
    outb(0x3F8 + 1, 0x00)   # Disable all interrupts
    outb(0x3F8 + 3, 0x80)   # Enable DLAB
    outb(0x3F8 + 0, 0x01)   # Divisor low byte
    outb(0x3F8 + 1, 0x00)   # Divisor high byte
    outb(0x3F8 + 3, 0x03)   # 8 bits, no parity, one stop bit
```

## Target Support

| Function | x86_64 | x86_32 | x86_16 |
|----------|--------|--------|--------|
| `outb` / `inb` | ✅ | ✅ | ✅ |
| `outw` / `inw` | ✅ | ✅ | ✅ |
| `_dev_outl` / `_dev_inl` | ✅ | ✅ | ❌ (32-bit types rejected) |
| `_dev_int(n)` (inline `INT nn`) | ✅ | ✅ | ✅ |
| `sti` / `cli` / `iret` / `halt` | ✅ | ✅ | ✅ |
| `pic_init` / `pic_send_eoi` | ✅ | ✅ | ✅ |

All helpers are **reference-driven**: only the ones a program actually
references get emitted, so importing this module is harmless even if you only
use a subset.  The ForgeOS32 and ForgeOS64 kernels use `std.hal` for their
serial, VGA, and PIC setup.
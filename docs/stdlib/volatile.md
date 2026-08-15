# std.volatile

Width-correct volatile memory access and memory barriers for hardware
interaction.

## Volatile Loads

```dev
def load_u8(p: ptr[uint8]) -> uint8
def load_u16(p: ptr[uint16]) -> uint16
def load_u32(p: ptr[uint32]) -> uint32
def load_u64(p: ptr[uint64]) -> uint64

def load_i8(p: ptr[int8]) -> int8
def load_i16(p: ptr[int16]) -> int16
def load_i32(p: ptr[int32]) -> int32
def load_i64(p: ptr[int64]) -> int64

def load_ptr(p: ptr[ptr[uint8]]) -> ptr[uint8]
```

Read from memory with the exact width. Every call emits a real memory access
(no caching, no elimination).

```dev
from std.volatile import load_u32

var status_reg: uint32 = 0
let status = load_u32(&status_reg)
```

## Volatile Stores

```dev
def store_u8(p: ptr[uint8], v: uint8) -> void
def store_u16(p: ptr[uint16], v: uint16) -> void
def store_u32(p: ptr[uint32], v: uint32) -> void
def store_u64(p: ptr[uint64], v: uint64) -> void

def store_i8(p: ptr[int8], v: int8) -> void
def store_i16(p: ptr[int16], v: int16) -> void
def store_i32(p: ptr[int32], v: int32) -> void
def store_i64(p: ptr[int64], v: int64) -> void

def store_ptr(p: ptr[ptr[uint8]], v: ptr[uint8]) -> void
```

```dev
from std.volatile import store_u32

var ctrl_reg: uint32 = 0
store_u32(&ctrl_reg, 0x1)   # Enable device
```

For MMIO, cast an absolute address to a pointer first:

```dev
let dr = 0x3F8 as ptr[uint8]
store_u8(dr, 65 as uint8)
```

## Memory Barriers

```dev
def full_barrier() -> void   # MFENCE: all loads and stores
def read_barrier() -> void   # LFENCE: loads only
def write_barrier() -> void  # SFENCE: stores only
```

```dev
from std.volatile import full_barrier

full_barrier()   # Ensure all previous writes are visible before reads
```

## Example: Memory-Mapped UART

```dev
from std.volatile import store_u8, load_u8, full_barrier

pub def uart_putc(c: char) -> void:
    let dr = 0x3F8 as ptr[uint8]    # Data register
    let lsr = 0x3FD as ptr[uint8]   # Line status register
    loop:
        if (load_u8(lsr) & 0x20 as uint8) != 0 as uint8:
            break
    store_u8(dr, c as uint8)
    full_barrier()
```

## Target Support

| Target | Barriers |
|--------|----------|
| x86_64 | `mfence`, `lfence`, `sfence` instructions |
| x86_32 | `mfence`, `lfence`, `sfence` |
| x86_16 | Not available |

## Safety

- Width-correct access only — use the function matching the register width
- No bounds checking; the caller must ensure the pointer is valid
- Barriers are available on hosted x86 targets; the x86_16 boot target does
  not support them
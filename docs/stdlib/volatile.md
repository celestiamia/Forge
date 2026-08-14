# std.volatile

Volatile memory access and memory barriers for hardware interaction.

## Volatile Access

### volatile_load

```dev
def volatile_load(ptr: ptr[byte]) -> byte
def volatile_load(ptr: ptr[uint16]) -> uint16
def volatile_load(ptr: ptr[uint32]) -> uint32
def volatile_load(ptr: ptr[uint64]) -> uint64
```

Read from memory with volatile semantics - compiler won't optimize away or reorder.

```dev
from std.volatile import volatile_load

let status_reg = 0xFFFF0000 as ptr[uint32]
let status = volatile_load(status_reg)
```

### volatile_store

```dev
def volatile_store(ptr: mut ptr[byte], value: byte) -> void
def volatile_store(ptr: mut ptr[uint16], value: uint16) -> void
def volatile_store(ptr: mut ptr[uint32], value: uint32) -> void
def volatile_store(ptr: mut ptr[uint64], value: uint64) -> void
```

Write to memory with volatile semantics.

```dev
from std.volatile import volatile_store

let ctrl_reg = 0xFFFF0004 as mut ptr[uint32]
volatile_store(ctrl_reg, 0x1)  # Enable device
```

## Memory Barriers

### mfence (Full Barrier)

```dev
def mfence() -> void
```

Full memory fence: prevents reordering of loads and stores across the fence.

```dev
from std.volatile import mfence

# Ensure all previous writes visible before reads
mfence()
```

### lfence (Load Barrier)

```dev
def lfence() -> void
```

Load fence: prevents load reordering across the fence.

```dev
from std.volatile import lfence

# Prevent speculative loads
lfence()
```

### sfence (Store Barrier)

```dev
def sfence() -> void
```

Store fence: prevents store reordering across the fence.

```dev
from std.volatile import sfence

# Ensure writes visible to other cores
sfence()
```

## Usage Patterns

### Memory-Mapped I/O

```dev
from std.volatile import volatile_load, volatile_store, mfence

# UART registers
const UART_BASE: uint32 = 0x3F8
const UART_DR: uint32 = UART_BASE + 0
const UART_LSR: uint32 = UART_BASE + 5

def uart_putc(c: char):
    let lsr = UART_LSR as ptr[uint8]
    let dr = UART_DR as mut ptr[uint8]
    
    # Wait for transmitter empty
    loop:
        if (volatile_load(lsr) & 0x20) != 0:
            break
    volatile_store(dr, c as uint8)
    mfence()  # Ensure write completes
```

### Spinlock

```dev
from std.volatile import volatile_load, volatile_store, mfence

struct Spinlock:
    locked: uint32

impl Spinlock:
    def lock(refmut self):
        loop:
            # Atomic exchange (simplified - needs atomic ops)
            if self.locked == 0:
                volatile_store(&mut self.locked, 1)
                mfence()
                return
    
    def unlock(refmut self):
        mfence()
        volatile_store(&mut self.locked, 0)
```

### Device Driver Pattern

```dev
from std.volatile import volatile_load, volatile_store, mfence, lfence, sfence

struct DeviceRegs:
    ctrl: uint32
    status: uint32
    data: uint32

def device_init(regs: ptr[DeviceRegs]) -> void:
    volatile_store(&mut (*regs).ctrl, 0x1)  # Reset
    mfence()
    volatile_store(&mut (*regs).ctrl, 0x3)  # Enable + IRQ
    sfence()  # Ensure control writes visible
```

## Target Support

| Target | Barriers |
|--------|----------|
| x86_64 | `mfence`, `lfence`, `sfence` instructions |
| x86_32 | `mfence`/`lfence`/`sfence` (SSE2+) or `lock addl $0,0(%esp)` |
| x86_16 | `lock` prefix or `cli`/`sti` |

## Compiler Guarantees

Volatile operations:
- Not eliminated by dead code elimination
- Not reordered with other volatile operations
- Not reordered with memory barriers
- May be reordered with non-volatile operations (use barriers)

## Best Practices

1. **Use volatile for MMIO** - Never use regular pointers for hardware registers
2. **Pair barriers** - Write barrier before read, read barrier after write
3. **Minimize scope** - Keep volatile sections small
4. **Document assumptions** - Memory ordering requirements

## Implementation

```dev
# x86_64 implementation
def volatile_load_u32(ptr: ptr[uint32]) -> uint32:
    asm!("mov {}, [{}]", out(reg) result, in(reg) ptr)

def volatile_store_u32(ptr: mut ptr[uint32], val: uint32):
    asm!("mov [{}], {}", in(reg) ptr, in(reg) val)

def mfence():
    asm!("mfence")
```
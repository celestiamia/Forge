# Forge Linker Descriptor (`.fld`)

A `.fld` file (Forge Linker Descriptor) describes a compilation target: its
architecture, output format, memory layout, runtime capabilities, and entry
point.  Passing one to `forgec` with `--linker` replaces the built-in target
presets with a fully user-specified configuration.

```bash
forgec hello.dev -o hello --linker examples/targets/x86_64-linux.fld
```

Working examples live in [`examples/targets/`](../../examples/targets/):
`x86_64-linux.fld`, `x86_32-linux.fld`, and `x86_16-boot.fld` — each mirrors
the built-in target of the same name.

## Grammar

The format is line-oriented and case-sensitive.  Directives may appear in any
order; `ARCH` and `FORMAT` are required.  `#` starts a comment that runs to
the end of the line.

```
ARCH   <name>                 # x86_64 | x86_32 | x86_16   (required)
FORMAT <name>                 # elf | elf32 | flat | raw   (required)
HOSTED <bool>                 # true | false               (default: true)
ENTRY  <symbol>               # entry function             (see defaults below)
LOAD   <hex>                  # x86_16 load address        (default: 0x7C00)
HEAP   size = <number>        # GC heap bytes              (default: 0)

MEMORY {
    <name> (<flags>) : origin = <number>, length = <number>
}

SECTIONS {
    .<section> > <region>
    ...
}

RUNTIME {
    syscalls = <bool>
    gc       = <bool>
    alloc    = <bool>
    float    = <bool>
    sockets  = <bool>
    files    = <bool>
}
```

### Numbers

- Decimal (`512`, `4096`) or hexadecimal (`0x7C00`) integers.
- `_` is allowed as a digit separator (`1_048_576`).
- Size suffixes: `K`/`k` = 1024, `M`/`m` = 1024², `G`/`g` = 1024³
  (`64K`, `4M`, `1G`).

### Directives

| Directive | Description |
|-----------|-------------|
| `ARCH` | Codegen backend: `x86_64`, `x86_32` (ELF32), or `x86_16` (flat boot). **Required.** |
| `FORMAT` | Output format: `elf`, `elf32`, `flat`, or `raw`. **Required.** `flat` produces a 512-byte boot sector (padded, `0x55AA` signature) and is x86_16-only; `raw` produces a bare image. `raw` is accepted for `x86_16` (multi-sector boot stages loaded by a boot sector) **and** for `x86_32` (a flat binary kernel loaded by a stage-2 loader at the address given by `LOAD`). |
| `HOSTED` | `true` emits the hosted `_start` runtime stub that calls the entry function; `false` targets the entry function directly. Defaults to `true`. |
| `ENTRY` | Entry function. Defaults to `_forge_main` (the mangled `pub def main()`) when hosted, `_start` when freestanding. In hosted mode this is the function the runtime calls; in freestanding mode it is the function execution begins at. |
| `LOAD` | Load address for x86_16 `flat`/`raw` images (absolute string-address fixup base; default `0x7C00`). For x86_32 `raw`, the link base of the flat binary — the stage-2 loader reads the image bytes to this address and jumps here. |
| `HEAP` | Size of the compiler-emitted heap in bytes. Must be non-zero when `RUNTIME alloc` or `RUNTIME gc` is enabled. Backs the GC arena on x86_64 (`gc`/`alloc`) and the free-list heap on x86_32 (`alloc`; `gc` is a clean error on x86_32). |
| `MEMORY { }` | Named memory regions with permission flags (`r`, `w`, `x`), origin, and length. |
| `SECTIONS { }` | Maps output sections (`.text`, `.rodata`, `.data`, `.bss`) to memory regions. |
| `RUNTIME { }` | Capability flags for the runtime helper set. All default to `false` except `syscalls` and `float`, which default to `true`. |

### Memory region / section syntax

```text
MEMORY {
    rom (rx)  : origin = 0x0000,    length = 64K
    ram (rwx) : origin = 0x10000,   length = 512K
}

SECTIONS {
    .text   > rom
    .rodata > rom
    .data   > ram
    .bss    > ram
}
```

Each `MEMORY` entry is `name (flags) : origin = <n>, length = <n>` with an
optional trailing comma.  Each `SECTIONS` entry is `.<section> > <region>`.

## Defaults and validation

The following defaults apply when a directive is omitted:

| Directive | Default |
|-----------|---------|
| `HOSTED` | `true` |
| `ENTRY` | `_forge_main` (hosted) / `_start` (freestanding) |
| `LOAD` | `0x7C00` (x86_16 only) |
| `HEAP` | `0` (no heap) |
| `RUNTIME` | `syscalls = true`, `float = true`, all others `false` |

The configuration is validated before codegen; violations are hard errors:

- `ARCH` must be `x86_64`, `x86_32`, or `x86_16`.
- Architecture and format must match: `x86_64` ⇒ `elf`, `x86_32` ⇒ `elf32`,
  `x86_16` ⇒ `flat` or `raw`.
- `FORMAT raw` is accepted only for `x86_16` and `x86_32`.
- `LOAD` is accepted for x86_16 `flat`/`raw` (default `0x7C00`) and required for x86_32 `raw` (the kernel link base); it is rejected for `elf`/`elf32` and hosted targets.
- Flat binaries cannot be hosted.
- `RUNTIME gc = true` requires `RUNTIME alloc = true`.
- `RUNTIME alloc` or `gc` requires `HEAP size > 0`.
- Hosted targets require a non-empty `ENTRY`.

## Built-in presets

The three built-in targets are exactly these configurations
(`src/linker/config.rs`):

| Preset | ARCH | FORMAT | HOSTED | ENTRY | HEAP |
|--------|------|--------|--------|-------|------|
| `x86_64-unknown-linux-gnu` | `x86_64` | `elf` | `true` | `_forge_main` | `4M` |
| `x86_32-unknown-linux-gnu` | `x86_32` | `elf32` | `true` | `_forge_main` | `0` |
| `x86_16-boot` | `x86_16` | `flat` | `false` | `_start` | `0` |

## Implementation status

| Directive | Status |
|-----------|--------|
| `ARCH` | Honored — selects the codegen backend. |
| `FORMAT` | Honored — `elf`/`elf32` select the object writer; `flat` pads to a 512-byte boot sector with the `0x55AA` signature, `raw` emits the bare image. |
| `HOSTED` | Honored — gates runtime emission. |
| `ENTRY` | Honored — hosted: the function the runtime calls; freestanding: the entry function. |
| `LOAD` | Honored — base address for absolute (string) fixups in x86_16 images. |
| `HEAP` | Honored — sets the GC arena size in `.bss` (x86_64 hosted, default 4 MiB). |
| `RUNTIME float` | Honored — when `false`, floating-point operations fail at codegen. |
| `RUNTIME gc/alloc/syscalls/sockets/files` | Describes capabilities; the compiler emits a helper only when the program actually references it (e.g. `std.gc` pulls in `_dev_gc_*`). |
| `MEMORY` / `SECTIONS` | Parsed and validated; not yet applied to object layout (writers use fixed base addresses). |
| `STACK` | Accepted and skipped (reserved). |

## Differences from the built-in targets

Because a linker script is explicit where presets hardcode, a custom `.fld`
behaves exactly like the preset it mirrors — the example files under
`examples/targets/` are provided precisely so you can copy one as a starting
point.  Two behaviors to be aware of:

- Freestanding compilation against a **hosted built-in preset** (`--freestanding`
  without `--linker`) enters at `_start`.  A custom script's `ENTRY` is always
  taken verbatim.
- The runtime's capability flags do not force emission of helpers; a program
  that never imports `std.alloc` gets no allocator even if the script enables
  it.
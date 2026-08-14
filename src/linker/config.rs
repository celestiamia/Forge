//! Target configuration for the Forge linker script system.
//!
//! A [`LinkerConfig`] describes everything the compiler needs to know about a
//! target: its architecture, output format, memory layout, which runtime
//! helpers to emit, and the entry point symbol.  It is produced either by
//! parsing a `.fld` (Forge Linker Descriptor) file or by instantiating one of
//! the built-in target presets.

use anyhow::Result;

/// Output binary format produced by the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFormat {
    Elf,
    Elf32,
    Flat,
    Raw,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Elf => "elf",
            OutputFormat::Elf32 => "elf32",
            OutputFormat::Flat => "flat",
            OutputFormat::Raw => "raw",
        }
    }
}

/// Which groups of runtime helpers the compiler should emit.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Basic syscall wrappers: `_dev_write`, `_dev_read`, `_dev_exit`,
    /// `_dev_puts`, `_dev_putchar`, `_dev_getchar`, `_dev_rand`, fences.
    pub syscalls: bool,
    /// Garbage-collected heap helpers: `_dev_gc_collect`, `_dev_gc_leak_check`,
    /// plus the stats/capacity functions.  Implies `alloc = true`.
    pub gc: bool,
    /// Raw allocator: `_dev_alloc`, `_dev_free`.  On x86_64 this is the
    /// GC free-list; on x86_32 it is a bump allocator.
    pub alloc: bool,
    /// Emit XMM-based float support.  When disabled, float operations will
    /// fail at codegen time.
    pub float: bool,
    /// Socket helpers: `_dev_socket`, `_dev_bind`, `_dev_listen`,
    /// `_dev_accept`, `_dev_close`.
    pub sockets: bool,
    /// Filesystem helpers: `_dev_open`, `_dev_lseek`, `_dev_unlink`,
    /// `_dev_fork`, `_dev_fcntl`, `_dev_setsockopt`.
    pub files: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            syscalls: true,
            gc: false,
            alloc: false,
            float: true,
            sockets: false,
            files: false,
        }
    }
}

/// A named, addressable memory region.
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub name: String,
    /// Read permission.
    pub read: bool,
    /// Write permission.
    pub write: bool,
    /// Execute permission.
    pub exec: bool,
    /// Start address.
    pub origin: u64,
    /// Length in bytes.
    pub length: u64,
}

/// Maps a binary section (`.text`, `.rodata`, …) to a memory region.
#[derive(Debug, Clone)]
pub struct SectionMapping {
    pub section: String,
    pub region: String,
}

/// Full target description.  Drives codegen backend selection, runtime
/// emission, object-file layout, and entry-point handling.
#[derive(Debug, Clone)]
pub struct LinkerConfig {
    /// Codegen backend: `"x86_64"`, `"x86_32"`, or `"x86_16"`.
    pub arch: String,
    /// Output format (ELF, ELF32, flat binary, raw).
    pub format: OutputFormat,
    /// Whether to emit a hosted `_start` that calls `main()`.
    pub hosted: bool,
    /// Entry-point symbol name.  Overridable per-project with `@entry`.
    pub entry: String,
    /// Base virtual address for the ELF image / origin for flat binaries.
    pub base_address: u64,
    /// Heap size in bytes.  `0` means no heap.
    pub heap_size: u64,
    /// Named memory regions.
    pub regions: Vec<MemoryRegion>,
    /// Section-to-region mappings.
    pub sections: Vec<SectionMapping>,
    /// Runtime helper selection.
    pub runtime: RuntimeConfig,
}

impl LinkerConfig {
    /// Validate internal consistency.  Returns `Ok(())` or an explanatory error.
    pub fn validate(&self) -> Result<()> {
        if self.runtime.gc && !self.runtime.alloc {
            anyhow::bail!("RUNTIME gc=true requires alloc=true");
        }
        if self.runtime.alloc && self.heap_size == 0 {
            anyhow::bail!("RUNTIME alloc=true requires HEAP size > 0");
        }
        if self.runtime.gc && self.heap_size == 0 {
            anyhow::bail!("RUNTIME gc=true requires HEAP size > 0");
        }
        if matches!(self.format, OutputFormat::Flat | OutputFormat::Raw) && self.hosted {
            anyhow::bail!("FORMAT {} cannot be hosted", self.format.as_str());
        }
        if self.hosted && self.entry.is_empty() {
            anyhow::bail!("hosted=true requires ENTRY to be set");
        }
        Ok(())
    }

    /// Format as the `obj_format` string the rest of the pipeline expects.
    pub fn obj_format_str(&self) -> &str {
        self.format.as_str()
    }
}

/// Built-in target: x86_64 Linux hosted ELF64.
pub fn builtin_x86_64_linux() -> LinkerConfig {
    LinkerConfig {
        arch: "x86_64".to_string(),
        format: OutputFormat::Elf,
        hosted: true,
        entry: "_forge_main".to_string(),
        base_address: 0x400000,
        heap_size: 4 * 1024 * 1024,
        regions: vec![MemoryRegion {
            name: "ram".to_string(),
            read: true,
            write: true,
            exec: true,
            origin: 0x400000,
            length: 512 * 1024 * 1024,
        }],
        sections: vec![
            SectionMapping { section: ".text".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".rodata".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".data".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".bss".to_string(), region: "ram".to_string() },
        ],
        runtime: RuntimeConfig {
            syscalls: true,
            gc: false,
            alloc: false,
            float: true,
            sockets: false,
            files: false,
        },
    }
}

/// Built-in target: x86_32 Linux hosted ELF32.
pub fn builtin_x86_32_linux() -> LinkerConfig {
    LinkerConfig {
        arch: "x86_32".to_string(),
        format: OutputFormat::Elf32,
        hosted: true,
        entry: "_forge_main".to_string(),
        base_address: 0x08048000,
        heap_size: 0,
        regions: vec![MemoryRegion {
            name: "ram".to_string(),
            read: true,
            write: true,
            exec: true,
            origin: 0x08048000,
            length: 512 * 1024 * 1024,
        }],
        sections: vec![
            SectionMapping { section: ".text".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".rodata".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".data".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".bss".to_string(), region: "ram".to_string() },
        ],
        runtime: RuntimeConfig {
            syscalls: true,
            gc: false,
            alloc: false,
            float: false,
            sockets: false,
            files: false,
        },
    }
}

/// Built-in target: x86_16 real-mode boot sector.
pub fn builtin_x86_16_boot() -> LinkerConfig {
    LinkerConfig {
        arch: "x86_16".to_string(),
        format: OutputFormat::Flat,
        hosted: false,
        entry: "_start".to_string(),
        base_address: 0x7C00,
        heap_size: 0,
        regions: vec![MemoryRegion {
            name: "ram".to_string(),
            read: true,
            write: true,
            exec: true,
            origin: 0x0000,
            length: 64 * 1024,
        }],
        sections: vec![
            SectionMapping { section: ".text".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".rodata".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".data".to_string(), region: "ram".to_string() },
            SectionMapping { section: ".bss".to_string(), region: "ram".to_string() },
        ],
        runtime: RuntimeConfig {
            syscalls: false,
            gc: false,
            alloc: false,
            float: false,
            sockets: false,
            files: false,
        },
    }
}

/// Look up a built-in target by name.  Returns `None` if the name is unknown.
pub fn builtin_target(name: &str) -> Option<LinkerConfig> {
    match name {
        "x86_64-unknown-linux-gnu" | "native" => Some(builtin_x86_64_linux()),
        "x86_32-unknown-linux-gnu" => Some(builtin_x86_32_linux()),
        "x86_16-boot" => Some(builtin_x86_16_boot()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_x86_64_is_hosted_elf() {
        let c = builtin_x86_64_linux();
        assert!(c.hosted);
        assert_eq!(c.arch, "x86_64");
        assert_eq!(c.format, OutputFormat::Elf);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn builtin_x86_16_is_freestanding_flat() {
        let c = builtin_x86_16_boot();
        assert!(!c.hosted);
        assert_eq!(c.format, OutputFormat::Flat);
        assert_eq!(c.base_address, 0x7C00);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn gc_requires_alloc() {
        let mut c = builtin_x86_64_linux();
        c.runtime.gc = true;
        c.runtime.alloc = false;
        assert!(c.validate().is_err());
    }

    #[test]
    fn alloc_requires_heap() {
        let mut c = builtin_x86_64_linux();
        c.runtime.alloc = true;
        c.heap_size = 0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn flat_cannot_be_hosted() {
        let mut c = builtin_x86_16_boot();
        c.hosted = true;
        assert!(c.validate().is_err());
    }
}

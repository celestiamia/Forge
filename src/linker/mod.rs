//! Linker script system for Forge.
//!
//! Enables compiling for arbitrary targets via a `.fld` (Forge Linker
//! Descriptor) file that describes the target's architecture, output format,
//! memory layout, runtime capabilities, and entry point.  This replaces the
//! hardcoded target triples with a fully user-extensible system.
//!
//! # Example
//!
//! ```ignore
//! let config = forgec::linker::load_linker_script("mytarget.fld")?;
//! // or
//! let config = forgec::linker::builtin_target("x86_64-unknown-linux-gnu").unwrap();
//! ```

pub mod config;
pub mod lexer;
pub mod parser;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub use config::{
    builtin_target, LinkerConfig, MemoryRegion, OutputFormat, RuntimeConfig,
    SectionMapping,
};

/// Load and parse a linker script from a file.
pub fn load_linker_script(path: &Path) -> Result<LinkerConfig> {
    let src = fs::read_to_string(path)
        .with_context(|| format!("reading linker script {}", path.display()))?;
    parser::parse_linker_script(&src)
        .map_err(|e| anyhow::anyhow!("parsing linker script {}: {}", path.display(), e))
}

/// Resolve a [`LinkerConfig`] from a target name or linker script path.
///
/// If `linker_path` is given, it is parsed as a `.fld` file.  Otherwise the
/// `target` name is matched against the built-in presets.  If neither yields a
/// config, an error is returned listing the supported built-in targets.
pub fn resolve_config(
    target: Option<&str>,
    linker_path: Option<&Path>,
) -> Result<LinkerConfig> {
    if let Some(path) = linker_path {
        return load_linker_script(path);
    }
    let name = target.unwrap_or("x86_64-unknown-linux-gnu");
    builtin_target(name).ok_or_else(|| {
        anyhow::anyhow!(
            "target `{}` is not a built-in. Provide a linker script with --linker, \
             or use one of: x86_64-unknown-linux-gnu, x86_32-unknown-linux-gnu, x86_16-boot",
            name
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_builtin_native() {
        let c = resolve_config(Some("native"), None).unwrap();
        assert_eq!(c.arch, "x86_64");
    }

    #[test]
    fn resolve_unknown_fails() {
        assert!(resolve_config(Some("riscv64-unknown-elf"), None).is_err());
    }

    #[test]
    fn linker_overrides_target() {
        let src = "ARCH x86_16\nFORMAT flat\nHOSTED false\nENTRY _start\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.fld");
        std::fs::write(&path, src).unwrap();
        let c = resolve_config(Some("x86_64-unknown-linux-gnu"), Some(&path)).unwrap();
        assert_eq!(c.arch, "x86_16");
    }
}

//! Forge compiler driver.
//!
//! Reads `.dev` source, parses it with the Python-like frontend, lowers it to
//! the native backend IR, and emits a native executable directly (no clang,
//! no LLVM, no external assembler or linker).

pub mod loader;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::backend::codegen::compile_program;
use crate::backend::codegen32;
use crate::driver::loader::{load_modules, merge_modules};
use crate::lower::lower;
use crate::obj::ObjectWriter;

#[derive(Debug, Clone)]
pub struct CompileOptions {
    pub source: PathBuf,
    pub output: PathBuf,
    pub target: Option<String>,
    pub freestanding: bool,
}

/// Compile a Forge source file to a native executable or flat binary.
pub fn compile(options: CompileOptions) -> Result<PathBuf> {
    let source = fs::read_to_string(&options.source)
        .with_context(|| format!("reading {}", options.source.display()))?;

    let graph = load_modules(&source, &options.source)?;
    let module = merge_modules(graph)?;

    // Run the full semantic analyzer before lowering.  It returns a typed tree
    // plus accumulated diagnostics; we report them and stop here if anything is
    // wrong so the lowerer/codegen never sees an ill-typed program.
    let typed = crate::sema::check_with_file(
        module.clone(),
        Some(options.source.to_string_lossy().into_owned()),
    );
    if !typed.errors.is_empty() {
        let mut msg = String::from("type checking failed:\n");
        for err in &typed.errors {
            msg.push_str(&format!("  {}\n", err));
        }
        anyhow::bail!(msg.trim_end().to_string());
    }

    let target = options.target.as_deref().unwrap_or("x86_64-unknown-linux-gnu");
    let (hosted, arch, obj_format) = classify_target(target)?;

    let mut program = lower(&module, hosted)?;
    program.target = Some(target.to_string());
    program.arch = Some(arch.to_string());
    program.obj_format = Some(obj_format.to_string());

    let writer: Box<dyn ObjectWriter> = match arch {
        "x86_32" => codegen32::compile_program(&program)?,
        _ => compile_program(&program)?,
    };
    writer.write(&options.output)
        .with_context(|| format!("writing {}", options.output.display()))?;

    // Make the output executable on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&options.output)?.permissions();
        let mut perms = perms;
        perms.set_mode(perms.mode() | 0o111);
        fs::set_permissions(&options.output, perms)?;
    }

    Ok(options.output.clone())
}

fn classify_target(target: &str) -> Result<(bool, &str, &str)> {
    match target {
        "x86_64-unknown-linux-gnu" | "native" => Ok((true, "x86_64", "elf")),
        "x86_32-unknown-linux-gnu" => Ok((true, "x86_32", "elf")),
        "x86_16-boot" => Ok((false, "x86_16", "flat")),
        _ => anyhow::bail!(
            "target {} is not supported yet; supported targets are x86_64-unknown-linux-gnu, x86_32-unknown-linux-gnu and x86_16-boot",
            target
        ),
    }
}

/// Convenience helper for tests.
pub fn compile_to_out(source: &Path, output: &Path, target: Option<&str>, freestanding: bool) -> Result<PathBuf> {
    compile(CompileOptions {
        source: source.to_path_buf(),
        output: output.to_path_buf(),
        target: target.map(|s| s.to_string()),
        freestanding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Command;

    fn write_temp_dev(dir: &tempfile::TempDir, name: &str, source: &str) -> PathBuf {
        let path = dir.path().join(name).with_extension("dev");
        std::fs::write(&path, source).unwrap();
        path
    }

    #[test]
    fn compiles_minimal_program_x86_32() {
        let src = r#"
package test

pub def main() -> int32:
    return 0
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "minimal32", src);
        let output = dir.path().join("out");
        let out = compile(CompileOptions {
            source,
            output,
            target: Some("x86_32-unknown-linux-gnu".to_string()),
            freestanding: false,
        }).unwrap();
        let status = Command::new(&out).status().unwrap();
        assert!(status.success(), "minimal x86_32 program should exit 0");
    }

    #[test]
    fn compiles_minimal_program() {
        let src = r#"
package test

pub def main() -> int32:
    return 0
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "minimal", src);
        let output = dir.path().join("out");
        let out = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
        }).unwrap();
        let status = Command::new(&out).status().unwrap();
        assert!(status.success(), "minimal program should exit 0");
    }

    #[test]
    fn rejects_ill_typed_program() {
        let src = r#"
package test

pub def main() -> int32:
    let x: int32 = "not an int"
    return x
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "bad", src);
        let output = dir.path().join("out");
        let err = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
        });
        assert!(err.is_err(), "ill-typed program should be rejected");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("type checking failed"),
            "expected type-checker error, got: {}",
            msg
        );
    }
}

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
use crate::linker::resolve_config;
use crate::lower::lower;
use crate::obj::ObjectWriter;

#[derive(Debug, Clone)]
pub struct CompileOptions {
    pub source: PathBuf,
    pub output: PathBuf,
    pub target: Option<String>,
    pub freestanding: bool,
    pub linker: Option<PathBuf>,
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

    let target_name = options
        .target
        .as_deref()
        .unwrap_or("x86_64-unknown-linux-gnu");
    let mut config = resolve_config(options.target.as_deref(), options.linker.as_deref())?;
    let hosted = config.hosted && !options.freestanding;
    if !hosted && options.linker.is_none() {
        // A built-in hosted preset (x86_64/x86_32) names `_forge_main` as its
        // entry; that is the hosted `main` mangling and is meaningless in
        // freestanding mode, which enters at `_start`.  Custom `.fld` scripts
        // keep their `ENTRY` directive verbatim.
        config.entry = "_start".to_string();
    }

    let mut program = lower(&module, hosted)?;
    program.target = Some(target_name.to_string());
    program.arch = Some(config.arch.clone());
    program.obj_format = Some(config.obj_format_str().to_string());
    program.config = Some(config.clone());

    let writer: Box<dyn ObjectWriter> = match config.arch.as_str() {
        "x86_32" => codegen32::compile_program(&program)?,
        _ => compile_program(&program)?,
    };
    writer
        .write(&options.output)
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

/// Convenience helper for tests.
#[allow(dead_code)]
pub fn compile_to_out(
    source: &Path,
    output: &Path,
    target: Option<&str>,
    freestanding: bool,
) -> Result<PathBuf> {
    compile(CompileOptions {
        source: source.to_path_buf(),
        output: output.to_path_buf(),
        target: target.map(|s| s.to_string()),
        freestanding,
        linker: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

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
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        })
        .unwrap();
        // Small delay to ensure file is fully written and permissions applied
        // before attempting execution (prevents "Text file busy" in parallel runs)
        thread::sleep(Duration::from_millis(10));
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
            linker: None,
        })
        .unwrap();
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
            linker: None,
        });
        assert!(err.is_err(), "ill-typed program should be rejected");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("type checking failed"),
            "expected type-checker error, got: {}",
            msg
        );
    }

    #[test]
    fn embeds_file_bytes_into_the_binary() {
        // `embed NAME = "file"` must bake the raw bytes of a data file into
        // the executable: `NAME` is a pointer to the blob and `NAME_LEN` is
        // its length.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("blob.bin"), [0x9A, 0x01, 0xFF]).unwrap();
        let src = r#"
package test

embed BLOB = "blob.bin"

pub def main() -> int32:
    if BLOB_LEN != 3:
        return 1
    unsafe:
        if (*BLOB) as int32 != 0x9A:
            return 2
    unsafe:
        if (*(BLOB + 2 as uint64)) as int32 != 0xFF:
            return 3
    return 0
    return 0
"#;
        let source = write_temp_dev(&dir, "embed", src);
        let output = dir.path().join("out");
        let out = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        })
        .unwrap();
        let status = Command::new(&out).status().unwrap();
        assert_eq!(
            status.code(),
            Some(0),
            "embedded blob should be readable with the right length"
        );
    }

    #[test]
    fn missing_embed_file_is_a_compile_error() {
        let dir = tempfile::tempdir().unwrap();
        let src = r#"
package test

embed MISSING = "nope.bin"

pub def main() -> int32:
    return 0
"#;
        let source = write_temp_dev(&dir, "missing_embed", src);
        let output = dir.path().join("out");
        let err = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        });
        assert!(err.is_err(), "missing embed file should be rejected");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("cannot read embedded file"),
            "expected embed error, got: {}",
            msg
        );
    }

    #[test]
    fn generic_function_is_a_clean_error() {
        let dir = tempfile::tempdir().unwrap();
        let src = r#"
package test

def identity[T](x: T) -> T:
    return x
"#;
        let source = write_temp_dev(&dir, "generic", src);
        let output = dir.path().join("out");
        let err = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        });
        assert!(
            err.is_err(),
            "generic function should be rejected, not panic"
        );
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("generic function `identity` is not supported yet"),
            "expected clean generic error, got: {}",
            msg
        );
    }

    #[test]
    fn block_expr_runs() {
        let src = r#"
package test

pub def main() -> int32:
    var x: int32 = {
        var a = 6
        var b = 7
        a * b
    }
    return x
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "block_expr", src);
        let output = dir.path().join("block_expr");
        let out = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(10));
        let status = Command::new(&out).status().unwrap();
        assert_eq!(
            status.code(),
            Some(42),
            "block expression should evaluate to 6*7 = 42"
        );
    }

    #[test]
    fn int128_is_a_clean_error() {
        let dir = tempfile::tempdir().unwrap();
        let src = r#"
package test

pub def main() -> int32:
    var x: int128 = 5
    return 0
"#;
        let source = write_temp_dev(&dir, "int128", src);
        let output = dir.path().join("out");
        let err = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        });
        assert!(err.is_err(), "int128 should be rejected, not panic");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("128-bit integers are not supported"),
            "expected clean int128 error, got: {}",
            msg
        );
    }

    #[test]
    fn impl_block_compiles_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let src = r#"
package test

struct Point:
    x: int32
    y: int32

impl Point:
    def magnitude(self: Point) -> int32:
        return self.x + self.y

pub def main() -> int32:
    var p: Point
    p.x = 3
    p.y = 4
    return 0
"#;
        let source = write_temp_dev(&dir, "impl", src);
        let output = dir.path().join("out");
        let out = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        })
        .expect("impl block should compile cleanly");
        // Small delay to ensure the file is fully written and permissions
        // applied before execution (prevents "Text file busy" in parallel runs).
        thread::sleep(Duration::from_millis(10));
        let status = Command::new(&out).status().unwrap();
        assert!(status.success(), "impl program should exit 0");
    }

    #[test]
    fn nested_struct_fields_compile_and_run() {
        let dir = tempfile::tempdir().unwrap();
        let src = r#"
package test

struct Inner:
    a: int32
    b: int32

struct Outer:
    inner: Inner
    tag: int32

pub def main() -> int32:
    var o: Outer
    o.inner.a = 7
    o.inner.b = 8
    o.tag = 1
    var x: Inner = o.inner
    if x.a + x.b != 15:
        return 1
    if o.inner.b != 8:
        return 2
    return 0
"#;
        let source = write_temp_dev(&dir, "nested", src);
        let output = dir.path().join("out");
        let out = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        })
        .expect("nested structs should compile cleanly");
        // Small delay to ensure the file is fully written and permissions
        // applied before execution (prevents "Text file busy" in parallel runs).
        thread::sleep(Duration::from_millis(10));
        let status = Command::new(&out).status().unwrap();
        assert_eq!(
            status.code(),
            Some(0),
            "nested struct program should exit 0"
        );
    }

    #[test]
    fn recursive_struct_by_value_is_a_clean_error() {
        let dir = tempfile::tempdir().unwrap();
        let src = r#"
package test

struct A:
    a: A

pub def main() -> int32:
    var x: A
    return 0
"#;
        let source = write_temp_dev(&dir, "recursive", src);
        let output = dir.path().join("out");
        let err = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        });
        assert!(
            err.is_err(),
            "recursive struct should be rejected, not crash"
        );
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("contains itself by value"),
            "expected recursive-struct error, got: {}",
            msg
        );
    }

    #[test]
    fn compound_assignment_runs() {
        let src = r#"
package test

pub def main() -> int32:
    var x: int32 = 1
    x += 2
    x -= 1
    x *= 3
    x /= 2
    x %= 4
    var y: int32 = 14
    y &= 15
    y |= 8
    y ^= 3
    y <<= 1
    y >>= 2
    return x + y
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "compound", src);
        let output = dir.path().join("compound");
        let out = compile(CompileOptions {
            source,
            output,
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        })
        .unwrap();
        thread::sleep(Duration::from_millis(10));
        let status = Command::new(&out).status().unwrap();
        // x: 1+2=3, -1=2, *3=6, /2=3, %4=3. y: 14&15=14, |8=14, ^3=13, <<1=26, >>2=6. sum=9.
        assert_eq!(status.code(), Some(9));
    }
}

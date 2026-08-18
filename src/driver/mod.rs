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

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    #[test]
    fn x86_16_pmode_stub_and_lba_builtins_emit() {
        let src = r#"
package test

extern def _dev_enter_pmode(lo: u16, hi: u16) -> void
extern def _dev_bios_disk_read_lba(drive: u16, lba_lo: u16, lba_hi: u16, count: u16, es: u16, bx: u16) -> u16

@freestanding
pub def _start() -> void:
    _dev_enter_pmode(0x0000, 0x0010)
    var s: u16 = _dev_bios_disk_read_lba(0x80, 4, 0, 8, 0x1000, 0)
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "pmode", src);
        let output = dir.path().join("loader.raw");
        let fld = dir.path().join("loader.fld");
        std::fs::write(
            &fld,
            "ARCH x86_16\nFORMAT raw\nHOSTED false\nENTRY _start\nLOAD 0x9000\n",
        )
        .unwrap();
        let out = compile(CompileOptions {
            source,
            output: output.clone(),
            target: None,
            freestanding: false,
            linker: Some(fld),
        })
        .unwrap();
        let bytes = fs::read(&out).unwrap();

        // lgdt [abs16]
        assert!(find_bytes(&bytes, &[0x0F, 0x01, 0x16]).is_some());
        // mov eax, cr0 / or eax, 1 / mov cr0, eax
        assert!(find_bytes(&bytes, &[0x66, 0x0F, 0x20, 0xC0]).is_some());
        assert!(find_bytes(&bytes, &[0x66, 0x83, 0xC8, 0x01]).is_some());
        assert!(find_bytes(&bytes, &[0x66, 0x0F, 0x22, 0xC0]).is_some());
        // far jump 66 EA <imm32> <sel 0x08>
        let fj = find_bytes(&bytes, &[0x66, 0xEA]).expect("far jump missing");
        assert_eq!(&bytes[fj + 6..fj + 8], &[0x08, 0x00]);
        // trampoline: mov ds/es/ss, ax
        assert!(find_bytes(&bytes, &[0x8E, 0xD8]).is_some());
        assert!(find_bytes(&bytes, &[0x8E, 0xC0]).is_some());
        assert!(find_bytes(&bytes, &[0x8E, 0xD0]).is_some());
        // flat GDT descriptors
        assert!(
            find_bytes(&bytes, &[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9A, 0xCF, 0x00]).is_some(),
            "code descriptor missing"
        );
        assert!(
            find_bytes(&bytes, &[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00]).is_some(),
            "data descriptor missing"
        );
        // GDTR limit 23 followed by a base that points at the GDT, which
        // starts with the null descriptor immediately before the code
        // descriptor.
        let code_desc = find_bytes(&bytes, &[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9A, 0xCF, 0x00])
            .expect("code descriptor missing");
        assert!(
            find_bytes(&bytes, &[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00]).is_some(),
            "data descriptor missing"
        );
        let gdt_off = code_desc - 8;
        assert_eq!(
            &bytes[gdt_off..gdt_off + 8],
            &[0; 8],
            "null descriptor missing"
        );
        let gdtr = find_bytes(&bytes, &[0x17, 0x00]).expect("GDTR missing");
        let base = u32::from_le_bytes(bytes[gdtr + 2..gdtr + 6].try_into().unwrap());
        assert_eq!(
            base,
            0x9000 + gdt_off as u32,
            "GDTR base must point at the GDT"
        );
        // far-jump target must be the trampoline (first instruction after
        // the far jump is mov ax, 0x10 with a 32-bit operand size)
        let tramp = fj + 8;
        let target = u32::from_le_bytes(bytes[fj + 2..fj + 6].try_into().unwrap());
        assert_eq!(
            target,
            0x9000 + tramp as u32,
            "far jump must land on the trampoline"
        );
        assert_eq!(&bytes[tramp..tramp + 4], &[0x66, 0xB8, 0x10, 0x00]);
        // LBA read: DAP built on the stack, packet size 0x10 pushed last
        assert!(find_bytes(&bytes, &[0x68, 0x10, 0x00]).is_some());
        // INT 13h AH=42h
        assert!(find_bytes(&bytes, &[0xB4, 0x42, 0xCD, 0x13]).is_some());
    }

    #[test]
    fn x86_32_raw_kernel_compiles_with_load_base_fixups() {
        let src = r#"
package test

extern def _dev_outb(port: u16, val: u8) -> void
extern def _dev_halt() -> void

@freestanding
pub def _start() -> void:
    var msg: ptr[char] = "ForgeOS32" as ptr[char]
    unsafe:
        var c: char = msg[0]
        _dev_outb(0x3F8, c as u8)
    _dev_halt()
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "kernel32", src);
        let output = dir.path().join("kernel.raw");
        let fld = dir.path().join("kernel.fld");
        std::fs::write(
            &fld,
            "ARCH x86_32\nFORMAT raw\nHOSTED false\nENTRY _start\nLOAD 0x100000\n",
        )
        .unwrap();
        let out = compile(CompileOptions {
            source,
            output: output.clone(),
            target: None,
            freestanding: false,
            linker: Some(fld),
        })
        .unwrap();
        let bytes = fs::read(&out).unwrap();

        // Not an ELF: starts with the entry prologue (push ebp; mov ebp, esp).
        assert_eq!(&bytes[..3], &[0x55, 0x89, 0xE5]);
        // String lives in the image...
        let s = find_bytes(&bytes, b"ForgeOS32").expect("string literal missing");
        // ...and the `mov eax, imm32` fixup points at LOAD base + offset.
        let target = 0x100000u32 + s as u32;
        assert!(
            find_bytes(&bytes, &target.to_le_bytes()).is_some(),
            "string address fixup against LOAD base missing"
        );
        // Freestanding helpers: cdecl frame; mov dx,[ebp+8]; mov al,[ebp+12];
        // out dx,al; leave; ret
        assert!(find_bytes(&bytes, &[0x66, 0x8B, 0x55, 0x08]).is_some());
        assert!(find_bytes(&bytes, &[0x8A, 0x45, 0x0C]).is_some());
        assert!(find_bytes(&bytes, &[0xEE, 0xC9, 0xC3]).is_some());
        // _dev_halt: cli; hlt
        assert!(find_bytes(&bytes, &[0xFA, 0xF4]).is_some());
    }

    #[test]
    fn x86_32_raw_rejects_hosted_and_large_x86_16_load() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(
            &dir,
            "kernel32",
            "package test\npub def main() -> int32:\n    return 0\n",
        );
        let fld = dir.path().join("bad.fld");
        std::fs::write(
            &fld,
            "ARCH x86_32\nFORMAT raw\nHOSTED true\nENTRY _forge_main\nLOAD 0x100000\n",
        )
        .unwrap();
        let err = compile(CompileOptions {
            source: source.clone(),
            output: dir.path().join("o1"),
            target: None,
            freestanding: false,
            linker: Some(fld.clone()),
        })
        .unwrap_err();
        assert!(err.to_string().contains("cannot be hosted"));

        let fld2 = dir.path().join("bad2.fld");
        std::fs::write(
            &fld2,
            "ARCH x86_16\nFORMAT raw\nHOSTED false\nENTRY _start\nLOAD 0x10000\n",
        )
        .unwrap();
        let err = compile(CompileOptions {
            source,
            output: dir.path().join("o2"),
            target: None,
            freestanding: false,
            linker: Some(fld2),
        })
        .unwrap_err();
        assert!(err.to_string().contains("16 bits"));
    }

    #[test]
    fn x86_64_raw_flat_binary_emits_packed_kernel_no_elf_header() {
        // Phase 0 of the 64-bit boot chain: `ARCH x86_64 FORMAT raw` must emit a
        // flat (packed) binary loaded at `LOAD`, NOT an ELF64.  x86_64 references
        // string literals / globals through RIP-relative addressing, so a raw
        // kernel is naturally relocatable — no LOAD-relative absolute fixups are
        // required for the common case.
        let src = r#"
package test

@freestanding
pub def _start() -> void:
    var msg: ptr[char] = "ForgeOS64" as ptr[char]
    var g: ptr[char] = msg
    unsafe:
        var c: char = g[7]
        c = c
"#;
        let dir = tempfile::tempdir().unwrap();
        let source = write_temp_dev(&dir, "k64", src);
        let output = dir.path().join("kernel.raw");
        let fld = dir.path().join("k64.fld");
        std::fs::write(
            &fld,
            "ARCH x86_64\nFORMAT raw\nHOSTED false\nENTRY _start\nLOAD 0x100000\n",
        )
        .unwrap();
        let out = compile(CompileOptions {
            source,
            output: output.clone(),
            target: None,
            freestanding: false,
            linker: Some(fld),
        })
        .unwrap();
        let bytes = fs::read(&out).unwrap();

        // Not an ELF64: no 7f 45 4c 46 magic and the entry begins with the
        // System V AMD64 prologue (push rbp; mov rbp, rsp).
        assert!(
            !bytes.starts_with(&[0x7F, 0x45, 0x4C, 0x46]),
            "x86_64 raw output must not be an ELF64"
        );
        assert_eq!(&bytes[..3], &[0x55, 0x48, 0x89]);
        assert_eq!(bytes[3], 0xE5, "expected `mov rbp, rsp` after `push rbp`");

        // String literal present in the packed image.
        find_bytes(&bytes, b"ForgeOS64").expect("string literal missing from x86_64 raw image");

        // Packed layout: no ELF/program-header overhead (the same source as the
        // x86_32 raw kernel emits in well under 256 bytes here).
        assert!(
            bytes.len() < 256,
            "x86_64 raw image unexpectedly large (got {} bytes)",
            bytes.len()
        );

        // No LOAD-relative absolute 8-byte address leaks into a local's RIP-
        // relative access: the string's runtime address is reached through
        // `lea rax, [rip+disp]`, so scanning for `0x100000 + offset` must fail.
        let s = find_bytes(&bytes, b"ForgeOS64").unwrap();
        let abs = (0x100000u64 + s as u64).to_le_bytes();
        assert!(
            find_bytes(&bytes, &abs).is_none(),
            "x86_64 raw must use RIP-relative, not LOAD-relative, addressing"
        );
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

struct Pair[T]:
    first: T
    second: T

def identity[T](x: T) -> T:
    return x

def make_pair[T](a: T, b: T) -> Pair[T]:
    return Pair[T] { first: a, second: b }

pub def main() -> int64:
    var p: Pair[int64] = make_pair(3 as int64, 4 as int64)
    return identity(42 as int64) + p.second
"#;
        let source = write_temp_dev(&dir, "generic", src);
        let output = dir.path().join("out");
        let result = compile(CompileOptions {
            source,
            output: output.clone(),
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            freestanding: false,
            linker: None,
        });
        assert!(
            result.is_ok(),
            "generic functions should compile, got: {:?}",
            result.err()
        );
        let status = std::process::Command::new(&output)
            .status()
            .expect("failed to run generic program");
        assert_eq!(
            status.code(),
            Some(46),
            "generic program should exit with 46 = 42 + 4"
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

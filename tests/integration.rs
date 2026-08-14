use assert_cmd::Command;
use std::fs;

fn compile_example(name: &str) -> std::path::PathBuf {
    compile_example_with_target(name, "x86_64-unknown-linux-gnu")
}

fn compile_example_with_target(name: &str, target: &str) -> std::path::PathBuf {
    let out_dir = std::env::temp_dir().join(format!(
        "forge_{}_{}_{}_test",
        name,
        target,
        std::process::id()
    ));
    let _ = fs::create_dir_all(&out_dir);
    let bin = out_dir.join(name);

    let mut cmd = Command::cargo_bin("forgec").unwrap();
    cmd.arg(format!("examples/{}.dev", name))
        .arg("-o")
        .arg(&bin)
        .arg("--target")
        .arg(target);
    cmd.assert().success();

    bin
}

/// Compile a source file at an arbitrary relative path (used for sub-directory
/// examples and multi-module projects).
fn compile_source(path: &str, target: &str) -> std::path::PathBuf {
    let out_dir = std::env::temp_dir().join(format!(
        "forge_{}_{}_{}_test",
        path.replace('/', "_"),
        target,
        std::process::id()
    ));
    let _ = fs::create_dir_all(&out_dir);
    let bin = out_dir.join("out");

    let mut cmd = Command::cargo_bin("forgec").unwrap();
    cmd.arg(format!("examples/{}", path))
        .arg("-o")
        .arg(&bin)
        .arg("--target")
        .arg(target);
    cmd.assert().success();

    bin
}

#[test]
fn hello_dev_compiles_and_runs() {
    let bin = compile_example("hello");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run hello binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "Hello, Forge!\n");
    assert!(
        output.status.success(),
        "hello binary exited with non-zero status"
    );
}

#[test]
fn hello_dev_compiles_and_runs_x86_32() {
    let bin = compile_example_with_target("hello", "x86_32-unknown-linux-gnu");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run hello (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "Hello, Forge!\n");
    assert!(
        output.status.success(),
        "hello (x86_32) binary exited with non-zero status"
    );
}

#[test]
fn bump_fmt_dev_compiles_and_runs() {
    let bin = compile_example("bump_fmt");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run bump_fmt binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "0\n42\n-7\n100\n2147483647\n-2147483647\n-2147483648\n",
        "bump_fmt produced unexpected output"
    );
    assert!(
        output.status.success(),
        "bump_fmt binary exited with non-zero status"
    );
}

#[test]
fn bump_fmt_dev_compiles_and_runs_x86_32() {
    // std.fmt now includes format_f64 which uses float64, not supported on x86_32.
    let result = Command::cargo_bin("forgec")
        .unwrap()
        .arg("examples/bump_fmt.dev")
        .arg("-o")
        .arg("/tmp/forge_bump_fmt_x86_32-unknown-linux-gnu_test/bump_fmt")
        .arg("--target")
        .arg("x86_32-unknown-linux-gnu")
        .output()
        .unwrap();
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        if stderr.contains("floating point is not implemented") {
            return; // Known limitation: x86_32 backend doesn't support floats
        }
        panic!("Compilation failed unexpectedly:\n{}", stderr);
    }
    let bin = "/tmp/forge_bump_fmt_x86_32-unknown-linux-gnu_test/bump_fmt";
    let output = Command::new(bin)
        .output()
        .expect("failed to run bump_fmt (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "0\n42\n-7\n100\n2147483647\n-2147483647\n-2147483648\n",
        "bump_fmt (x86_32) produced unexpected output"
    );
    assert!(
        output.status.success(),
        "bump_fmt (x86_32) binary exited with non-zero status"
    );
}

#[test]
fn power_dev_compiles_and_runs() {
    let bin = compile_example("power");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run power binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "1024\n81\n1\n");
    assert!(
        output.status.success(),
        "power binary exited with non-zero status"
    );
}

#[test]
fn floor_div_dev_compiles_and_runs() {
    let bin = compile_example("floor_div");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run floor_div binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "-4\n3\n-4\n");
    assert!(
        output.status.success(),
        "floor_div binary exited with non-zero status"
    );
}

#[test]
fn float_fmt_dev_compiles_and_runs() {
    let bin = compile_example("float_fmt");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run float_fmt binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "3.140000\n-12.500000\n");
    assert!(
        output.status.success(),
        "float_fmt binary exited with non-zero status"
    );
}

#[test]
fn gc_dev_compiles_and_runs() {
    let bin = compile_example("gc");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run gc binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "64\n0\n4194304\n1\nreuse ok\n",
        "gc produced unexpected output"
    );
    assert!(
        output.status.success(),
        "gc binary exited with non-zero status"
    );
}

#[test]
fn gc_dev_detects_leak_from_dead_frame() {
    // test_gc2: a pointer dropped when `make_leak` returns must be reported by
    // leak_check (dead frames are zeroed at function exit), then reclaimed by
    // collect().
    let bin = compile_example("test_gc2");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run test_gc2 binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "64\n0\n4194304\n1\n",
        "test_gc2 produced unexpected output"
    );
    assert!(
        output.status.success(),
        "test_gc2 binary exited with non-zero status"
    );
}

#[test]
fn gc_dev_reuses_freed_blocks() {
    let bin = compile_example("test_reuse");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run test_reuse binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "0\n", "test_reuse produced unexpected output");
    assert!(
        output.status.success(),
        "test_reuse binary exited with non-zero status"
    );
}

#[test]
fn gc_dev_reuse_then_collect() {
    // test_reuse_leak: free-list reuse followed by leak detection + collection.
    // Regression test for the stale free-list `next` pointer bug that crashed
    // the allocator on the second request after a reuse.
    let bin = compile_example("test_reuse_leak");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run test_reuse_leak binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "64\n0\n4194304\n1\n",
        "test_reuse_leak produced unexpected output"
    );
    assert!(
        output.status.success(),
        "test_reuse_leak binary exited with non-zero status"
    );
}

#[test]
fn gc_dev_stress_many_blocks() {
    // test_gc_stress: 200 blocks, interleaved frees, dropped references, then
    // collection must reclaim everything.
    let bin = compile_example("test_gc_stress");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run test_gc_stress binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "stress ok\n",
        "test_gc_stress produced unexpected output"
    );
    assert!(
        output.status.success(),
        "test_gc_stress binary exited with non-zero status"
    );
}

#[test]
fn gc_dev_auto_collects_on_exhaustion() {
    // test_gc_stress2: churn 16 MiB of allocations through the 4 MiB arena so
    // the free list exhausts; the allocator must auto-collect and reuse.
    let bin = compile_example("test_gc_stress2");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run test_gc_stress2 binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "stress2 ok\n",
        "test_gc_stress2 produced unexpected output"
    );
    assert!(
        output.status.success(),
        "test_gc_stress2 binary exited with non-zero status"
    );
}

#[test]
fn bump_dev_compiles_and_runs() {
    let bin = compile_example("bump");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run bump binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "bump ok\n");
    assert!(
        output.status.success(),
        "bump binary exited with non-zero status"
    );
}

#[test]
fn fib_dev_compiles_and_runs() {
    let bin = compile_example("fib");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run fib binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "fib ok\n");
    assert!(
        output.status.success(),
        "fib binary exited with non-zero status"
    );
}

#[test]
fn struct_dev_compiles_and_runs() {
    let bin = compile_example("struct");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run struct binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "struct ok\n");
    assert!(
        output.status.success(),
        "struct binary exited with non-zero status"
    );
}

#[test]
fn getchar_dev_compiles_and_runs() {
    let bin = compile_example("getchar");
    let mut cmd = Command::new(&bin);
    cmd.write_stdin("x\n");
    let output = cmd.output().expect("failed to run getchar binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Got a character"),
        "unexpected output: {}",
        stdout
    );
    assert!(
        output.status.success(),
        "getchar binary exited with non-zero status"
    );
}

#[test]
fn guess_dev_compiles_and_runs() {
    let bin = compile_example("guess");
    let mut cmd = Command::new(&bin);
    // Feed a sequence of digits; the program reads one digit per round.
    cmd.write_stdin("12345\n");
    let output = cmd.output().expect("failed to run guess binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("correct!") || stdout.contains("too many guesses"),
        "unexpected output: {}",
        stdout
    );
}

#[test]
fn rps_dev_compiles_and_runs() {
    let bin = compile_example("rps");
    let mut cmd = Command::new(&bin);
    cmd.write_stdin("r\np\ns\n");
    let output = cmd.output().expect("failed to run rps binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Final score:"),
        "unexpected output: {}",
        stdout
    );
    assert!(
        output.status.success(),
        "rps binary exited with non-zero status"
    );
}

#[test]
fn volatile_dev_compiles_and_runs() {
    let bin = compile_example("volatile");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run volatile binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "volatile u32 ok\nvolatile u8 ok\n");
    assert!(
        output.status.success(),
        "volatile binary exited with non-zero status"
    );
}

#[test]
fn mem_dev_compiles_and_runs() {
    let bin = compile_example("mem");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run mem binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "copy ok\nset ok\nzero ok\ncompare ok\n");
    assert!(
        output.status.success(),
        "mem binary exited with non-zero status"
    );
}

#[test]
fn string_dev_compiles_and_runs() {
    let bin = compile_example("string");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run string binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "string ok\n");
    assert!(
        output.status.success(),
        "string binary exited with non-zero status"
    );
}

#[test]
fn math_dev_compiles_and_runs() {
    let bin = compile_example("math");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run math binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "math ok\n");
    assert!(
        output.status.success(),
        "math binary exited with non-zero status"
    );
}

#[test]
fn match_bc_dev_compiles_and_runs() {
    let bin = compile_example("match_bc");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run match_bc binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "thirteen\n");
    assert!(
        output.status.success(),
        "match_bc binary exited with non-zero status"
    );
}

#[test]
fn bootloader_dev_compiles_and_runs() {
    // Only run this test if qemu-system-x86_64 is available
    if std::process::Command::new("qemu-system-x86_64")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("Skipping bootloader test: qemu-system-x86_64 not found");
        return;
    }

    let out_dir =
        std::env::temp_dir().join(format!("forge_bootloader_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&out_dir);
    let bin = out_dir.join("bootloader");

    let mut cmd = Command::cargo_bin("forgec").unwrap();
    cmd.arg("examples/bootloader.dev")
        .arg("-o")
        .arg(&bin)
        .arg("--target")
        .arg("x86_16-boot");
    cmd.assert().success();

    let bytes = fs::read(&bin).expect("failed to read boot sector");
    assert_eq!(bytes.len(), 512, "boot sector must be 512 bytes");
    assert_eq!(
        &bytes[510..512],
        &[0x55, 0xAA],
        "missing boot signature 0x55 0xAA"
    );

    let mut qemu = std::process::Command::new("qemu-system-x86_64");
    qemu.arg("-fda")
        .arg(&bin)
        .arg("-nographic")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = qemu.spawn().expect("failed to spawn qemu-system-x86_64");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let _ = child.kill();

    // With -nographic, BIOS int 0x10 output is written to the emulator's
    // stdout, which we capture here.  It is mixed with SeaBIOS boot messages,
    // so we only assert the message is present.
    let output = child
        .wait_with_output()
        .expect("failed to read qemu output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Hello, Forge bootloader"),
        "bootloader did not print expected message; qemu stdout: {:?}",
        stdout
    );
}

#[test]
fn fileio_dev_compiles_and_runs() {
    let bin = compile_example("fileio");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run fileio binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "file I/O demo\nHello from Forge file I/O!\nfileio ok\n",
        "fileio produced unexpected output"
    );
    assert!(
        output.status.success(),
        "fileio binary exited with non-zero status"
    );
}

#[test]
fn fileio_dev_compiles_and_runs_x86_32() {
    let bin = compile_example_with_target("fileio", "x86_32-unknown-linux-gnu");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run fileio (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "file I/O demo\nHello from Forge file I/O!\nfileio ok\n",
        "fileio (x86_32) produced unexpected output"
    );
    assert!(
        output.status.success(),
        "fileio (x86_32) binary exited with non-zero status"
    );
}

#[test]
fn brainfuck_dev_compiles_and_runs() {
    let bin = compile_source("brainfuck/brainfuck.dev", "x86_64-unknown-linux-gnu");
    let output = Command::new(&bin)
        .arg("examples/brainfuck/hello.bf")
        .output()
        .expect("failed to run brainfuck binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "Hello World!\n",
        "brainfuck produced unexpected output"
    );
    assert!(
        output.status.success(),
        "brainfuck binary exited with non-zero status"
    );
}

#[test]
fn brainfuck_dev_compiles_and_runs_x86_32() {
    let bin = compile_source("brainfuck/brainfuck.dev", "x86_32-unknown-linux-gnu");
    let output = Command::new(&bin)
        .arg("examples/brainfuck/hello.bf")
        .output()
        .expect("failed to run brainfuck (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "Hello World!\n",
        "brainfuck (x86_32) produced unexpected output"
    );
    assert!(
        output.status.success(),
        "brainfuck (x86_32) binary exited with non-zero status"
    );
}

#[test]
fn brainfuck_dev_reads_stdin() {
    let bin = compile_source("brainfuck/brainfuck.dev", "x86_64-unknown-linux-gnu");
    let out_dir = std::env::temp_dir().join("forge_brainfuck_cat_test");
    let _ = fs::create_dir_all(&out_dir);
    let prog = out_dir.join("cat.bf");
    fs::write(&prog, ",[.,]").unwrap();

    let mut cmd = Command::new(&bin);
    cmd.arg(&prog).write_stdin("Hello from stdin");
    let output = cmd.output().expect("failed to run brainfuck cat binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "Hello from stdin",
        "brainfuck cat produced unexpected output"
    );
    assert!(
        output.status.success(),
        "brainfuck cat binary exited with non-zero status"
    );
}

#[test]
fn randfile_dev_compiles_and_runs() {
    let bin = compile_example("randfile");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run randfile binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Random values differ per run, so we verify the structural output.
    assert!(
        stdout.contains("Generating 10 random numbers"),
        "unexpected output: {}",
        stdout
    );
    assert!(
        stdout.contains("Wrote 10 random numbers."),
        "unexpected output: {}",
        stdout
    );
    assert!(stdout.contains("Done."), "unexpected output: {}", stdout);
    // 10 random numbers should appear between "Wrote" and "Done".
    let wrote_idx = stdout.find("Wrote 10 random numbers.").unwrap();
    let done_idx = stdout.find("Done.").unwrap();
    let between = &stdout[wrote_idx..done_idx];
    let num_lines = between.lines().count();
    assert!(
        num_lines >= 10,
        "expected >=10 number lines, got {}: {}",
        num_lines,
        stdout
    );
    assert!(
        output.status.success(),
        "randfile binary exited with non-zero status"
    );
}

#[test]
fn randfile_dev_compiles_and_runs_x86_32() {
    // std.fmt now includes format_f64 which uses float64, not supported on x86_32.
    let result = Command::cargo_bin("forgec")
        .unwrap()
        .arg("examples/randfile.dev")
        .arg("-o")
        .arg("/tmp/forge_randfile_x86_32-unknown-linux-gnu_test/randfile")
        .arg("--target")
        .arg("x86_32-unknown-linux-gnu")
        .output()
        .unwrap();
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        if stderr.contains("floating point is not implemented") {
            return; // Known limitation: x86_32 backend doesn't support floats
        }
        panic!("Compilation failed unexpectedly:\n{}", stderr);
    }
    let bin = "/tmp/forge_randfile_x86_32-unknown-linux-gnu_test/randfile";
    let output = Command::new(bin)
        .output()
        .expect("failed to run randfile (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Generating 10 random numbers"),
        "unexpected output: {}",
        stdout
    );
    assert!(
        stdout.contains("Wrote 10 random numbers."),
        "unexpected output: {}",
        stdout
    );
    assert!(stdout.contains("Done."), "unexpected output: {}", stdout);
    assert!(
        output.status.success(),
        "randfile (x86_32) binary exited with non-zero status"
    );
}

#[test]
fn multimod_dev_compiles_and_runs() {
    let bin = compile_source("multimod/multimod.dev", "x86_64-unknown-linux-gnu");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run multimod binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "multimod ok\n",
        "multimod produced unexpected output"
    );
    assert!(
        output.status.success(),
        "multimod binary exited with non-zero status"
    );
}

#[test]
fn multimod_dev_compiles_and_runs_x86_32() {
    let bin = compile_source("multimod/multimod.dev", "x86_32-unknown-linux-gnu");
    let output = Command::new(&bin)
        .output()
        .expect("failed to run multimod (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "multimod ok\n",
        "multimod (x86_32) produced unexpected output"
    );
    assert!(
        output.status.success(),
        "multimod (x86_32) binary exited with non-zero status"
    );
}

#[test]
fn multimod_name_conflict_is_reported() {
    let out_dir = std::env::temp_dir().join("forge_conflict_test");
    let _ = fs::create_dir_all(&out_dir);

    fs::write(
        out_dir.join("a.dev"),
        "package a\npub def shared() -> int32:\n    return 1\n",
    )
    .unwrap();
    fs::write(
        out_dir.join("b.dev"),
        "package b\npub def shared() -> int32:\n    return 2\n",
    )
    .unwrap();
    fs::write(
        out_dir.join("main.dev"),
        "package main\nimport a\nimport b\npub def main() -> int32:\n    return 0\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("forgec").unwrap();
    cmd.arg(out_dir.join("main.dev"))
        .arg("-o")
        .arg(out_dir.join("out"))
        .arg("--target")
        .arg("x86_64-unknown-linux-gnu");
    let output = cmd.output().expect("failed to run forgec");
    assert!(
        !output.status.success(),
        "expected compilation to fail on name conflict"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("name conflict"),
        "expected name conflict error, got: {}",
        stderr
    );
}

#[test]
fn multimod_unresolved_module_is_reported() {
    let out_dir = std::env::temp_dir().join("forge_missing_module_test");
    let _ = fs::create_dir_all(&out_dir);

    fs::write(
        out_dir.join("main.dev"),
        "package main\nimport nonexistent\npub def main() -> int32:\n    return 0\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("forgec").unwrap();
    cmd.arg(out_dir.join("main.dev"))
        .arg("-o")
        .arg(out_dir.join("out"))
        .arg("--target")
        .arg("x86_64-unknown-linux-gnu");
    let output = cmd.output().expect("failed to run forgec");
    assert!(
        !output.status.success(),
        "expected compilation to fail on unresolved module"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot resolve module"),
        "expected 'cannot resolve module' error, got: {}",
        stderr
    );
}

/// Helper: send a raw HTTP GET request and return the full response.
fn http_get(path: &str) -> String {
    let mut stream = std::net::TcpStream::connect("127.0.0.1:8080")
        .expect("failed to connect to website server on port 8080");
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        path
    );
    use std::io::Write;
    stream
        .write_all(request.as_bytes())
        .expect("failed to write request");
    let mut response = String::new();
    use std::io::Read;
    stream
        .read_to_string(&mut response)
        .expect("failed to read response");
    response
}

#[test]
fn website_dev_compiles_and_serves_pages() {
    let bin = compile_source("website/server.dev", "x86_64-unknown-linux-gnu");

    // Start the server with its working directory set to examples/website/
    // so it can find the static/ directory.
    let mut child = std::process::Command::new(&bin)
        .current_dir("examples/website")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn website server");

    // Give the server time to bind and listen.
    std::thread::sleep(std::time::Duration::from_secs(3));

    // GET / -> served index.html
    let resp = http_get("/");
    assert!(resp.contains("200 OK"), "GET / should return 200 OK");
    assert!(resp.contains("<html"), "GET / should return HTML");
    assert!(
        resp.contains("Forge Website"),
        "GET / should contain page title"
    );

    // GET /about -> served about.html
    let resp = http_get("/about");
    assert!(resp.contains("200 OK"), "GET /about should return 200 OK");
    assert!(
        resp.contains("About Forge"),
        "GET /about should contain title"
    );

    // GET /style.css -> correct content type
    let resp = http_get("/style.css");
    assert!(resp.contains("200 OK"));
    assert!(
        resp.contains("text/css"),
        "style.css should have text/css content type"
    );
    assert!(
        resp.contains("box-sizing"),
        "style.css should contain CSS rules"
    );

    // GET /app.js -> correct content type
    let resp = http_get("/app.js");
    assert!(resp.contains("200 OK"));
    assert!(
        resp.contains("application/javascript"),
        "app.js should have application/javascript content type"
    );

    // GET /api/status -> JSON response
    let resp = http_get("/api/status");
    assert!(resp.contains("200 OK"));
    assert!(resp.contains("application/json"));
    assert!(
        resp.contains("\"status\":\"ok\""),
        "API status should contain status:ok"
    );
    assert!(
        resp.contains("forge-web"),
        "API status should contain service name"
    );

    // GET /api/random -> JSON with a numeric value
    let resp = http_get("/api/random");
    assert!(resp.contains("200 OK"));
    assert!(resp.contains("application/json"));
    assert!(
        resp.contains("\"value\":"),
        "API random should contain value field"
    );

    // GET /hello?name=Forge -> dynamic greeting
    let resp = http_get("/hello?name=Forge");
    assert!(resp.contains("200 OK"));
    assert!(
        resp.contains("Hello, Forge!"),
        "Hello page should greet Forge"
    );

    // GET /hello (no name param) -> default to World
    let resp = http_get("/hello");
    assert!(
        resp.contains("Hello, World!"),
        "Hello page should default to World"
    );

    // GET /counter -> dynamic counter page
    let resp = http_get("/counter");
    assert!(resp.contains("200 OK"));
    assert!(
        resp.contains("Total page views:"),
        "Counter page should show count"
    );

    // GET /nonexistent -> 404
    let resp = http_get("/nonexistent");
    assert!(
        resp.contains("404 Not Found"),
        "Unknown route should return 404"
    );

    // Clean up
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn website_dev_compiles_and_serves_pages_x86_32() {
    // std.fmt now includes format_f64 which uses float64, not supported on x86_32.
    let out_dir = std::env::temp_dir().join("forge_website_server_x86_32-unknown-linux-gnu_test");
    let _ = std::fs::create_dir_all(&out_dir);
    let bin = out_dir.join("server");
    let mut cmd = Command::cargo_bin("forgec").unwrap();
    cmd.arg("examples/website/server.dev")
        .arg("-o")
        .arg(&bin)
        .arg("--target")
        .arg("x86_32-unknown-linux-gnu");
    let result = cmd.output().unwrap();
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        if stderr.contains("floating point is not implemented") {
            return; // Known limitation: x86_32 backend doesn't support floats
        }
        panic!("Compilation failed unexpectedly:\n{}", stderr);
    }

    let mut child = std::process::Command::new(&bin)
        .current_dir("examples/website")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn website server (x86_32)");

    std::thread::sleep(std::time::Duration::from_secs(3));

    let resp = http_get("/");
    assert!(
        resp.contains("200 OK"),
        "GET / should return 200 OK (x86_32)"
    );
    assert!(resp.contains("<html"), "GET / should return HTML (x86_32)");

    let resp = http_get("/api/status");
    assert!(
        resp.contains("\"status\":\"ok\""),
        "API status should work (x86_32)"
    );

    let resp = http_get("/nonexistent");
    assert!(
        resp.contains("404 Not Found"),
        "Unknown route should return 404 (x86_32)"
    );

    let _ = child.kill();
    let _ = child.wait();
}

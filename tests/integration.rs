use assert_cmd::Command;
use std::fs;

fn compile_example(name: &str) -> std::path::PathBuf {
    compile_example_with_target(name, "x86_64-unknown-linux-gnu")
}

fn compile_example_with_target(name: &str, target: &str) -> std::path::PathBuf {
    let out_dir = std::env::temp_dir().join(format!("forge_{}_{}_test", name, target));
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

#[test]
fn hello_dev_compiles_and_runs() {
    let bin = compile_example("hello");
    let output = Command::new(&bin).output().expect("failed to run hello binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "Hello, Forge!\n");
    assert!(output.status.success(), "hello binary exited with non-zero status");
}

#[test]
fn hello_dev_compiles_and_runs_x86_32() {
    let bin = compile_example_with_target("hello", "x86_32-unknown-linux-gnu");
    let output = Command::new(&bin).output().expect("failed to run hello (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "Hello, Forge!\n");
    assert!(output.status.success(), "hello (x86_32) binary exited with non-zero status");
}

#[test]
fn bump_fmt_dev_compiles_and_runs() {
    let bin = compile_example("bump_fmt");
    let output = Command::new(&bin).output().expect("failed to run bump_fmt binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout,
        "0\n42\n-7\n100\n2147483647\n-2147483647\n",
        "bump_fmt produced unexpected output"
    );
    assert!(output.status.success(), "bump_fmt binary exited with non-zero status");
}

#[test]
fn bump_fmt_dev_compiles_and_runs_x86_32() {
    let bin = compile_example_with_target("bump_fmt", "x86_32-unknown-linux-gnu");
    let output = Command::new(&bin).output().expect("failed to run bump_fmt (x86_32) binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout,
        "0\n42\n-7\n100\n2147483647\n-2147483647\n",
        "bump_fmt (x86_32) produced unexpected output"
    );
    assert!(
        output.status.success(),
        "bump_fmt (x86_32) binary exited with non-zero status"
    );
}

#[test]
fn bump_dev_compiles_and_runs() {
    let bin = compile_example("bump");
    let output = Command::new(&bin).output().expect("failed to run bump binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "bump ok\n");
    assert!(output.status.success(), "bump binary exited with non-zero status");
}

#[test]
fn fib_dev_compiles_and_runs() {
    let bin = compile_example("fib");
    let output = Command::new(&bin).output().expect("failed to run fib binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "fib ok\n");
    assert!(output.status.success(), "fib binary exited with non-zero status");
}

#[test]
fn struct_dev_compiles_and_runs() {
    let bin = compile_example("struct");
    let output = Command::new(&bin).output().expect("failed to run struct binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "struct ok\n");
    assert!(output.status.success(), "struct binary exited with non-zero status");
}

#[test]
fn getchar_dev_compiles_and_runs() {
    let bin = compile_example("getchar");
    let mut cmd = Command::new(&bin);
    cmd.write_stdin("x\n");
    let output = cmd.output().expect("failed to run getchar binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Got a character"), "unexpected output: {}", stdout);
    assert!(output.status.success(), "getchar binary exited with non-zero status");
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
    assert!(stdout.contains("Final score:"), "unexpected output: {}", stdout);
    assert!(output.status.success(), "rps binary exited with non-zero status");
}

#[test]
fn volatile_dev_compiles_and_runs() {
    let bin = compile_example("volatile");
    let output = Command::new(&bin).output().expect("failed to run volatile binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "volatile u32 ok\nvolatile u8 ok\n");
    assert!(output.status.success(), "volatile binary exited with non-zero status");
}

#[test]
fn mem_dev_compiles_and_runs() {
    let bin = compile_example("mem");
    let output = Command::new(&bin).output().expect("failed to run mem binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "copy ok\nset ok\nzero ok\ncompare ok\n");
    assert!(output.status.success(), "mem binary exited with non-zero status");
}

#[test]
fn string_dev_compiles_and_runs() {
    let bin = compile_example("string");
    let output = Command::new(&bin).output().expect("failed to run string binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "string ok\n");
    assert!(output.status.success(), "string binary exited with non-zero status");
}

#[test]
fn math_dev_compiles_and_runs() {
    let bin = compile_example("math");
    let output = Command::new(&bin).output().expect("failed to run math binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "math ok\n");
    assert!(output.status.success(), "math binary exited with non-zero status");
}

#[test]
fn match_bc_dev_compiles_and_runs() {
    let bin = compile_example("match_bc");
    let output = Command::new(&bin).output().expect("failed to run match_bc binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "thirteen\n");
    assert!(output.status.success(), "match_bc binary exited with non-zero status");
}

#[test]
fn bootloader_dev_compiles_and_runs() {
    let out_dir = std::env::temp_dir().join(format!("forge_bootloader_test_{}", std::process::id()));
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
    assert_eq!(&bytes[510..512],
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

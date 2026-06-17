pub mod elf;
pub mod elf32;
pub mod flat;

use std::io;
use std::path::Path;

/// Common interface for object-file / binary writers.
pub trait ObjectWriter {
    fn write(&self, path: &Path) -> io::Result<()>;
}

impl ObjectWriter for elf::Elf64Writer {
    fn write(&self, path: &Path) -> io::Result<()> {
        elf::Elf64Writer::write(self, path)
    }
}

impl ObjectWriter for flat::FlatWriter {
    fn write(&self, path: &Path) -> io::Result<()> {
        flat::FlatWriter::write(self, path)
    }
}

impl ObjectWriter for elf32::Elf32Writer {
    fn write(&self, path: &Path) -> io::Result<()> {
        elf32::Elf32Writer::write(self, path)
    }
}

#[cfg(test)]
mod tests {
    use super::elf::Elf64Writer;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    /// A minimal x86-64 Linux static _start that exits with status 42.
    const EXIT_42: &[u8] = &[
        0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00, // mov $60, %rax   (sys_exit)
        0x48, 0xc7, 0xc7, 0x2a, 0x00, 0x00, 0x00, // mov $42, %rdi   (status)
        0x0f, 0x05,                               // syscall
    ];

    /// A minimal x86-32 Linux static _start that exits with status 42.
    const EXIT_42_32: &[u8] = &[
        0xbb, 0x2a, 0x00, 0x00, 0x00, // mov $42, %ebx
        0xb8, 0x01, 0x00, 0x00, 0x00, // mov $1, %eax   (sys_exit)
        0xcd, 0x80,                   // int $0x80
    ];

    #[test]
    fn tiny_static_elf_exits_42() {
        let base = 0x400000u64;
        let phnum = 2u64; // two PT_LOAD segments for a static executable
        let header_area = 64 + 56 * phnum;
        let entry = base + header_area;

        let writer = Elf64Writer::new(
            EXIT_42.to_vec(),
            Vec::new(),
            Vec::new(),
            0,
            entry,
        );

        let mut path = std::env::temp_dir();
        path.push(format!("elf64_writer_test_{}", std::process::id()));

        writer.write(&path).expect("failed to write ELF");

        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();

        let status = Command::new(&path).status().expect("failed to execute ELF");

        // Clean up the temporary executable.
        let _ = fs::remove_file(&path);

        assert_eq!(status.code(), Some(42), "ELF did not exit with status 42");
    }

    #[test]
    fn tiny_static_elf32_exits_42() {
        use super::elf32::Elf32Writer;
        let base = 0x08048000u32;
        let phnum = 2u32;
        let header_area = 52 + 32 * phnum;
        let entry = base + header_area;

        let writer = Elf32Writer::new(
            EXIT_42_32.to_vec(),
            Vec::new(),
            Vec::new(),
            0,
            entry,
        );

        let mut path = std::env::temp_dir();
        path.push(format!("elf32_writer_test_{}", std::process::id()));

        writer.write(&path).expect("failed to write ELF");

        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();

        let status = Command::new(&path).status().expect("failed to execute ELF");

        let _ = fs::remove_file(&path);

        assert_eq!(status.code(), Some(42), "ELF32 did not exit with status 42");
    }
}

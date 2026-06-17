use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// A simple writer for flat binary images.
///
/// When `boot_sector` is true, the output is padded to 510 bytes and the
/// boot signature `0x55 0xAA` is appended, producing a 512-byte boot sector.
#[derive(Debug, Clone)]
pub struct FlatWriter {
    pub code: Vec<u8>,
    pub boot_sector: bool,
}

impl FlatWriter {
    pub fn new(code: Vec<u8>, boot_sector: bool) -> Self {
        Self { code, boot_sector }
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        let mut out = File::create(path)?;
        out.write_all(&self.code)?;
        if self.boot_sector {
            let pad = 510usize.saturating_sub(self.code.len());
            out.write_all(&vec![0u8; pad])?;
            out.write_all(&[0x55, 0xAA])?;
        }
        out.flush()
    }
}

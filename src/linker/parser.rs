//! Parser for the Forge Linker Descriptor (`.fld`) format.
//!
//! Converts a token stream from [`super::lexer`] into a [`super::config::LinkerConfig`].

use super::config::{LinkerConfig, MemoryRegion, OutputFormat, RuntimeConfig, SectionMapping};
use super::lexer::{Lexer, Tk, Token};

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Self { toks, pos: 0 }
    }

    pub fn parse(mut self) -> Result<LinkerConfig, String> {
        let mut arch: Option<String> = None;
        let mut format: Option<OutputFormat> = None;
        let mut hosted: Option<bool> = None;
        let mut entry: Option<String> = None;
        // base_address is always None here; the default is determined by format below
        let mut heap_size: u64 = 0;
        // Origin for x86_16 flat/raw images; the BIOS loads boot sectors at
        // 0x7C00, other stages can override with `LOAD`.
        let mut load_base: u16 = 0x7C00;
        let mut regions: Vec<MemoryRegion> = Vec::new();
        let mut sections: Vec<SectionMapping> = Vec::new();
        let mut runtime = RuntimeConfig::default();

        while !self.at_eof() {
            let ident = self.expect_ident()?;
            match ident.as_str() {
                "MEMORY" => {
                    self.expect(Tk::LBrace)?;
                    regions = self.parse_memory_regions()?;
                    self.expect(Tk::RBrace)?;
                }
                "SECTIONS" => {
                    self.expect(Tk::LBrace)?;
                    sections = self.parse_sections()?;
                    self.expect(Tk::RBrace)?;
                }
                "RUNTIME" => {
                    self.expect(Tk::LBrace)?;
                    runtime = self.parse_runtime()?;
                    self.expect(Tk::RBrace)?;
                }
                "ARCH" => arch = Some(self.expect_ident()?),
                "FORMAT" => format = Some(self.parse_format()?),
                "HOSTED" => hosted = Some(self.parse_bool()?),
                "ENTRY" => entry = Some(self.expect_ident()?),
                "LOAD" => load_base = self.parse_load_base()?,
                "STACK" => {
                    self.skip_stack();
                }
                "HEAP" => heap_size = self.parse_heap()?,
                _ => Err(format!(
                    "unknown block/key `{}` at line {}",
                    ident,
                    self.cur_line()
                ))?,
            }
        }

        let arch = arch.ok_or_else(|| "missing ARCH".to_string())?;
        let format = format.ok_or_else(|| "missing FORMAT".to_string())?;
        let hosted = hosted.unwrap_or(true);
        let entry = entry.unwrap_or_else(|| {
            if hosted {
                "_forge_main".to_string()
            } else {
                "_start".to_string()
            }
        });
        let base_address = match &format {
            OutputFormat::Elf => 0x400000,
            OutputFormat::Elf32 => 0x08048000,
            OutputFormat::Flat | OutputFormat::Raw => 0x0,
        };

        let config = LinkerConfig {
            arch,
            format,
            hosted,
            entry,
            base_address,
            load_base,
            heap_size,
            regions,
            sections,
            runtime,
        };
        config
            .validate()
            .map_err(|e| format!("validation: {}", e))?;
        Ok(config)
    }

    fn parse_memory_regions(&mut self) -> Result<Vec<MemoryRegion>, String> {
        let mut regions = Vec::new();
        while self.peek() != Tk::RBrace {
            let name = self.expect_ident()?;
            self.expect(Tk::LParen)?;
            let flags = self.parse_flags()?;
            self.expect(Tk::RParen)?;
            self.expect(Tk::Colon)?;
            let mut origin = 0u64;
            let mut length = 0u64;
            loop {
                let key = self.expect_ident()?;
                self.expect(Tk::Equals)?;
                let val = self.expect_number()?;
                match key.as_str() {
                    "origin" => origin = val,
                    "length" => length = val,
                    _ => Err(format!("unknown region field `{}`", key))?,
                }
                if self.peek() == Tk::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            regions.push(MemoryRegion {
                name,
                read: flags.0,
                write: flags.1,
                exec: flags.2,
                origin,
                length,
            });
        }
        Ok(regions)
    }

    fn parse_flags(&mut self) -> Result<(bool, bool, bool), String> {
        let mut r = false;
        let mut w = false;
        let mut x = false;
        while self.peek() != Tk::RParen {
            let flags = self.expect_ident()?;
            for c in flags.chars() {
                match c {
                    'r' if !r => r = true,
                    'w' if !w => w = true,
                    'x' if !x => x = true,
                    _ => return Err(format!("unknown permission flag `{}`", c)),
                }
            }
        }
        Ok((r, w, x))
    }

    fn parse_sections(&mut self) -> Result<Vec<SectionMapping>, String> {
        let mut mappings = Vec::new();
        while self.peek() != Tk::RBrace {
            let section = self.expect_ident()?;
            self.expect(Tk::Gt)?;
            let region = self.expect_ident()?;
            mappings.push(SectionMapping { section, region });
        }
        Ok(mappings)
    }

    fn parse_runtime(&mut self) -> Result<RuntimeConfig, String> {
        let mut rc = RuntimeConfig::default();
        while self.peek() != Tk::RBrace {
            let key = self.expect_ident()?;
            self.expect(Tk::Equals)?;
            let val = self.parse_bool()?;
            match key.as_str() {
                "syscalls" => rc.syscalls = val,
                "gc" => rc.gc = val,
                "alloc" => rc.alloc = val,
                "float" => rc.float = val,
                "sockets" => rc.sockets = val,
                "files" => rc.files = val,
                _ => Err(format!("unknown runtime field `{}`", key))?,
            }
        }
        Ok(rc)
    }

    fn parse_format(&mut self) -> Result<OutputFormat, String> {
        let s = self.expect_ident()?;
        match s.as_str() {
            "elf" => Ok(OutputFormat::Elf),
            "elf32" => Ok(OutputFormat::Elf32),
            "flat" => Ok(OutputFormat::Flat),
            "raw" => Ok(OutputFormat::Raw),
            _ => Err(format!(
                "unknown format `{}` (expected elf, elf32, flat, raw)",
                s
            )),
        }
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        let s = self.expect_ident()?;
        match s.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(format!("expected `true` or `false`, got `{}`", s)),
        }
    }

    fn parse_heap(&mut self) -> Result<u64, String> {
        let mut size = 0;
        loop {
            let key = self.expect_ident()?;
            self.expect(Tk::Equals)?;
            let val = self.expect_number()?;
            match key.as_str() {
                "size" => size = val,
                _ => Err(format!("unknown heap field `{}`", key))?,
            }
            if self.peek() == Tk::Comma {
                self.advance();
            } else {
                break;
            }
        }
        Ok(size)
    }

    fn skip_stack(&mut self) {
        while self.peek() != Tk::LBrace && !self.at_eof() && !self.at_block_start() {
            self.advance();
        }
    }

    /// `LOAD <hex>` — where an x86_16 flat/raw stage is loaded in memory.
    fn parse_load_base(&mut self) -> Result<u16, String> {
        let tok = self.peek().clone();
        if let Tk::Number(n) = tok {
            self.advance();
            if n > u64::from(u16::MAX) {
                return Err(format!("LOAD address {} does not fit in 16 bits", n));
            }
            Ok(n as u16)
        } else {
            Err("LOAD expects a hexadecimal address".to_string())
        }
    }

    fn at_block_start(&self) -> bool {
        matches!(self.peek(), Tk::Ident(s) if matches!(s.as_str(), "MEMORY" | "SECTIONS" | "RUNTIME" | "ARCH" | "FORMAT" | "HOSTED" | "ENTRY" | "LOAD" | "STACK" | "HEAP"))
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.cur() {
            Tk::Ident(s) => {
                self.advance();
                Ok(s.clone())
            }
            Tk::Eof => Err(format!("unexpected EOF at line {}", self.cur_line())),
            other => Err(format!(
                "expected identifier, got {:?} at line {}",
                other,
                self.cur_line()
            )),
        }
    }

    fn expect_number(&mut self) -> Result<u64, String> {
        match self.cur() {
            Tk::Number(n) => {
                self.advance();
                Ok(n)
            }
            Tk::Eof => Err(format!("unexpected EOF at line {}", self.cur_line())),
            other => Err(format!(
                "expected number, got {:?} at line {}",
                other,
                self.cur_line()
            )),
        }
    }

    fn expect(&mut self, tk: Tk) -> Result<(), String> {
        if self.peek() == tk {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "expected {:?}, got {:?} at line {}",
                tk,
                self.peek(),
                self.cur_line()
            ))
        }
    }

    fn peek(&self) -> Tk {
        self.cur()
    }

    fn cur(&self) -> Tk {
        self.toks
            .get(self.pos)
            .map(|t| t.kind.clone())
            .unwrap_or(Tk::Eof)
    }

    fn cur_line(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.line).unwrap_or(0)
    }

    fn at_eof(&self) -> bool {
        self.pos >= self.toks.len() || self.cur() == Tk::Eof
    }

    fn advance(&mut self) {
        if self.pos < self.toks.len() {
            self.pos += 1;
        }
    }
}

pub fn parse_linker_script(src: &str) -> Result<LinkerConfig, String> {
    let mut lexer = Lexer::new(src);
    let toks = lexer.tokenize()?;
    let parser = Parser::new(toks);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_hosted() {
        let src = r#"
            ARCH x86_64
            FORMAT elf
            HOSTED true
            ENTRY _forge_main
        "#;
        let cfg = parse_linker_script(src).unwrap();
        assert_eq!(cfg.arch, "x86_64");
        assert!(cfg.hosted);
        assert_eq!(cfg.entry, "_forge_main");
    }

    #[test]
    fn parse_freestanding_flat() {
        let src = r#"
            ARCH x86_16
            FORMAT flat
            HOSTED false
            ENTRY _start

            MEMORY {
                ram (rwx) : origin = 0x0000, length = 64K
            }

            SECTIONS {
                .text > ram
                .rodata > ram
                .data > ram
                .bss > ram
            }

            RUNTIME {
                syscalls = false
                gc = false
                alloc = false
                float = false
                sockets = false
                files = false
            }
        "#;
        let cfg = parse_linker_script(src).unwrap();
        assert_eq!(cfg.arch, "x86_16");
        assert!(!cfg.hosted);
        assert_eq!(cfg.format, OutputFormat::Flat);
        assert_eq!(cfg.regions.len(), 1);
        assert_eq!(cfg.regions[0].origin, 0);
        assert_eq!(cfg.regions[0].length, 65536);
        assert_eq!(cfg.sections.len(), 4);
    }

    #[test]
    fn parse_with_heap_and_gc() {
        let src = r#"
            ARCH x86_64
            FORMAT elf
            HOSTED false
            ENTRY _start
            HEAP size = 4M

            RUNTIME {
                syscalls = false
                gc = true
                alloc = true
                float = true
                sockets = false
                files = false
            }
        "#;
        let cfg = parse_linker_script(src).unwrap();
        assert_eq!(cfg.heap_size, 4 * 1024 * 1024);
        assert!(cfg.runtime.gc);
        assert!(cfg.runtime.alloc);
    }

    #[test]
    fn gc_without_alloc_fails() {
        let src = r#"
            ARCH x86_64
            FORMAT elf
            HOSTED false
            ENTRY _start
            RUNTIME { gc = true alloc = false }
        "#;
        assert!(parse_linker_script(src).is_err());
    }

    #[test]
    fn missing_arch_fails() {
        let src = "FORMAT elf\nHOSTED true\n";
        assert!(parse_linker_script(src).is_err());
    }

    #[test]
    fn section_mapping_parses() {
        let src = r#"
            ARCH x86_64
            FORMAT elf
            HOSTED true
            ENTRY _forge_main
            MEMORY {
                rom (rx) : origin = 0x0, length = 64K
                ram (rwx) : origin = 0x10000, length = 512K
            }
            SECTIONS {
                .text > rom
                .rodata > rom
                .data > ram
                .bss > ram
            }
        "#;
        let cfg = parse_linker_script(src).unwrap();
        assert_eq!(cfg.sections[0].section, ".text");
        assert_eq!(cfg.sections[0].region, "rom");
        assert_eq!(cfg.sections[3].region, "ram");
    }
}

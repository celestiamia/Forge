use anyhow::{Result, bail};
use std::collections::HashMap;

/// A tiny two-pass assembler for the subset of 16-bit x86 real-mode
/// instructions needed by the Forge bootloader examples.
///
/// Supports:
///   label:
///   db byte, byte, ...   (bytes as hex 0xNN, decimal, or 'string')
///   cli, hlt, lodsb, ret
///   xor ax, ax
///   mov ax, imm16 / label
///   mov ds|es|ss, ax
///   mov sp|si|bx, imm16 / label
///   mov ah, imm8
///   or al, al
///   int imm8
///   out imm8, al
///   jmp label  (short)
///   je/jz label, jne/jnz label (short)
///   call label (near rel16)
pub fn assemble(source: &str) -> Result<Vec<u8>> {
    assemble_with_origin(source, 0)
}

/// Assemble with a fixed origin added to every absolute label reference.
///
/// This is used for boot sectors: the binary is loaded at physical 0x7C00,
/// so referencing a label as an immediate address yields `origin + label_offset`.
pub fn assemble_with_origin(source: &str, origin: u16) -> Result<Vec<u8>> {
    let mut asm = AsmState::new(origin);

    // First pass: resolve label offsets and emit code with placeholder fixups.
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("");
        let line = line.split(';').next().unwrap_or("");
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        asm.parse_line(line)?;
    }

    // Second pass: patch short jump / call displacements and absolute immediates.
    for (label, offset) in asm.fixups {
        let target = asm
            .labels
            .get(&label)
            .ok_or_else(|| anyhow::anyhow!("undefined label: {}", label))?;
        let pc = offset + 1; // after the 1-byte disp field
        let rel = *target as i64 - pc as i64;
        if rel < -128 || rel > 127 {
            bail!("short jump to {} out of range", label);
        }
        asm.bytes[offset] = rel as u8;
    }

    for (label, offset) in asm.rel16_fixups {
        let target = asm
            .labels
            .get(&label)
            .ok_or_else(|| anyhow::anyhow!("undefined label: {}", label))?;
        let pc = offset + 2; // after the 2-byte disp field
        let rel = (*target as i64 - pc as i64) as i16;
        let bytes = rel.to_le_bytes();
        asm.bytes[offset] = bytes[0];
        asm.bytes[offset + 1] = bytes[1];
    }

    for (label, offset) in asm.imm16_fixups {
        let target = asm
            .labels
            .get(&label)
            .ok_or_else(|| anyhow::anyhow!("undefined label: {}", label))?;
        let addr = asm.origin as u32 + *target as u32;
        if addr > u16::MAX as u32 {
            bail!("absolute address for {} exceeds 16 bits", label);
        }
        let bytes = (addr as u16).to_le_bytes();
        asm.bytes[offset] = bytes[0];
        asm.bytes[offset + 1] = bytes[1];
    }

    Ok(asm.bytes)
}

struct AsmState {
    bytes: Vec<u8>,
    labels: HashMap<String, usize>,
    fixups: Vec<(String, usize)>,
    rel16_fixups: Vec<(String, usize)>,
    imm16_fixups: Vec<(String, usize)>,
    origin: u16,
}

impl AsmState {
    fn new(origin: u16) -> Self {
        Self {
            bytes: Vec::new(),
            labels: HashMap::new(),
            fixups: Vec::new(),
            rel16_fixups: Vec::new(),
            imm16_fixups: Vec::new(),
            origin,
        }
    }

    fn parse_line(&mut self, line: &str) -> Result<()> {
        // Label definition?
        if let Some((label, rest)) = line.split_once(':') {
            let label = label.trim();
            if !label.is_empty() {
                self.labels
                    .insert(label.to_string().to_lowercase(), self.bytes.len());
            }
            let rest = rest.trim();
            if rest.is_empty() {
                return Ok(());
            }
            return self.parse_instruction(rest);
        }

        self.parse_instruction(line)
    }

    fn parse_instruction(&mut self, inst: &str) -> Result<()> {
        let inst = inst.trim();
        let tokens: Vec<&str> = inst.split_whitespace().collect();
        if tokens.is_empty() {
            return Ok(());
        }

        match tokens[0].to_lowercase().as_str() {
            "db" => self.parse_db(&tokens[1..].join(" ")),
            "cli" => self.emit(&[0xFA]),
            "hlt" => self.emit(&[0xF4]),
            "lodsb" => self.emit(&[0xAC]),
            "ret" => self.emit(&[0xC3]),
            "xor" => self.parse_xor(&tokens[1..]),
            "mov" => self.parse_mov(&tokens[1..]),
            "or" => self.parse_or(&tokens[1..]),
            "int" => self.parse_int(&tokens[1..]),
            "out" => self.parse_out(&tokens[1..]),
            "jmp" => self.parse_jmp(&tokens[1..]),
            "je" | "jz" => self.parse_jcc(&tokens[1..], 0x74),
            "jne" | "jnz" => self.parse_jcc(&tokens[1..], 0x75),
            "call" => self.parse_call(&tokens[1..]),
            other => bail!("unsupported instruction: {}", other),
        }
    }

    fn parse_db(&mut self, args: &str) -> Result<()> {
        let mut i = 0;
        let bytes = args.as_bytes();
        while i < bytes.len() {
            // Skip whitespace and commas.
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b',') {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }

            if bytes[i] == b'\'' || bytes[i] == b'"' {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    self.bytes.push(bytes[i]);
                    i += 1;
                }
                if i >= bytes.len() {
                    bail!("unterminated string in db");
                }
                i += 1; // closing quote
            } else {
                let start = i;
                while i < bytes.len() && bytes[i] != b',' && bytes[i] != b' ' && bytes[i] != b'\t' {
                    i += 1;
                }
                let tok = std::str::from_utf8(&bytes[start..i])?;
                self.bytes.push(parse_byte(tok)?);
            }
        }
        Ok(())
    }

    fn parse_xor(&mut self, args: &[&str]) -> Result<()> {
        let args = strip_commas(args);
        if args == ["ax", "ax"] {
            // 16-bit xor ax, ax
            self.emit(&[0x31, 0xC0])
        } else {
            bail!("unsupported xor: {}", args.join(", "))
        }
    }

    fn parse_mov(&mut self, args: &[&str]) -> Result<()> {
        let args = strip_commas(args);
        if args.len() != 2 {
            bail!("mov expects two operands");
        }
        let dst = args[0].as_str();
        let src = args[1].as_str();

        match dst {
            "ds" if src == "ax" => self.emit(&[0x8E, 0xD8]),
            "es" if src == "ax" => self.emit(&[0x8E, 0xC0]),
            "ss" if src == "ax" => self.emit(&[0x8E, 0xD0]),
            "ax" => {
                self.emit(&[0xB8])?;
                self.emit_imm16(src)?;
                Ok(())
            }
            "sp" | "si" | "bx" => {
                let opcode = match dst {
                    "sp" => 0xBC,
                    "si" => 0xBE,
                    "bx" => 0xBB,
                    _ => unreachable!(),
                };
                self.emit(&[opcode])?;
                self.emit_imm16(src)?;
                Ok(())
            }
            "ah" => {
                let imm = parse_u8(src)?;
                self.emit(&[0xB4, imm])
            }
            "bh" => {
                let imm = parse_u8(src)?;
                self.emit(&[0xB7, imm])
            }
            "bl" => {
                let imm = parse_u8(src)?;
                self.emit(&[0xB3, imm])
            }
            _ => bail!("unsupported mov: {}, {}", dst, src),
        }
    }

    fn emit_imm16(&mut self, src: &str) -> Result<()> {
        if let Ok(imm) = parse_u16(src) {
            self.emit(&imm.to_le_bytes())
        } else {
            // Treat as a label: emit a placeholder and record a fixup that will
            // be patched to origin + label_offset in the second pass.
            let label = src.to_lowercase();
            let offset = self.bytes.len();
            self.emit(&[0x00, 0x00])?;
            self.imm16_fixups.push((label, offset));
            Ok(())
        }
    }

    fn parse_or(&mut self, args: &[&str]) -> Result<()> {
        let args = strip_commas(args);
        if args == ["al", "al"] {
            self.emit(&[0x0A, 0xC0])
        } else {
            bail!("unsupported or: {}", args.join(", "))
        }
    }

    fn parse_int(&mut self, args: &[&str]) -> Result<()> {
        let args = strip_commas(args);
        if args.len() != 1 {
            bail!("int expects one operand");
        }
        let imm = parse_u8(&args[0])?;
        self.emit(&[0xCD, imm])
    }

    fn parse_out(&mut self, args: &[&str]) -> Result<()> {
        let args = strip_commas(args);
        if args.len() != 2 || args[1] != "al" {
            bail!("unsupported out: {}", args.join(", "));
        }
        let port = parse_u8(&args[0])?;
        self.emit(&[0xE6, port])
    }

    fn parse_jmp(&mut self, args: &[&str]) -> Result<()> {
        let args = strip_commas(args);
        if args.len() != 1 {
            bail!("jmp expects a label");
        }
        let label = args[0].to_lowercase();
        let offset = self.bytes.len();
        self.emit(&[0xEB, 0x00])?; // placeholder
        self.fixups.push((label, offset + 1));
        Ok(())
    }

    fn parse_jcc(&mut self, args: &[&str], opcode: u8) -> Result<()> {
        let args = strip_commas(args);
        if args.len() != 1 {
            bail!("conditional jump expects a label");
        }
        let label = args[0].to_lowercase();
        let offset = self.bytes.len();
        self.emit(&[opcode, 0x00])?; // placeholder
        self.fixups.push((label, offset + 1));
        Ok(())
    }

    fn parse_call(&mut self, args: &[&str]) -> Result<()> {
        let args = strip_commas(args);
        if args.len() != 1 {
            bail!("call expects a label");
        }
        let label = args[0].to_lowercase();
        let offset = self.bytes.len();
        self.emit(&[0xE8, 0x00, 0x00])?; // placeholder rel16
        self.rel16_fixups.push((label, offset + 1));
        Ok(())
    }

    fn emit(&mut self, bytes: &[u8]) -> Result<()> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

fn strip_commas(args: &[&str]) -> Vec<String> {
    args.iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_u8(s: &str) -> Result<u8> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u8::from_str_radix(&s[2..], 16).map_err(|e| anyhow::anyhow!("bad byte {}: {}", s, e))
    } else {
        s.parse::<u8>()
            .map_err(|e| anyhow::anyhow!("bad byte {}: {}", s, e))
    }
}

fn parse_u16(s: &str) -> Result<u16> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u16::from_str_radix(&s[2..], 16).map_err(|e| anyhow::anyhow!("bad word {}: {}", s, e))
    } else {
        s.parse::<u16>()
            .map_err(|e| anyhow::anyhow!("bad word {}: {}", s, e))
    }
}

fn parse_byte(s: &str) -> Result<u8> {
    parse_u8(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_bootsector() {
        let src = r#"
            xor ax, ax
            mov ds, ax
            mov ss, ax
            mov sp, 0x7C00
            mov si, msg
            call print
            cli
            hlt

        print:
            lodsb
            or al, al
            je done
            out 0xE9, al
            jmp print
        done:
            ret

        msg: db 'Hi', 0
        "#;
        let bytes = assemble_with_origin(src, 0x7C00).unwrap();
        assert!(bytes.len() <= 510, "boot code too large: {}", bytes.len());
        assert_eq!(bytes[0], 0x31); // xor ax, ax
    }
}

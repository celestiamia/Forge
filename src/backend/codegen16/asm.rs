use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;

pub(crate) struct Encoder {
    bytes: Vec<u8>,
    labels: HashMap<u32, usize>,
    short_fixups: Vec<(u32, usize)>,
    rel16_fixups: Vec<(u32, usize)>,
    imm16_fixups: Vec<(u32, usize)>,
    next_label: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg16 {
    Ax = 0,
    Cx = 1,
    Dx = 2,
    Bx = 3,
    Sp = 4,
    Bp = 5,
    Si = 6,
    Di = 7,
}

impl Reg16 {
    /// 16-bit memory-addressing r/m code for indirect operands.
    /// This differs from the register-field encoding: BP is encoded as 110
    /// and SI as 100 when used as a base/index in a ModR/M memory operand.
    pub(super) fn rm16(self) -> u8 {
        match self {
            Reg16::Si => 4,
            Reg16::Di => 5,
            Reg16::Bp => 6,
            Reg16::Bx => 7,
            _ => self as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg8 {
    Al = 0,
    Cl = 1,
    Dl = 2,
    Bl = 3,
    Ah = 4,
    Ch = 5,
    Dh = 6,
    Bh = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SegReg {
    Es = 0,
    Cs = 1,
    Ss = 2,
    Ds = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Cond {
    E = 0x4,
    Ne = 0x5,
    B = 0x2,
    Be = 0x6,
    A = 0x7,
    Ae = 0x3,
    L = 0xC,
    Le = 0xE,
    G = 0xF,
    Ge = 0xD,
}

impl Encoder {
    pub(super) fn new() -> Self {
        Self {
            bytes: Vec::new(),
            labels: HashMap::new(),
            short_fixups: Vec::new(),
            rel16_fixups: Vec::new(),
            imm16_fixups: Vec::new(),
            next_label: 1,
        }
    }

    pub(super) fn new_label(&mut self) -> u32 {
        let lab = self.next_label;
        self.next_label += 1;
        lab
    }

    pub(super) fn bind(&mut self, lab: u32) {
        self.labels.insert(lab, self.bytes.len());
    }

    pub(super) fn into_bytes(self) -> Result<Vec<u8>> {
        let mut bytes = self.bytes;
        for (lab, off) in self.short_fixups {
            let target = *self
                .labels
                .get(&lab)
                .ok_or_else(|| anyhow!("undefined short jump label {}", lab))?;
            let pc = off + 1;
            let rel = target as i64 - pc as i64;
            if rel < -128 || rel > 127 {
                bail!(
                    "16-bit backend: short jump to label {} is out of range ({} bytes)",
                    lab,
                    rel
                );
            }
            bytes[off] = rel as u8;
        }
        for (lab, off) in self.rel16_fixups {
            let target = *self
                .labels
                .get(&lab)
                .ok_or_else(|| anyhow!("undefined rel16 label {}", lab))?;
            let pc = off + 2;
            let rel = (target as i64 - pc as i64) as i16;
            bytes[off..off + 2].copy_from_slice(&rel.to_le_bytes());
        }
        for (lab, off) in self.imm16_fixups {
            let target = *self
                .labels
                .get(&lab)
                .ok_or_else(|| anyhow!("undefined imm16 label {}", lab))?;
            let addr = (0x7C00u32 + target as u32) as u16;
            bytes[off..off + 2].copy_from_slice(&addr.to_le_bytes());
        }
        Ok(bytes)
    }

    pub(super) fn emit(&mut self, b: u8) {
        self.bytes.push(b);
    }

    pub(super) fn emit_slice(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(super) fn emit_imm8(&mut self, v: i8) {
        self.bytes.push(v as u8);
    }

    pub(super) fn emit_imm16(&mut self, v: u16) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    pub(super) fn modrm(&mut self, mode: u8, reg: u8, rm: u8) {
        self.emit(((mode & 3) << 6) | ((reg & 7) << 3) | (rm & 7));
    }

    pub(super) fn sib8(&mut self, v: i8) {
        self.emit(v as u8);
    }

    pub(super) fn push(&mut self, r: Reg16) {
        self.emit(0x50 + r as u8);
    }

    pub(super) fn pop(&mut self, r: Reg16) {
        self.emit(0x58 + r as u8);
    }

    pub(super) fn mov16_rr(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x89);
        self.modrm(3, src as u8, dst as u8);
    }

    pub(super) fn mov16_rm(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x8B);
        self.modrm(3, dst as u8, src as u8);
    }

    pub(super) fn mov16_imm(&mut self, r: Reg16, imm: u16) {
        self.emit(0xB8 + r as u8);
        self.emit_imm16(imm);
    }

    pub(super) fn mov16_imm_label(&mut self, r: Reg16, lab: u32) {
        self.emit(0xB8 + r as u8);
        let off = self.bytes.len();
        self.emit_imm16(0);
        self.imm16_fixups.push((lab, off));
    }

    pub(super) fn mov8_imm(&mut self, r: Reg8, imm: u8) {
        self.emit(0xB0 + r as u8);
        self.emit(imm);
    }

    pub(super) fn mov_sp_imm(&mut self, imm: u16) {
        self.emit(0xBC);
        self.emit_imm16(imm);
    }

    pub(super) fn mov_seg_ax(&mut self, seg: SegReg) -> Result<()> {
        match seg {
            SegReg::Ds => self.emit_slice(&[0x8E, 0xD8]),
            SegReg::Es => self.emit_slice(&[0x8E, 0xC0]),
            SegReg::Ss => self.emit_slice(&[0x8E, 0xD0]),
            SegReg::Cs => bail!("loading into CS segment register is not supported"),
        }
        Ok(())
    }

    pub(super) fn lea_bp(&mut self, dst: Reg16, off: i8) {
        self.emit(0x8D);
        self.modrm(1, dst as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    pub(super) fn load16_bp(&mut self, dst: Reg16, off: i8) {
        self.emit(0x8B);
        self.modrm(1, dst as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    pub(super) fn store16_bp(&mut self, off: i8, src: Reg16) {
        self.emit(0x89);
        self.modrm(1, src as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    pub(super) fn load8_bp(&mut self, dst: Reg8, off: i8) {
        self.emit(0x8A);
        self.modrm(1, dst as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    pub(super) fn store8_bp(&mut self, off: i8, src: Reg8) {
        self.emit(0x88);
        self.modrm(1, src as u8, Reg16::Bp.rm16());
        self.emit(off as u8);
    }

    pub(super) fn load16_si(&mut self, dst: Reg16) {
        self.emit(0x8B);
        self.modrm(0, dst as u8, Reg16::Si.rm16());
    }

    pub(super) fn store16_si(&mut self, src: Reg16) {
        self.emit(0x89);
        self.modrm(0, src as u8, Reg16::Si.rm16());
    }

    pub(super) fn load8_si(&mut self, dst: Reg8) {
        self.emit(0x8A);
        self.modrm(0, dst as u8, Reg16::Si.rm16());
    }

    pub(super) fn store8_si(&mut self, src: Reg8) {
        self.emit(0x88);
        self.modrm(0, src as u8, Reg16::Si.rm16());
    }

    pub(super) fn add_rr(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x01);
        self.modrm(3, src as u8, dst as u8);
    }

    pub(super) fn sub_rr(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x29);
        self.modrm(3, src as u8, dst as u8);
    }

    pub(super) fn cmp_rr(&mut self, dst: Reg16, src: Reg16) {
        self.emit(0x39);
        self.modrm(3, src as u8, dst as u8);
    }

    pub(super) fn add_ax_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xC0]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit(0x05);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    pub(super) fn sub_ax_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xE8]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit(0x2D);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    pub(super) fn add_sp_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xC4]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit_slice(&[0x81, 0xC4]);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    pub(super) fn sub_sp_imm(&mut self, imm: i16) -> Result<()> {
        if imm >= i8::MIN as i16 && imm <= i8::MAX as i16 {
            self.emit_slice(&[0x83, 0xEC]);
            self.emit_imm8(imm as i8);
        } else {
            self.emit_slice(&[0x81, 0xEC]);
            self.emit_imm16(imm as u16);
        }
        Ok(())
    }

    pub(super) fn xor_ax_ax(&mut self) {
        self.emit_slice(&[0x31, 0xC0]);
    }

    pub(super) fn xor_ah_ah(&mut self) {
        self.emit_slice(&[0x30, 0xE4]);
    }

    pub(super) fn cbw(&mut self) {
        self.emit(0x98);
    }

    pub(super) fn cwd(&mut self) {
        self.emit(0x99);
    }

    pub(super) fn inc(&mut self, r: Reg16) {
        self.emit(0x40 + r as u8);
    }

    pub(super) fn dec(&mut self, r: Reg16) {
        self.emit(0x48 + r as u8);
    }

    pub(super) fn test_ax_ax(&mut self) {
        self.emit_slice(&[0x85, 0xC0]);
    }

    pub(super) fn shl_ax_imm(&mut self, imm: u8) {
        self.emit_slice(&[0xC1, 0xE0]);
        self.emit(imm);
    }

    pub(super) fn shr_ax_imm(&mut self, imm: u8) {
        self.emit_slice(&[0xC1, 0xE8]);
        self.emit(imm);
    }

    pub(super) fn imul_r16(&mut self, r: Reg16) {
        self.emit(0xF7);
        self.modrm(3, 5, r as u8);
    }

    pub(super) fn idiv_r16(&mut self, r: Reg16) {
        self.emit(0xF7);
        self.modrm(3, 7, r as u8);
    }

    pub(super) fn div_r16(&mut self, r: Reg16) {
        self.emit(0xF7);
        self.modrm(3, 6, r as u8);
    }

    pub(super) fn xor_dx_dx(&mut self) {
        self.emit_slice(&[0x31, 0xD2]);
    }

    pub(super) fn setcc(&mut self, cond: Cond, r: Reg8) {
        self.emit(0x0F);
        self.emit(0x90 + cond as u8);
        self.modrm(3, 0, r as u8);
    }

    pub(super) fn jmp_short_lab(&mut self, lab: u32) {
        self.emit(0xEB);
        let off = self.bytes.len();
        self.emit(0);
        self.short_fixups.push((lab, off));
    }

    pub(super) fn je_short_lab(&mut self, lab: u32) {
        self.jcc_short_lab(0x74, lab);
    }

    pub(super) fn jne_short_lab(&mut self, lab: u32) {
        self.jcc_short_lab(0x75, lab);
    }

    pub(super) fn jcc_short_lab(&mut self, opcode: u8, lab: u32) {
        self.emit(opcode);
        let off = self.bytes.len();
        self.emit(0);
        self.short_fixups.push((lab, off));
    }

    pub(super) fn call_near_lab(&mut self, lab: u32) {
        self.emit(0xE8);
        let off = self.bytes.len();
        self.emit_imm16(0);
        self.rel16_fixups.push((lab, off));
    }

    pub(super) fn ret(&mut self) {
        self.emit(0xC3);
    }

    pub(super) fn cli(&mut self) {
        self.emit(0xFA);
    }

    pub(super) fn hlt(&mut self) {
        self.emit(0xF4);
    }

    pub(super) fn int(&mut self, imm: u8) {
        self.emit_slice(&[0xCD, imm]);
    }

    pub(super) fn out_imm8_al(&mut self, port: u8) {
        self.emit_slice(&[0xE6, port]);
    }

    pub(super) fn out_dx_al(&mut self) {
        self.emit(0xEE);
    }

    pub(super) fn db(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(super) fn db_str(&mut self, s: &str) {
        self.bytes.extend_from_slice(s.as_bytes());
        self.emit(0);
    }
}

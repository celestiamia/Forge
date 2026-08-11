//! `Assembler` builder that accumulates encoded 32-bit x86 bytes and resolves
//! local labels for forward/backward jumps.
//!
//! All instruction-emitting methods return [`anyhow::Result`] and surface
//! encoding problems via [`crate::backend::error::EncodeError`] /
//! [`crate::backend::error::AssemblerError`] instead of panicking, mirroring
//! the 64-bit assembler.

use crate::backend::error::{fmt_operand32, AssemblerError, EncodeError};
use crate::backend::x86::{
    encode, AluOp, Cond, IntoJmpTarget, IntoOp, JmpTarget, Label, Mem, Operand, Reg,
};
use anyhow::{bail, Result};

#[derive(Debug)]
struct Fixup {
    label: Label,
    /// Offset of the 32-bit displacement field within `buf`.
    offset: usize,
    /// Offset of the next instruction (target used to compute rel32).
    pc: usize,
}

/// A simple 32-bit x86 byte assembler with label fixups.
#[derive(Debug, Default)]
pub struct Assembler {
    buf: Vec<u8>,
    labels: std::collections::HashMap<Label, usize>,
    fixups: Vec<Fixup>,
    next_label: Label,
}

impl Assembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn into_bytes(self) -> Result<Vec<u8>> {
        if !self.fixups.is_empty() {
            let labels: Vec<Label> = self.fixups.iter().map(|f| f.label).collect();
            bail!(AssemblerError::UnresolvedLabels {
                count: self.fixups.len(),
                labels: format!("{:?}", labels),
            });
        }
        Ok(self.buf)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Patch a 32-bit immediate at `offset`. Used for stack-size and address fixups.
    pub fn patch_i32(&mut self, offset: usize, value: i32) {
        self.buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    pub fn append_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn push_byte(&mut self, byte: u8) {
        self.buf.push(byte);
    }

    pub fn leave(&mut self) {
        self.buf.push(0xC9);
    }

    /// Allocate a new label identifier without binding it.
    pub fn new_label(&mut self) -> Label {
        let l = self.next_label;
        self.next_label += 1;
        l
    }

    /// Bind a label to the current code position and resolve pending fixups.
    pub fn bind(&mut self, label: Label) {
        let offset = self.buf.len();
        self.labels.insert(label, offset);
        let mut i = 0;
        while i < self.fixups.len() {
            if self.fixups[i].label == label {
                let f = self.fixups.swap_remove(i);
                let rel = offset as i64 - f.pc as i64;
                let bytes = (rel as i32).to_le_bytes();
                self.buf[f.offset..f.offset + 4].copy_from_slice(&bytes);
            } else {
                i += 1;
            }
        }
    }

    /// Convenience: allocate a label and bind it at the current position.
    pub fn label(&mut self) -> Label {
        let l = self.new_label();
        self.bind(l);
        l
    }

    fn emit_rel32(&mut self, label: Label, pc_after_disp: usize) {
        if let Some(&target) = self.labels.get(&label) {
            let rel = target as i64 - pc_after_disp as i64;
            self.buf.extend_from_slice(&(rel as i32).to_le_bytes());
        } else {
            self.fixups.push(Fixup {
                label,
                offset: self.buf.len(),
                pc: pc_after_disp,
            });
            self.buf.extend_from_slice(&0i32.to_le_bytes());
        }
    }

    fn emit_jmp_label(&mut self, label: Label, cond: Option<Cond>) {
        let pc = self.buf.len();
        match cond {
            None => {
                self.buf.push(0xE9);
                self.emit_rel32(label, pc + 5);
            }
            Some(c) => {
                self.buf.push(0x0F);
                self.buf.push(0x80 + c.opcode_offset());
                self.emit_rel32(label, pc + 6);
            }
        }
    }

    // ------------------------------------------------------------------------
    // Public instruction methods
    // ------------------------------------------------------------------------

    pub fn mov(&mut self, dst: impl IntoOp, src: impl IntoOp) -> Result<()> {
        let dst = dst.into_op();
        let src = src.into_op();
        match (dst, src) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                encode::mov_rm32_r32(&mut self.buf, Operand::Reg(dst), src)?;
            }
            (Operand::Reg(dst), Operand::Mem(src)) => {
                encode::mov_r32_rm32(&mut self.buf, dst, Operand::Mem(src))?;
            }
            (Operand::Mem(dst), Operand::Reg(src)) => {
                encode::mov_rm32_r32(&mut self.buf, Operand::Mem(dst), src)?;
            }
            (Operand::Reg(dst), Operand::Imm32(imm)) => {
                encode::mov_rm32_imm32(&mut self.buf, Operand::Reg(dst), imm)?;
            }
            (dst, Operand::Imm32(imm)) => {
                encode::mov_rm32_imm32(&mut self.buf, dst, imm)?;
            }
            (dst, Operand::Imm16(imm)) => {
                encode::mov_rm32_imm32(&mut self.buf, dst, imm as i32)?;
            }
            (dst, Operand::Imm8(imm)) => {
                encode::mov_rm32_imm32(&mut self.buf, dst, imm as i32)?;
            }
            (dst, src) => {
                bail!(EncodeError::InvalidOperands {
                    instruction: "MOV",
                    dst: fmt_operand32(&dst),
                    src: fmt_operand32(&src),
                });
            }
        }
        Ok(())
    }

    fn alu(&mut self, op: AluOp, dst: impl IntoOp, src: impl IntoOp) -> Result<()> {
        let dst = dst.into_op();
        let src = src.into_op();
        match (dst, src) {
            (dst, Operand::Reg(src)) => encode::alu_rm32_r32(&mut self.buf, op, dst, src)?,
            (Operand::Reg(dst), Operand::Mem(src)) => {
                encode::alu_r32_rm32(&mut self.buf, op, dst, Operand::Mem(src))?;
            }
            (dst, Operand::Imm32(imm)) => encode::alu_rm32_imm32(&mut self.buf, op, dst, imm)?,
            (dst, Operand::Imm16(imm)) => {
                encode::alu_rm32_imm32(&mut self.buf, op, dst, imm as i32)?;
            }
            (dst, Operand::Imm8(imm)) => {
                encode::alu_rm32_imm32(&mut self.buf, op, dst, imm as i32)?;
            }
            (dst, src) => {
                bail!(EncodeError::InvalidOperands {
                    instruction: "ALU",
                    dst: fmt_operand32(&dst),
                    src: fmt_operand32(&src),
                });
            }
        }
        Ok(())
    }

    pub fn add(&mut self, dst: impl IntoOp, src: impl IntoOp) -> Result<()> {
        self.alu(AluOp::Add, dst, src)
    }

    pub fn sub(&mut self, dst: impl IntoOp, src: impl IntoOp) -> Result<()> {
        self.alu(AluOp::Sub, dst, src)
    }

    pub fn and(&mut self, dst: impl IntoOp, src: impl IntoOp) -> Result<()> {
        self.alu(AluOp::And, dst, src)
    }

    pub fn or(&mut self, dst: impl IntoOp, src: impl IntoOp) -> Result<()> {
        self.alu(AluOp::Or, dst, src)
    }

    pub fn xor(&mut self, dst: impl IntoOp, src: impl IntoOp) -> Result<()> {
        self.alu(AluOp::Xor, dst, src)
    }

    pub fn cmp(&mut self, a: impl IntoOp, b: impl IntoOp) -> Result<()> {
        let a = a.into_op();
        let b = b.into_op();
        match (a, b) {
            (dst, Operand::Reg(src)) => encode::cmp_rm32_r32(&mut self.buf, dst, src)?,
            (dst, Operand::Imm32(imm)) => encode::cmp_rm32_imm32(&mut self.buf, dst, imm)?,
            (dst, Operand::Imm16(imm)) => {
                encode::cmp_rm32_imm32(&mut self.buf, dst, imm as i32)?;
            }
            (dst, Operand::Imm8(imm)) => {
                encode::cmp_rm32_imm32(&mut self.buf, dst, imm as i32)?;
            }
            (dst, Operand::Imm64(imm)) => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    encode::cmp_rm32_imm32(&mut self.buf, dst, imm as i32)?;
                } else {
                    bail!(EncodeError::ImmTooLarge {
                        instruction: "CMP",
                        value: imm,
                    });
                }
            }
            (a, b) => {
                bail!(EncodeError::InvalidOperands {
                    instruction: "CMP",
                    dst: fmt_operand32(&a),
                    src: fmt_operand32(&b),
                });
            }
        }
        Ok(())
    }

    pub fn test(&mut self, a: impl IntoOp, b: impl IntoOp) -> Result<()> {
        let a = a.into_op();
        let b = b.into_op();
        match (a, b) {
            (dst, Operand::Reg(src)) => encode::test_rm32_r32(&mut self.buf, dst, src)?,
            (dst, Operand::Imm32(imm)) => {
                encode::test_rm32_imm32(&mut self.buf, dst, imm)?;
            }
            (dst, Operand::Imm16(imm)) => {
                encode::test_rm32_imm32(&mut self.buf, dst, imm as i32)?;
            }
            (dst, Operand::Imm8(imm)) => {
                encode::test_rm32_imm32(&mut self.buf, dst, imm as i32)?;
            }
            (dst, Operand::Imm64(imm)) => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    encode::test_rm32_imm32(&mut self.buf, dst, imm as i32)?;
                } else {
                    bail!(EncodeError::ImmTooLarge {
                        instruction: "TEST",
                        value: imm,
                    });
                }
            }
            (a, b) => {
                bail!(EncodeError::InvalidOperands {
                    instruction: "TEST",
                    dst: fmt_operand32(&a),
                    src: fmt_operand32(&b),
                });
            }
        }
        Ok(())
    }

    pub fn neg(&mut self, rm: impl IntoOp) -> Result<()> {
        encode::neg(&mut self.buf, rm.into_op())
    }

    pub fn inc(&mut self, rm: impl IntoOp) -> Result<()> {
        encode::inc(&mut self.buf, rm.into_op())
    }

    pub fn dec(&mut self, rm: impl IntoOp) -> Result<()> {
        encode::dec(&mut self.buf, rm.into_op())
    }

    pub fn push(&mut self, src: impl IntoOp) -> Result<()> {
        let src = src.into_op();
        match src {
            Operand::Reg(r) => encode::push_reg(&mut self.buf, r)?,
            Operand::Imm32(imm) => encode::push_imm32(&mut self.buf, imm)?,
            Operand::Imm16(imm) => encode::push_imm32(&mut self.buf, imm as i32)?,
            Operand::Imm8(imm) => encode::push_imm32(&mut self.buf, imm as i32)?,
            Operand::Imm64(imm) => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    encode::push_imm32(&mut self.buf, imm as i32)?;
                } else {
                    bail!(EncodeError::ImmTooLarge {
                        instruction: "PUSH",
                        value: imm,
                    });
                }
            }
            _ => bail!(EncodeError::InvalidOperands {
                instruction: "PUSH",
                dst: fmt_operand32(&src),
                src: fmt_operand32(&src),
            }),
        }
        Ok(())
    }

    pub fn pop(&mut self, r: Reg) -> Result<()> {
        encode::pop(&mut self.buf, r)
    }

    pub fn call(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::call_rel32(&mut self.buf, rel)?,
            JmpTarget::Label(l) => {
                let pc = self.buf.len();
                self.buf.push(0xE8);
                self.emit_rel32(l, pc + 5);
            }
        }
        Ok(())
    }

    pub fn ret(&mut self) -> Result<()> {
        encode::ret(&mut self.buf)
    }

    pub fn jmp(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::jmp_rel32(&mut self.buf, rel)?,
            JmpTarget::Label(l) => self.emit_jmp_label(l, None),
        }
        Ok(())
    }

    pub fn je(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::E, target)
    }

    pub fn jne(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::Ne, target)
    }

    pub fn jg(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::G, target)
    }

    pub fn jge(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::Ge, target)
    }

    pub fn jl(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::L, target)
    }

    pub fn jle(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::Le, target)
    }

    pub fn ja(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::A, target)
    }

    pub fn jae(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::Ae, target)
    }

    pub fn jb(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::B, target)
    }

    pub fn jbe(&mut self, target: impl IntoJmpTarget) -> Result<()> {
        self.jcc(Cond::Be, target)
    }

    /// Generic conditional jump (any `Cond`).
    pub fn jcc(&mut self, cond: Cond, target: impl IntoJmpTarget) -> Result<()> {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::jcc_rel32(&mut self.buf, cond, rel)?,
            JmpTarget::Label(l) => self.emit_jmp_label(l, Some(cond)),
        }
        Ok(())
    }

    pub fn jcc_rel32(&mut self, cond: Cond, rel: i32) -> Result<()> {
        encode::jcc_rel32(&mut self.buf, cond, rel)
    }

    pub fn setcc(&mut self, cond: Cond, dst: impl IntoOp) -> Result<()> {
        encode::setcc(&mut self.buf, cond, dst.into_op())
    }

    pub fn syscall(&mut self) -> Result<()> {
        encode::syscall(&mut self.buf)
    }

    pub fn int(&mut self, imm: u8) -> Result<()> {
        encode::int(&mut self.buf, imm)
    }

    pub fn lea(&mut self, dst: Reg, src: Mem) -> Result<()> {
        encode::lea_r32_mem(&mut self.buf, dst, src)
    }

    pub fn cdq(&mut self) -> Result<()> {
        encode::cdq(&mut self.buf)
    }

    pub fn imul(&mut self, dst: Reg, src: impl IntoOp) -> Result<()> {
        encode::imul_r32_rm32(&mut self.buf, dst, src.into_op())
    }

    pub fn idiv(&mut self, rm: impl IntoOp) -> Result<()> {
        encode::idiv(&mut self.buf, rm.into_op())
    }

    pub fn div(&mut self, rm: impl IntoOp) -> Result<()> {
        encode::div(&mut self.buf, rm.into_op())
    }

    pub fn shl(&mut self, dst: impl IntoOp, imm: i8) -> Result<()> {
        encode::shl_rm32_imm8(&mut self.buf, dst.into_op(), imm)
    }

    pub fn shl_cl(&mut self, dst: impl IntoOp) -> Result<()> {
        encode::shl_rm32_cl(&mut self.buf, dst.into_op())
    }

    pub fn shr(&mut self, dst: impl IntoOp, imm: i8) -> Result<()> {
        encode::shr_rm32_imm8(&mut self.buf, dst.into_op(), imm)
    }

    pub fn shr_cl(&mut self, dst: impl IntoOp) -> Result<()> {
        encode::shr_rm32_cl(&mut self.buf, dst.into_op())
    }

    pub fn sar(&mut self, dst: impl IntoOp, imm: i8) -> Result<()> {
        encode::sar_rm32_imm8(&mut self.buf, dst.into_op(), imm)
    }

    pub fn sar_cl(&mut self, dst: impl IntoOp) -> Result<()> {
        encode::sar_rm32_cl(&mut self.buf, dst.into_op())
    }

    pub fn movzx8(&mut self, dst: Reg, src: impl IntoOp) -> Result<()> {
        encode::movzx_r32_rm8(&mut self.buf, dst, src.into_op())
    }

    pub fn movsx8(&mut self, dst: Reg, src: impl IntoOp) -> Result<()> {
        encode::movsx_r32_rm8(&mut self.buf, dst, src.into_op())
    }

    pub fn movsx16(&mut self, dst: Reg, src: impl IntoOp) -> Result<()> {
        encode::movsx_r32_rm16(&mut self.buf, dst, src.into_op())
    }

    pub fn movzx16(&mut self, dst: Reg, src: impl IntoOp) -> Result<()> {
        encode::movzx_r32_rm16(&mut self.buf, dst, src.into_op())
    }

    pub fn mov32(&mut self, dst: Reg, src: impl IntoOp) -> Result<()> {
        encode::mov_r32_rm32(&mut self.buf, dst, src.into_op())
    }

    pub fn store8(&mut self, dst: Mem, src: Reg) -> Result<()> {
        encode::mov_rm8_r8(&mut self.buf, dst, src)
    }

    pub fn store16(&mut self, dst: Mem, src: Reg) -> Result<()> {
        encode::mov_rm16_r16(&mut self.buf, dst, src)
    }

    pub fn store32(&mut self, dst: Mem, src: Reg) -> Result<()> {
        encode::mov_rm32_r32_store(&mut self.buf, dst, src)
    }

    pub fn rdtsc(&mut self) -> Result<()> {
        encode::rdtsc(&mut self.buf)
    }

    pub fn lfence(&mut self) -> Result<()> {
        encode::lfence(&mut self.buf)
    }

    pub fn sfence(&mut self) -> Result<()> {
        encode::sfence(&mut self.buf)
    }

    pub fn mfence(&mut self) -> Result<()> {
        encode::mfence(&mut self.buf)
    }
}

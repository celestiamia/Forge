//! `Assembler` builder that accumulates encoded x86-64 bytes and resolves
//! local labels for forward/backward jumps.

use crate::backend::x64::{
    encode, AluOp, Cond, IntoJmpTarget, IntoOp, JmpTarget, Label, Mem, Operand, Reg,
};

#[derive(Debug)]
struct Fixup {
    label: Label,
    /// Offset of the 32-bit displacement field within `buf`.
    offset: usize,
    /// Offset of the next instruction (target used to compute rel32).
    pc: usize,
}

/// A simple x86-64 byte assembler with label fixups.
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

    pub fn into_bytes(self) -> Vec<u8> {
        if !self.fixups.is_empty() {
            panic!("assembler finished with unresolved labels");
        }
        self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Patch a 32-bit immediate at `offset`. Used for stack-size fixups.
    pub fn patch_i32(&mut self, offset: usize, value: i32) {
        self.buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Patch a 64-bit immediate at `offset`. Used for absolute address fixups.
    pub fn patch_i64(&mut self, offset: usize, value: u64) {
        self.buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
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

    /// Emit `mov reg, imm64` unconditionally (the "movabs" form).
    pub fn movabs(&mut self, dst: Reg, imm: u64) {
        encode::mov_r64_imm64(&mut self.buf,
            dst,
            imm as i64,
        );
    }

    pub fn mov(&mut self, dst: impl IntoOp, src: impl IntoOp) {
        let dst = dst.into_op();
        let src = src.into_op();
        match (dst, src) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                encode::mov_rm64_r64(&mut self.buf, Operand::Reg(dst), src);
            }
            (Operand::Reg(dst), Operand::Mem(src)) => {
                encode::mov_r64_rm64(&mut self.buf, dst, Operand::Mem(src));
            }
            (Operand::Mem(dst), Operand::Reg(src)) => {
                encode::mov_rm64_r64(&mut self.buf, Operand::Mem(dst), src);
            }
            (dst, Operand::Imm64(imm)) => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    encode::mov_rm64_imm32(&mut self.buf, dst, imm as i32);
                } else {
                    match dst {
                        Operand::Reg(r) => encode::mov_r64_imm64(&mut self.buf, r, imm),
                        _ => panic!("MOV r/m64, imm64 is not encodable"),
                    }
                }
            }
            (dst, Operand::Imm32(imm)) => {
                encode::mov_rm64_imm32(&mut self.buf, dst, imm);
            }
            (dst, Operand::Imm16(imm)) => {
                encode::mov_rm64_imm32(&mut self.buf, dst, imm as i32);
            }
            (dst, Operand::Imm8(imm)) => {
                encode::mov_rm64_imm32(&mut self.buf, dst, imm as i32);
            }
            _ => panic!("invalid operands for mov"),
        }
    }

    fn alu(&mut self, op: AluOp, dst: impl IntoOp, src: impl IntoOp) {
        let dst = dst.into_op();
        let src = src.into_op();
        match (dst, src) {
            (dst, Operand::Reg(src)) => encode::alu_rm64_r64(&mut self.buf, op, dst, src),
            (dst, Operand::Imm32(imm)) => encode::alu_rm64_imm32(&mut self.buf, op, dst, imm),
            (dst, Operand::Imm16(imm)) => {
                encode::alu_rm64_imm32(&mut self.buf, op, dst, imm as i32)
            }
            (dst, Operand::Imm8(imm)) => {
                encode::alu_rm64_imm32(&mut self.buf, op, dst, imm as i32)
            }
            (dst, Operand::Imm64(imm)) => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    encode::alu_rm64_imm32(&mut self.buf, op, dst, imm as i32);
                } else {
                    panic!("ALU r/m64, imm64 is not encodable");
                }
            }
            _ => panic!("invalid ALU operands"),
        }
    }

    pub fn add(&mut self, dst: impl IntoOp, src: impl IntoOp) {
        self.alu(AluOp::Add, dst, src);
    }

    pub fn sub(&mut self, dst: impl IntoOp, src: impl IntoOp) {
        self.alu(AluOp::Sub, dst, src);
    }

    pub fn and(&mut self, dst: impl IntoOp, src: impl IntoOp) {
        self.alu(AluOp::And, dst, src);
    }

    pub fn or(&mut self, dst: impl IntoOp, src: impl IntoOp) {
        self.alu(AluOp::Or, dst, src);
    }

    pub fn xor(&mut self, dst: impl IntoOp, src: impl IntoOp) {
        self.alu(AluOp::Xor, dst, src);
    }

    pub fn cmp(&mut self, a: impl IntoOp, b: impl IntoOp) {
        let a = a.into_op();
        let b = b.into_op();
        match (a, b) {
            (dst, Operand::Reg(src)) => encode::cmp_rm64_r64(&mut self.buf, dst, src),
            (dst, Operand::Imm32(imm)) => encode::cmp_rm64_imm32(&mut self.buf, dst, imm),
            (dst, Operand::Imm16(imm)) => {
                encode::cmp_rm64_imm32(&mut self.buf, dst, imm as i32)
            }
            (dst, Operand::Imm8(imm)) => {
                encode::cmp_rm64_imm32(&mut self.buf, dst, imm as i32)
            }
            (dst, Operand::Imm64(imm)) => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    encode::cmp_rm64_imm32(&mut self.buf, dst, imm as i32);
                } else {
                    panic!("CMP r/m64, imm64 is not encodable");
                }
            }
            _ => panic!("invalid CMP operands"),
        }
    }

    pub fn test(&mut self, a: impl IntoOp, b: impl IntoOp) {
        let a = a.into_op();
        let b = b.into_op();
        match (a, b) {
            (dst, Operand::Reg(src)) => encode::test_rm64_r64(&mut self.buf, dst, src),
            (dst, Operand::Imm32(imm)) => {
                encode::test_rm64_imm32(&mut self.buf, dst, imm)
            }
            (dst, Operand::Imm16(imm)) => {
                encode::test_rm64_imm32(&mut self.buf, dst, imm as i32)
            }
            (dst, Operand::Imm8(imm)) => {
                encode::test_rm64_imm32(&mut self.buf, dst, imm as i32)
            }
            (dst, Operand::Imm64(imm)) => {
                if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
                    encode::test_rm64_imm32(&mut self.buf, dst, imm as i32);
                } else {
                    panic!("TEST r/m64, imm64 is not encodable");
                }
            }
            _ => panic!("invalid TEST operands"),
        }
    }

    pub fn neg(&mut self, rm: impl IntoOp) {
        encode::neg(&mut self.buf, rm.into_op());
    }

    pub fn inc(&mut self, rm: impl IntoOp) {
        encode::inc(&mut self.buf, rm.into_op());
    }

    pub fn dec(&mut self, rm: impl IntoOp) {
        encode::dec(&mut self.buf, rm.into_op());
    }

    pub fn push(&mut self, r: Reg) {
        encode::push_reg64(&mut self.buf, r);
    }

    pub fn pop(&mut self, r: Reg) {
        encode::pop_reg64(&mut self.buf, r);
    }

    pub fn call(&mut self, target: impl IntoJmpTarget) {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::call_rel32(&mut self.buf, rel),
            JmpTarget::Label(l) => {
                let pc = self.buf.len();
                self.buf.push(0xE8);
                self.emit_rel32(l, pc + 5);
            }
        }
    }

    pub fn ret(&mut self) {
        encode::ret(&mut self.buf);
    }

    pub fn jmp(&mut self, target: impl IntoJmpTarget) {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::jmp_rel32(&mut self.buf, rel),
            JmpTarget::Label(l) => self.emit_jmp_label(l, None),
        }
    }

    pub fn je(&mut self, target: impl IntoJmpTarget) {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::jcc_rel32(&mut self.buf, Cond::E, rel),
            JmpTarget::Label(l) => self.emit_jmp_label(l, Some(Cond::E)),
        }
    }

    pub fn jne(&mut self, target: impl IntoJmpTarget) {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::jcc_rel32(&mut self.buf, Cond::Ne, rel),
            JmpTarget::Label(l) => self.emit_jmp_label(l, Some(Cond::Ne)),
        }
    }

    /// Generic conditional jump (any `Cond`).
    pub fn jcc(&mut self, cond: Cond, target: impl IntoJmpTarget) {
        match target.into_jmp_target() {
            JmpTarget::Rel32(rel) => encode::jcc_rel32(&mut self.buf, cond, rel),
            JmpTarget::Label(l) => self.emit_jmp_label(l, Some(cond)),
        }
    }

    pub fn setcc(&mut self, cond: Cond, dst: impl IntoOp) {
        encode::setcc(&mut self.buf, cond, dst.into_op());
    }

    pub fn syscall(&mut self) {
        encode::syscall(&mut self.buf);
    }

    pub fn lea(&mut self, dst: Reg, src: Mem) {
        encode::lea_r64_mem(&mut self.buf, dst, src);
    }

    /// `lea dst, [rip + label]`.  The displacement is patched when `label` is bound.
    pub fn lea_rip(&mut self, dst: Reg, label: Label) {
        let mut rex = encode::Rex::new(true);
        rex.r = dst.is_high();
        rex.emit(&mut self.buf);
        self.buf.push(0x8D);
        self.buf.push(encode::modrm(0b00, dst.enc(), 0b101));
        // displacement field starts at the next 4 bytes; pc is after it.
        let pc = self.buf.len() + 4;
        self.emit_rel32(label, pc);
    }

    pub fn cwd(&mut self) {
        encode::cwd(&mut self.buf);
    }

    pub fn cdq(&mut self) {
        encode::cdq(&mut self.buf);
    }

    pub fn cqo(&mut self) {
        encode::cqo(&mut self.buf);
    }

    pub fn imul(&mut self, dst: Reg, src: impl IntoOp) {
        encode::imul_r64_rm64(&mut self.buf, dst, src.into_op());
    }

    pub fn idiv(&mut self, rm: impl IntoOp) {
        encode::idiv(&mut self.buf, rm.into_op());
    }

    pub fn shl(&mut self, dst: impl IntoOp, imm: i8) {
        encode::shl_rm64_imm8(&mut self.buf, dst.into_op(), imm);
    }

    pub fn shl_cl(&mut self, dst: impl IntoOp) {
        encode::shl_rm64_cl(&mut self.buf, dst.into_op());
    }

    pub fn shr(&mut self, dst: impl IntoOp, imm: i8) {
        encode::shr_rm64_imm8(&mut self.buf, dst.into_op(), imm);
    }

    pub fn shr_cl(&mut self, dst: impl IntoOp) {
        encode::shr_rm64_cl(&mut self.buf, dst.into_op());
    }

    pub fn sar(&mut self, dst: impl IntoOp, imm: i8) {
        encode::sar_rm64_imm8(&mut self.buf, dst.into_op(), imm);
    }

    pub fn sar_cl(&mut self, dst: impl IntoOp) {
        encode::sar_rm64_cl(&mut self.buf, dst.into_op());
    }

    pub fn movzx8(&mut self, dst: Reg, src: impl IntoOp) {
        encode::movzx_r64_rm8(&mut self.buf, dst, src.into_op());
    }

    pub fn movsx8(&mut self, dst: Reg, src: impl IntoOp) {
        encode::movsx_r64_rm8(&mut self.buf, dst, src.into_op());
    }

    pub fn movsx16(&mut self, dst: Reg, src: impl IntoOp) {
        encode::movsx_r64_rm16(&mut self.buf, dst, src.into_op());
    }

    pub fn movsxd(&mut self, dst: Reg, src: impl IntoOp) {
        encode::movsxd_r64_rm32(&mut self.buf, dst, src.into_op());
    }

    pub fn mov32(&mut self, dst: Reg, src: impl IntoOp) {
        encode::mov_r32_rm32(&mut self.buf, dst, src.into_op());
    }

    pub fn movzx16(&mut self, dst: Reg, src: impl IntoOp) {
        encode::movzx_r64_rm16(&mut self.buf, dst, src.into_op());
    }

    pub fn store8(&mut self, dst: Mem, src: Reg) {
        encode::mov_rm8_r64(&mut self.buf, dst, src);
    }

    pub fn store16(&mut self, dst: Mem, src: Reg) {
        encode::mov_rm16_r64(&mut self.buf, dst, src);
    }

    pub fn store32(&mut self, dst: Mem, src: Reg) {
        encode::mov_rm32_r64(&mut self.buf, dst, src);
    }

    pub fn lfence(&mut self) {
        encode::lfence(&mut self.buf);
    }

    pub fn sfence(&mut self) {
        encode::sfence(&mut self.buf);
    }

    pub fn mfence(&mut self) {
        encode::mfence(&mut self.buf);
    }
}

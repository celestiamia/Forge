//! Core 32-bit x86 instruction encoding helpers: ModR/M, SIB, immediates,
//! and per-instruction encoders.

#![allow(dead_code)]

use crate::backend::x86::{AluOp, Cond, Inst, JmpTarget, Mem, Operand, Reg, Scale, ShiftOp};

pub fn emit_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

pub fn emit_i8(buf: &mut Vec<u8>, v: i8) {
    buf.push(v as u8);
}

pub fn emit_i32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub fn modrm(mod_bits: u8, reg: u8, rm: u8) -> u8 {
    ((mod_bits & 3) << 6) | ((reg & 7) << 3) | (rm & 7)
}

pub fn sib(scale: u8, index: u8, base: u8) -> u8 {
    ((scale & 3) << 6) | ((index & 7) << 3) | (base & 7)
}

fn encode_modrm_sib_disp(reg_op: u8, rm: Operand, out: &mut Vec<u8>) {
    match rm {
        Operand::Reg(r) => {
            out.push(modrm(0b11, reg_op, r.enc()));
        }
        Operand::Mem(mem) => encode_mem(reg_op, mem, out),
        _ => panic!("invalid r/m operand: {:?}", rm),
    }
}

pub fn modrm_suffix(reg_op: u8, rm: Operand) -> Vec<u8> {
    let mut out = Vec::new();
    encode_modrm_sib_disp(reg_op, rm, &mut out);
    out
}

fn encode_mem(reg_op: u8, mem: Mem, out: &mut Vec<u8>) {
    match mem {
        Mem::Disp32(disp) => {
            out.push(modrm(0b00, reg_op, 0b101));
            emit_i32(out, disp);
        }
        Mem::Base(base) => encode_base_mem(reg_op, base, None, Scale::One, 0, out),
        Mem::BaseDisp(base, disp) => {
            encode_base_mem(reg_op, base, None, Scale::One, disp, out)
        }
        Mem::BaseIndexScale(base, index, scale) => {
            encode_base_mem(reg_op, base, Some(index), scale, 0, out)
        }
        Mem::BaseIndexScaleDisp(base, index, scale, disp) => {
            encode_base_mem(reg_op, base, Some(index), scale, disp, out)
        }
    }
}

fn encode_base_mem(
    reg_op: u8,
    base: Reg,
    index: Option<Reg>,
    scale: Scale,
    disp: i32,
    out: &mut Vec<u8>,
) {
    let base_enc = base.enc();
    let index_enc = match index {
        None => 0b100,
        Some(r) => {
            if matches!(r, Reg::Esp) {
                panic!("ESP cannot be used as an index register");
            }
            r.enc()
        }
    };

    let base_is_ebp = base == Reg::Ebp;
    let needs_sib = index.is_some() || base_enc == 0b100;

    let mod_bits = if disp == 0 && !base_is_ebp {
        0b00
    } else if disp >= -128 && disp <= 127 {
        0b01
    } else {
        0b10
    };

    if needs_sib {
        out.push(modrm(mod_bits, reg_op, 0b100));
        out.push(sib(scale.bits(), index_enc, base_enc));
    } else {
        out.push(modrm(mod_bits, reg_op, base_enc));
    }

    match mod_bits {
        0b00 => {
            if base_is_ebp {
                // [ebp] with no displacement is encoded as disp32=0.
                emit_i32(out, 0);
            }
        }
        0b01 => out.push(disp as i8 as u8),
        0b10 => emit_i32(out, disp),
        _ => unreachable!(),
    }
}

// ----------------------------------------------------------------------------
// Per-instruction encoders
// ----------------------------------------------------------------------------

pub fn mov_rm32_r32(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    let suffix = modrm_suffix(src.enc(), dst);
    buf.push(0x89);
    buf.extend(suffix);
}

pub fn mov_r32_rm32(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let suffix = modrm_suffix(dst.enc(), src);
    buf.push(0x8B);
    buf.extend(suffix);
}

pub fn mov_rm32_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    let suffix = modrm_suffix(0, dst); // /0
    buf.push(0xC7);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

pub fn mov_r32_imm32(buf: &mut Vec<u8>, dst: Reg, imm: i32) {
    buf.push(0xB8 + dst.enc());
    emit_i32(buf, imm);
}

pub fn mov_rm8_r8(buf: &mut Vec<u8>, dst: Mem, src: Reg) {
    let suffix = modrm_suffix(src.enc(), Operand::Mem(dst));
    buf.push(0x88);
    buf.extend(suffix);
}

pub fn mov_rm16_r16(buf: &mut Vec<u8>, dst: Mem, src: Reg) {
    let suffix = modrm_suffix(src.enc(), Operand::Mem(dst));
    buf.push(0x66);
    buf.push(0x89);
    buf.extend(suffix);
}

pub fn mov_rm32_r32_store(buf: &mut Vec<u8>, dst: Mem, src: Reg) {
    let suffix = modrm_suffix(src.enc(), Operand::Mem(dst));
    buf.push(0x89);
    buf.extend(suffix);
}

pub fn alu_rm32_r32(buf: &mut Vec<u8>, op: AluOp, dst: Operand, src: Reg) {
    let opcode = match op {
        AluOp::Add => 0x01,
        AluOp::Sub => 0x29,
        AluOp::And => 0x21,
        AluOp::Or => 0x09,
        AluOp::Xor => 0x31,
    };
    let suffix = modrm_suffix(src.enc(), dst);
    buf.push(opcode);
    buf.extend(suffix);
}

/// Encode `op r32, r/m32` (register destination, register-or-memory source),
/// e.g. `add eax, [ebp+8]`. The `reg` field of ModR/M holds the destination
/// register and the `r/m` field holds the source.
pub fn alu_r32_rm32(buf: &mut Vec<u8>, op: AluOp, dst: Reg, src: Operand) {
    let opcode = match op {
        AluOp::Add => 0x03,
        AluOp::Sub => 0x2B,
        AluOp::And => 0x23,
        AluOp::Or => 0x0B,
        AluOp::Xor => 0x33,
    };
    let suffix = modrm_suffix(dst.enc(), src);
    buf.push(opcode);
    buf.extend(suffix);
}

pub fn alu_rm32_imm32(buf: &mut Vec<u8>, op: AluOp, dst: Operand, imm: i32) {
    let ext = match op {
        AluOp::Add => 0,
        AluOp::Or => 1,
        AluOp::And => 4,
        AluOp::Sub => 5,
        AluOp::Xor => 6,
    };
    let suffix = modrm_suffix(ext, dst);
    buf.push(0x81);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

pub fn cmp_rm32_r32(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    let suffix = modrm_suffix(src.enc(), dst);
    buf.push(0x39);
    buf.extend(suffix);
}

pub fn cmp_rm32_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    let suffix = modrm_suffix(7, dst); // /7
    buf.push(0x81);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

pub fn test_rm32_r32(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    let suffix = modrm_suffix(src.enc(), dst);
    buf.push(0x85);
    buf.extend(suffix);
}

pub fn test_rm32_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    let suffix = modrm_suffix(0, dst); // /0
    buf.push(0xF7);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

fn encode_unary(buf: &mut Vec<u8>, ext: u8, rm: Operand, opcode: u8) {
    let suffix = modrm_suffix(ext, rm);
    buf.push(opcode);
    buf.extend(suffix);
}

pub fn neg(buf: &mut Vec<u8>, rm: Operand) {
    encode_unary(buf, 3, rm, 0xF7);
}

pub fn inc(buf: &mut Vec<u8>, rm: Operand) {
    encode_unary(buf, 0, rm, 0xFF);
}

pub fn dec(buf: &mut Vec<u8>, rm: Operand) {
    encode_unary(buf, 1, rm, 0xFF);
}

pub fn push_reg(buf: &mut Vec<u8>, reg: Reg) {
    buf.push(0x50 + reg.enc());
}

pub fn push_imm32(buf: &mut Vec<u8>, imm: i32) {
    buf.push(0x68);
    emit_i32(buf, imm);
}

pub fn pop(buf: &mut Vec<u8>, reg: Reg) {
    buf.push(0x58 + reg.enc());
}

pub fn call_rel32(buf: &mut Vec<u8>, rel: i32) {
    buf.push(0xE8);
    emit_i32(buf, rel);
}

pub fn ret(buf: &mut Vec<u8>) {
    buf.push(0xC3);
}

pub fn jmp_rel32(buf: &mut Vec<u8>, rel: i32) {
    buf.push(0xE9);
    emit_i32(buf, rel);
}

pub fn jcc_rel32(buf: &mut Vec<u8>, cond: Cond, rel: i32) {
    buf.push(0x0F);
    buf.push(0x80 + cond.opcode_offset());
    emit_i32(buf, rel);
}

pub fn setcc(buf: &mut Vec<u8>, cond: Cond, dst: Operand) {
    let suffix = modrm_suffix(0, dst); // /0
    buf.push(0x0F);
    buf.push(0x90 + cond.opcode_offset());
    buf.extend(suffix);
}

pub fn syscall(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0x05]);
}

pub fn int(buf: &mut Vec<u8>, imm: u8) {
    buf.push(0xCD);
    buf.push(imm);
}

pub fn lea_r32_mem(buf: &mut Vec<u8>, dst: Reg, src: Mem) {
    let suffix = modrm_mem_suffix(dst.enc(), src);
    buf.push(0x8D);
    buf.extend(suffix);
}

fn modrm_mem_suffix(reg_op: u8, mem: Mem) -> Vec<u8> {
    let mut out = Vec::new();
    encode_mem(reg_op, mem, &mut out);
    out
}

pub fn cdq(buf: &mut Vec<u8>) {
    buf.push(0x99);
}

pub fn imul_r32_rm32(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let suffix = modrm_suffix(dst.enc(), src);
    buf.extend_from_slice(&[0x0F, 0xAF]);
    buf.extend(suffix);
}

pub fn idiv(buf: &mut Vec<u8>, rm: Operand) {
    encode_unary(buf, 7, rm, 0xF7);
}

pub fn div(buf: &mut Vec<u8>, rm: Operand) {
    encode_unary(buf, 6, rm, 0xF7);
}

fn encode_shift_rm32_imm8(buf: &mut Vec<u8>, op: ShiftOp, rm: Operand, imm: i8) {
    let ext = match op {
        ShiftOp::Shl => 4,
        ShiftOp::Shr => 5,
        ShiftOp::Sar => 7,
    };
    let suffix = modrm_suffix(ext, rm);
    buf.push(0xC1);
    buf.extend(suffix);
    emit_i8(buf, imm);
}

pub fn shl_rm32_imm8(buf: &mut Vec<u8>, rm: Operand, imm: i8) {
    encode_shift_rm32_imm8(buf, ShiftOp::Shl, rm, imm);
}

pub fn shr_rm32_imm8(buf: &mut Vec<u8>, rm: Operand, imm: i8) {
    encode_shift_rm32_imm8(buf, ShiftOp::Shr, rm, imm);
}

pub fn sar_rm32_imm8(buf: &mut Vec<u8>, rm: Operand, imm: i8) {
    encode_shift_rm32_imm8(buf, ShiftOp::Sar, rm, imm);
}

fn encode_shift_rm32_cl(buf: &mut Vec<u8>, op: ShiftOp, rm: Operand) {
    let ext = match op {
        ShiftOp::Shl => 4,
        ShiftOp::Shr => 5,
        ShiftOp::Sar => 7,
    };
    let suffix = modrm_suffix(ext, rm);
    buf.push(0xD3);
    buf.extend(suffix);
}

pub fn shl_rm32_cl(buf: &mut Vec<u8>, rm: Operand) {
    encode_shift_rm32_cl(buf, ShiftOp::Shl, rm);
}

pub fn shr_rm32_cl(buf: &mut Vec<u8>, rm: Operand) {
    encode_shift_rm32_cl(buf, ShiftOp::Shr, rm);
}

pub fn sar_rm32_cl(buf: &mut Vec<u8>, rm: Operand) {
    encode_shift_rm32_cl(buf, ShiftOp::Sar, rm);
}

pub fn movzx_r32_rm8(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let suffix = modrm_suffix(dst.enc(), src);
    buf.extend_from_slice(&[0x0F, 0xB6]);
    buf.extend(suffix);
}

pub fn movsx_r32_rm8(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let suffix = modrm_suffix(dst.enc(), src);
    buf.extend_from_slice(&[0x0F, 0xBE]);
    buf.extend(suffix);
}

pub fn movsx_r32_rm16(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let suffix = modrm_suffix(dst.enc(), src);
    buf.extend_from_slice(&[0x0F, 0xBF]);
    buf.extend(suffix);
}

pub fn movzx_r32_rm16(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let suffix = modrm_suffix(dst.enc(), src);
    buf.extend_from_slice(&[0x0F, 0xB7]);
    buf.extend(suffix);
}

pub fn lfence(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0xAE, 0xE8]);
}

pub fn sfence(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0xAE, 0xF8]);
}

pub fn mfence(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
}

pub fn rdtsc(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0x31]);
}

/// Encode a generic `Inst` to bytes.
pub fn encode_inst(buf: &mut Vec<u8>, inst: Inst) {
    match inst {
        Inst::MovRM32Imm32 { dst, imm } => mov_rm32_imm32(buf, dst, imm),
        Inst::MovRM32R32 { dst, src } => mov_rm32_r32(buf, dst, src),
        Inst::MovR32RM32 { dst, src } => mov_r32_rm32(buf, dst, src),
        Inst::MovR32Imm32 { dst, imm } => mov_r32_imm32(buf, dst, imm),
        Inst::AluRM32R32 { op, dst, src } => alu_rm32_r32(buf, op, dst, src),
        Inst::AluRM32Imm32 { op, dst, imm } => alu_rm32_imm32(buf, op, dst, imm),
        Inst::CmpRM32R32 { dst, src } => cmp_rm32_r32(buf, dst, src),
        Inst::CmpRM32Imm32 { dst, imm } => cmp_rm32_imm32(buf, dst, imm),
        Inst::TestRM32R32 { dst, src } => test_rm32_r32(buf, dst, src),
        Inst::TestRM32Imm32 { dst, imm } => test_rm32_imm32(buf, dst, imm),
        Inst::Neg(rm) => neg(buf, rm),
        Inst::Inc(rm) => inc(buf, rm),
        Inst::Dec(rm) => dec(buf, rm),
        Inst::PushReg(r) => push_reg(buf, r),
        Inst::PushImm32(imm) => push_imm32(buf, imm),
        Inst::Pop(r) => pop(buf, r),
        Inst::CallRel32(rel) => call_rel32(buf, rel),
        Inst::Ret => ret(buf),
        Inst::JmpRel32(rel) => jmp_rel32(buf, rel),
        Inst::Jcc { cond, target } => match target {
            JmpTarget::Rel32(rel) => jcc_rel32(buf, cond, rel),
            JmpTarget::Label(_) => panic!("Jcc with label must be emitted via Assembler"),
        },
        Inst::Setcc { cond, dst } => setcc(buf, cond, dst),
        Inst::Syscall => syscall(buf),
        Inst::Int(imm) => int(buf, imm),
        Inst::Lea { dst, src } => lea_r32_mem(buf, dst, src),
        Inst::Cdq => cdq(buf),
        Inst::ImulR32RM32 { dst, src } => imul_r32_rm32(buf, dst, src),
        Inst::Idiv(rm) => idiv(buf, rm),
        Inst::Div(rm) => div(buf, rm),
        Inst::ShiftRM32Imm8 { op, dst, imm } => encode_shift_rm32_imm8(buf, op, dst, imm),
        Inst::MovzxR32RM8 { dst, src } => movzx_r32_rm8(buf, dst, src),
        Inst::MovsxR32RM8 { dst, src } => movsx_r32_rm8(buf, dst, src),
        Inst::MovsxR32RM16 { dst, src } => movsx_r32_rm16(buf, dst, src),
        Inst::MovRM8R8 { dst, src } => mov_rm8_r8(buf, dst, src),
        Inst::MovRM16R16 { dst, src } => mov_rm16_r16(buf, dst, src),
        Inst::MovRM32R32Store { dst, src } => mov_rm32_r32_store(buf, dst, src),
        Inst::Rdtsc => rdtsc(buf),
    }
}

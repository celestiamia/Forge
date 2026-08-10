//! Core x86-64 instruction encoding helpers: REX, ModR/M, SIB, immediates,
//! and per-instruction encoders.

#![allow(dead_code)]

use crate::backend::x64::{AluOp, Cond, Inst, JmpTarget, Mem, Operand, Reg, Scale, ShiftOp};

pub fn emit_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

pub fn emit_i8(buf: &mut Vec<u8>, v: i8) {
    buf.push(v as u8);
}

pub fn emit_i32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub fn emit_i64(buf: &mut Vec<u8>, v: i64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[derive(Clone, Copy, Default)]
pub struct Rex {
    pub(crate) w: bool,
    pub(crate) r: bool,
    pub(crate) x: bool,
    pub(crate) b: bool,
    pub(crate) force: bool,
}

impl Rex {
    pub fn new(w: bool) -> Self {
        Self {
            w,
            r: false,
            x: false,
            b: false,
            force: false,
        }
    }

    pub fn emit(self, buf: &mut Vec<u8>) {
        let mut v = 0x40;
        if self.w {
            v |= 0x08;
        }
        if self.r {
            v |= 0x04;
        }
        if self.x {
            v |= 0x02;
        }
        if self.b {
            v |= 0x01;
        }
        if v != 0x40 || self.force {
            buf.push(v);
        }
    }
}

pub fn modrm(mod_bits: u8, reg: u8, rm: u8) -> u8 {
    ((mod_bits & 3) << 6) | ((reg & 7) << 3) | (rm & 7)
}

pub fn sib(scale: u8, index: u8, base: u8) -> u8 {
    ((scale & 3) << 6) | ((index & 7) << 3) | (base & 7)
}

fn encode_modrm_sib_disp(rex: &mut Rex, reg_op: u8, rm: Operand, out: &mut Vec<u8>, is_byte: bool) {
    match rm {
        Operand::Reg(r) => {
            if r.is_high() {
                rex.b = true;
            } else if is_byte && matches!(r, Reg::Rsp | Reg::Rbp | Reg::Rsi | Reg::Rdi) {
                // SPL, BPL, SIL, DIL require an empty REX prefix.
                rex.force = true;
            }
            out.push(modrm(0b11, reg_op, r.enc()));
        }
        Operand::Mem(mem) => encode_mem(rex, reg_op, mem, out),
        _ => panic!("invalid r/m operand: {:?}", rm),
    }
}

/// Returns the ModR/M+SIB+displacement bytes and updates `rex` with any X/B
/// bits required by the memory addressing.
pub fn modrm_suffix(rex: &mut Rex, reg_op: u8, rm: Operand) -> Vec<u8> {
    let mut out = Vec::new();
    encode_modrm_sib_disp(rex, reg_op, rm, &mut out, false);
    out
}

/// Same as `modrm_suffix` but for byte-sized r/m operands (SETcc/MOVZX/MOVSX 8-bit).
pub fn modrm_suffix_byte(rex: &mut Rex, reg_op: u8, rm: Operand) -> Vec<u8> {
    let mut out = Vec::new();
    encode_modrm_sib_disp(rex, reg_op, rm, &mut out, true);
    out
}

fn encode_mem(rex: &mut Rex, reg_op: u8, mem: Mem, out: &mut Vec<u8>) {
    match mem {
        Mem::Disp32(disp) => {
            // [disp32] requires SIB with base=101, index=100, mod=00.
            out.push(modrm(0b00, reg_op, 0b100));
            out.push(sib(0b00, 0b100, 0b101));
            emit_i32(out, disp);
        }
        Mem::Base(base) => encode_base_mem(rex, reg_op, base, None, Scale::One, 0, out),
        Mem::BaseDisp(base, disp) => {
            encode_base_mem(rex, reg_op, base, None, Scale::One, disp, out)
        }
        Mem::BaseIndexScale(base, index, scale) => {
            encode_base_mem(rex, reg_op, base, Some(index), scale, 0, out)
        }
        Mem::BaseIndexScaleDisp(base, index, scale, disp) => {
            encode_base_mem(rex, reg_op, base, Some(index), scale, disp, out)
        }
        Mem::RipRel(disp) => {
            out.push(modrm(0b00, reg_op, 0b101));
            emit_i32(out, disp);
        }
    }
}

fn encode_base_mem(
    rex: &mut Rex,
    reg_op: u8,
    base: Reg,
    index: Option<Reg>,
    scale: Scale,
    disp: i32,
    out: &mut Vec<u8>,
) {
    if base.is_high() {
        rex.b = true;
    }
    let base_enc = base.enc();

    let index_enc = match index {
        None => 0b100,
        Some(r) => {
            if matches!(r, Reg::Rsp | Reg::R12) {
                panic!("RSP/R12 cannot be used as an index register");
            }
            if r.is_high() {
                rex.x = true;
            }
            r.enc()
        }
    };

    let base_is_rbp_r13 = matches!(base, Reg::Rbp | Reg::R13);
    let needs_sib = index.is_some() || base_enc == 0b100;

    // RBP/R13 as a base cannot use mod=00; force mod=01 with zero disp8 when
    // the displacement would otherwise be zero.
    let mod_bits = if disp == 0 && !base_is_rbp_r13 {
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
            if base_is_rbp_r13 {
                // [rbp]/[r13] with no displacement is encoded as disp32=0.
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

pub fn mov_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    let mut rex = Rex::new(true);
    rex.r = src.is_high();
    let suffix = modrm_suffix(&mut rex, src.enc(), dst);
    rex.emit(buf);
    buf.push(0x89);
    buf.extend(suffix);
}

pub fn mov_r64_rm64(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.push(0x8B);
    buf.extend(suffix);
}

pub fn mov_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    let mut rex = Rex::new(true);
    let suffix = modrm_suffix(&mut rex, 0, dst); // /0
    rex.emit(buf);
    buf.push(0xC7);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

pub fn mov_r64_imm64(buf: &mut Vec<u8>, dst: Reg, imm: i64) {
    let mut rex = Rex::new(true);
    rex.b = dst.is_high();
    rex.emit(buf);
    buf.push(0xB8 + dst.enc());
    emit_i64(buf, imm);
}

/// `mov r8, r/m8` (sign-agnostic 8-bit store: source is the low byte of `src`).
pub fn mov_rm8_r64(buf: &mut Vec<u8>, dst: Mem, src: Reg) {
    let mut rex = Rex::new(false);
    rex.r = src.is_high();
    if matches!(src, Reg::Rsp | Reg::Rbp | Reg::Rsi | Reg::Rdi) {
        // SPL, BPL, SIL, DIL require an empty REX prefix to be distinguished
        // from AH/BH/CH/DH when used as the reg operand.
        rex.force = true;
    }
    let suffix = modrm_suffix_byte(&mut rex, src.enc(), Operand::Mem(dst));
    rex.emit(buf);
    buf.push(0x88);
    buf.extend(suffix);
}

/// `mov r/m16, r16` (16-bit store with operand-size prefix).
pub fn mov_rm16_r64(buf: &mut Vec<u8>, dst: Mem, src: Reg) {
    let mut rex = Rex::new(false);
    rex.r = src.is_high();
    let suffix = modrm_suffix(&mut rex, src.enc(), Operand::Mem(dst));
    rex.emit(buf);
    buf.push(0x66);
    buf.push(0x89);
    buf.extend(suffix);
}

/// `mov r/m32, r32` (32-bit store that ignores the upper 32 bits).
pub fn mov_rm32_r64(buf: &mut Vec<u8>, dst: Mem, src: Reg) {
    let mut rex = Rex::new(false);
    rex.r = src.is_high();
    let suffix = modrm_suffix(&mut rex, src.enc(), Operand::Mem(dst));
    rex.emit(buf);
    buf.push(0x89);
    buf.extend(suffix);
}

/// `mov r32, r/m32` (32-bit load that zero-extends to 64 bits).
pub fn mov_r32_rm32(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(false);
    rex.r = dst.is_high();
    let suffix = modrm_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.push(0x8B);
    buf.extend(suffix);
}

/// `movzx r64, r/m16` (16-bit zero-extending load).
pub fn movzx_r64_rm16(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.extend_from_slice(&[0x0F, 0xB7]);
    buf.extend(suffix);
}

/// Memory-fence instructions.
pub fn lfence(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0xAE, 0xE8]);
}
pub fn sfence(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0xAE, 0xF8]);
}
pub fn mfence(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0xAE, 0xF0]);
}

pub fn alu_rm64_r64(buf: &mut Vec<u8>, op: AluOp, dst: Operand, src: Reg) {
    let opcode = match op {
        AluOp::Add => 0x01,
        AluOp::Sub => 0x29,
        AluOp::And => 0x21,
        AluOp::Or => 0x09,
        AluOp::Xor => 0x31,
    };
    let mut rex = Rex::new(true);
    rex.r = src.is_high();
    let suffix = modrm_suffix(&mut rex, src.enc(), dst);
    rex.emit(buf);
    buf.push(opcode);
    buf.extend(suffix);
}

pub fn alu_rm64_imm32(buf: &mut Vec<u8>, op: AluOp, dst: Operand, imm: i32) {
    let ext = match op {
        AluOp::Add => 0,
        AluOp::Or => 1,
        AluOp::And => 4,
        AluOp::Sub => 5,
        AluOp::Xor => 6,
    };
    let mut rex = Rex::new(true);
    let suffix = modrm_suffix(&mut rex, ext, dst);
    rex.emit(buf);
    buf.push(0x81);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

pub fn add_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    alu_rm64_r64(buf, AluOp::Add, dst, src);
}
pub fn add_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    alu_rm64_imm32(buf, AluOp::Add, dst, imm);
}
pub fn sub_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    alu_rm64_r64(buf, AluOp::Sub, dst, src);
}
pub fn sub_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    alu_rm64_imm32(buf, AluOp::Sub, dst, imm);
}
pub fn and_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    alu_rm64_r64(buf, AluOp::And, dst, src);
}
pub fn and_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    alu_rm64_imm32(buf, AluOp::And, dst, imm);
}
pub fn or_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    alu_rm64_r64(buf, AluOp::Or, dst, src);
}
pub fn or_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    alu_rm64_imm32(buf, AluOp::Or, dst, imm);
}
pub fn xor_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    alu_rm64_r64(buf, AluOp::Xor, dst, src);
}
pub fn xor_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    alu_rm64_imm32(buf, AluOp::Xor, dst, imm);
}

pub fn cmp_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    let mut rex = Rex::new(true);
    rex.r = src.is_high();
    let suffix = modrm_suffix(&mut rex, src.enc(), dst);
    rex.emit(buf);
    buf.push(0x39);
    buf.extend(suffix);
}

pub fn cmp_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    let mut rex = Rex::new(true);
    let suffix = modrm_suffix(&mut rex, 7, dst); // /7
    rex.emit(buf);
    buf.push(0x81);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

pub fn test_rm64_r64(buf: &mut Vec<u8>, dst: Operand, src: Reg) {
    let mut rex = Rex::new(true);
    rex.r = src.is_high();
    let suffix = modrm_suffix(&mut rex, src.enc(), dst);
    rex.emit(buf);
    buf.push(0x85);
    buf.extend(suffix);
}

pub fn test_rm64_imm32(buf: &mut Vec<u8>, dst: Operand, imm: i32) {
    let mut rex = Rex::new(true);
    let suffix = modrm_suffix(&mut rex, 0, dst); // /0
    rex.emit(buf);
    buf.push(0xF7);
    buf.extend(suffix);
    emit_i32(buf, imm);
}

fn encode_unary(buf: &mut Vec<u8>, ext: u8, rm: Operand, opcode: u8) {
    let mut rex = Rex::new(true);
    let suffix = modrm_suffix(&mut rex, ext, rm);
    rex.emit(buf);
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

pub fn push_reg64(buf: &mut Vec<u8>, reg: Reg) {
    let mut rex = Rex::new(false);
    rex.b = reg.is_high();
    rex.emit(buf);
    buf.push(0x50 + reg.enc());
}

pub fn pop_reg64(buf: &mut Vec<u8>, reg: Reg) {
    let mut rex = Rex::new(false);
    rex.b = reg.is_high();
    rex.emit(buf);
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
    let mut rex = Rex::new(false);
    let suffix = modrm_suffix_byte(&mut rex, 0, dst); // /0
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x90 + cond.opcode_offset());
    buf.extend(suffix);
}

pub fn syscall(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x0F, 0x05]);
}

pub fn lea_r64_mem(buf: &mut Vec<u8>, dst: Reg, src: Mem) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_mem_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.push(0x8D);
    buf.extend(suffix);
}

fn modrm_mem_suffix(rex: &mut Rex, reg_op: u8, mem: Mem) -> Vec<u8> {
    let mut out = Vec::new();
    encode_mem(rex, reg_op, mem, &mut out);
    out
}

pub fn cwd(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x66, 0x99]);
}

pub fn cdq(buf: &mut Vec<u8>) {
    buf.push(0x99);
}

pub fn cqo(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0x48, 0x99]);
}

pub fn imul_r64_rm64(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.extend_from_slice(&[0x0F, 0xAF]);
    buf.extend(suffix);
}

pub fn idiv(buf: &mut Vec<u8>, rm: Operand) {
    encode_unary(buf, 7, rm, 0xF7);
}

pub fn udiv(buf: &mut Vec<u8>, rm: Operand) {
    encode_unary(buf, 6, rm, 0xF7);
}

fn encode_shift_rm64_imm8(buf: &mut Vec<u8>, op: ShiftOp, rm: Operand, imm: i8) {
    let ext = match op {
        ShiftOp::Shl => 4,
        ShiftOp::Shr => 5,
        ShiftOp::Sar => 7,
    };
    let mut rex = Rex::new(true);
    let suffix = modrm_suffix(&mut rex, ext, rm);
    rex.emit(buf);
    buf.push(0xC1);
    buf.extend(suffix);
    emit_i8(buf, imm);
}

pub fn shl_rm64_imm8(buf: &mut Vec<u8>, rm: Operand, imm: i8) {
    encode_shift_rm64_imm8(buf, ShiftOp::Shl, rm, imm);
}
pub fn shl_rm64_cl(buf: &mut Vec<u8>, rm: Operand) {
    encode_shift_rm64_cl(buf, ShiftOp::Shl, rm);
}
pub fn shr_rm64_imm8(buf: &mut Vec<u8>, rm: Operand, imm: i8) {
    encode_shift_rm64_imm8(buf, ShiftOp::Shr, rm, imm);
}
pub fn shr_rm64_cl(buf: &mut Vec<u8>, rm: Operand) {
    encode_shift_rm64_cl(buf, ShiftOp::Shr, rm);
}
pub fn sar_rm64_imm8(buf: &mut Vec<u8>, rm: Operand, imm: i8) {
    encode_shift_rm64_imm8(buf, ShiftOp::Sar, rm, imm);
}
pub fn sar_rm64_cl(buf: &mut Vec<u8>, rm: Operand) {
    encode_shift_rm64_cl(buf, ShiftOp::Sar, rm);
}

fn encode_shift_rm64_cl(buf: &mut Vec<u8>, op: ShiftOp, rm: Operand) {
    let ext = match op {
        ShiftOp::Shl => 4,
        ShiftOp::Shr => 5,
        ShiftOp::Sar => 7,
    };
    let mut rex = Rex::new(true);
    let suffix = modrm_suffix_byte(&mut rex, ext, rm);
    rex.emit(buf);
    buf.push(0xD3);
    buf.extend(suffix);
}

pub fn movzx_r64_rm8(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_suffix_byte(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.extend_from_slice(&[0x0F, 0xB6]);
    buf.extend(suffix);
}

pub fn movsx_r64_rm8(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_suffix_byte(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.extend_from_slice(&[0x0F, 0xBE]);
    buf.extend(suffix);
}

pub fn movsx_r64_rm16(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.extend_from_slice(&[0x0F, 0xBF]);
    buf.extend(suffix);
}

pub fn movsxd_r64_rm32(buf: &mut Vec<u8>, dst: Reg, src: Operand) {
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    let suffix = modrm_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.push(0x63);
    buf.extend(suffix);
}

// ----------------------------------------------------------------------------
// SSE scalar floating-point encoders
// ----------------------------------------------------------------------------

/// `movsd xmm, xmm` (double-precision move)
pub fn movsd_xmm_xmm(buf: &mut Vec<u8>, dst: Reg, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(false);
    rex.r = dst.is_high();
    rex.b = src.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x10);
    buf.push(modrm(0b11, dst.enc(), src.enc()));
}

/// `movsd xmm, m64` (load double from memory)
pub fn movsd_xmm_mem(buf: &mut Vec<u8>, dst: Reg, src: Mem) {
    buf.push(0xF2);
    let mut rex = Rex::new(false);
    rex.r = dst.is_high();
    let suffix = modrm_mem_suffix(&mut rex, dst.enc(), src);
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x10);
    buf.extend(suffix);
}

/// `movsd m64, xmm` (store double to memory)
pub fn movsd_mem_xmm(buf: &mut Vec<u8>, dst: Mem, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(false);
    rex.r = src.is_high();
    let suffix = modrm_mem_suffix(&mut rex, src.enc(), dst);
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x11);
    buf.extend(suffix);
}

/// `addsd xmm, xmm` (double-precision add)
pub fn addsd(buf: &mut Vec<u8>, dst: Reg, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(false);
    rex.r = dst.is_high();
    rex.b = src.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x58);
    buf.push(modrm(0b11, dst.enc(), src.enc()));
}

/// `subsd xmm, xmm` (double-precision subtract)
pub fn subsd(buf: &mut Vec<u8>, dst: Reg, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(false);
    rex.r = dst.is_high();
    rex.b = src.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x5C);
    buf.push(modrm(0b11, dst.enc(), src.enc()));
}

/// `mulsd xmm, xmm` (double-precision multiply)
pub fn mulsd(buf: &mut Vec<u8>, dst: Reg, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(false);
    rex.r = dst.is_high();
    rex.b = src.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x59);
    buf.push(modrm(0b11, dst.enc(), src.enc()));
}

/// `divsd xmm, xmm` (double-precision divide)
pub fn divsd(buf: &mut Vec<u8>, dst: Reg, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(false);
    rex.r = dst.is_high();
    rex.b = src.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x5E);
    buf.push(modrm(0b11, dst.enc(), src.enc()));
}

/// `cvtsi2sd xmm, r64` (convert signed int to double)
pub fn cvtsi2sd(buf: &mut Vec<u8>, dst: Reg, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    rex.b = src.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x2A);
    buf.push(modrm(0b11, dst.enc(), src.enc()));
}

/// `cvttsd2si r64, xmm` (convert double to signed int with truncation)
pub fn cvttsd2si(buf: &mut Vec<u8>, dst: Reg, src: Reg) {
    buf.push(0xF2);
    let mut rex = Rex::new(true);
    rex.r = dst.is_high();
    rex.b = src.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x2C);
    buf.push(modrm(0b11, dst.enc(), src.enc()));
}

/// `ucomisd xmm, xmm` (unordered compare double)
pub fn ucomisd(buf: &mut Vec<u8>, a: Reg, b: Reg) {
    buf.push(0x66);
    let mut rex = Rex::new(false);
    rex.r = a.is_high();
    rex.b = b.is_high();
    rex.emit(buf);
    buf.push(0x0F);
    buf.push(0x2E);
    buf.push(modrm(0b11, a.enc(), b.enc()));
}

/// Encode a generic `Inst` to bytes. This is the high-level dispatcher.
pub fn encode_inst(buf: &mut Vec<u8>, inst: Inst) {
    match inst {
        Inst::MovRM64Imm32 { dst, imm } => mov_rm64_imm32(buf, dst, imm),
        Inst::MovRM64R64 { dst, src } => mov_rm64_r64(buf, dst, src),
        Inst::MovR64RM64 { dst, src } => mov_r64_rm64(buf, dst, src),
        Inst::MovR64Imm64 { dst, imm } => mov_r64_imm64(buf, dst, imm),
        Inst::AluRM64R64 { op, dst, src } => alu_rm64_r64(buf, op, dst, src),
        Inst::AluRM64Imm32 { op, dst, imm } => alu_rm64_imm32(buf, op, dst, imm),
        Inst::CmpRM64R64 { dst, src } => cmp_rm64_r64(buf, dst, src),
        Inst::CmpRM64Imm32 { dst, imm } => cmp_rm64_imm32(buf, dst, imm),
        Inst::TestRM64R64 { dst, src } => test_rm64_r64(buf, dst, src),
        Inst::TestRM64Imm32 { dst, imm } => test_rm64_imm32(buf, dst, imm),
        Inst::Neg(rm) => neg(buf, rm),
        Inst::Inc(rm) => inc(buf, rm),
        Inst::Dec(rm) => dec(buf, rm),
        Inst::Push(r) => push_reg64(buf, r),
        Inst::Pop(r) => pop_reg64(buf, r),
        Inst::CallRel32(rel) => call_rel32(buf, rel),
        Inst::Ret => ret(buf),
        Inst::JmpRel32(rel) => jmp_rel32(buf, rel),
        Inst::Jcc { cond, target } => match target {
            JmpTarget::Rel32(rel) => jcc_rel32(buf, cond, rel),
            JmpTarget::Label(_) => panic!("Jcc with label must be emitted via Assembler"),
        },
        Inst::Setcc { cond, dst } => setcc(buf, cond, dst),
        Inst::Syscall => syscall(buf),
        Inst::Lea { dst, src } => lea_r64_mem(buf, dst, src),
        Inst::Cwd => cwd(buf),
        Inst::Cdq => cdq(buf),
        Inst::Cqo => cqo(buf),
        Inst::ImulR64RM64 { dst, src } => imul_r64_rm64(buf, dst, src),
        Inst::Idiv(rm) => idiv(buf, rm),
        Inst::ShiftRM64Imm8 { op, dst, imm } => encode_shift_rm64_imm8(buf, op, dst, imm),
        Inst::MovzxR64RM8 { dst, src } => movzx_r64_rm8(buf, dst, src),
        Inst::MovsxR64RM8 { dst, src } => movsx_r64_rm8(buf, dst, src),
        Inst::MovsxR64RM16 { dst, src } => movsx_r64_rm16(buf, dst, src),
        Inst::MovsxdR64RM32 { dst, src } => movsxd_r64_rm32(buf, dst, src),
    }
}

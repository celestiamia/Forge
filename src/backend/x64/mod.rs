//! Self-contained x86-64 instruction encoder.
//!
//! This module provides a register enum, operand/memory addressing helpers,
//! a raw instruction enum, low-level byte encoding routines, and an
//! `Assembler` builder that supports labels with forward/backward jump fixups.

mod encode;
mod reg;
mod inst;
mod asm;

pub use asm::Assembler;
pub use encode::encode_inst;
pub use inst::{
    AluOp, Cond, Inst, IntoJmpTarget, IntoOp, JmpTarget, Label, Mem, Operand, Scale, ShiftOp,
};
pub use reg::Reg;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mov_reg_imm32() {
        let mut a = Assembler::new();
        a.mov(Reg::Rax, 0x12345678i32);
        assert_eq!(a.bytes(), &[0x48, 0xC7, 0xC0, 0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn mov_reg_reg() {
        let mut a = Assembler::new();
        a.mov(Reg::Rcx, Reg::Rax);
        // REX.W 89 C1: mod=11 reg=rax(0) r/m=rcx(1)
        assert_eq!(a.bytes(), &[0x48, 0x89, 0xC1]);
    }

    #[test]
    fn mov_reg_mem_base() {
        let mut a = Assembler::new();
        a.mov(Reg::Rax, Mem::base(Reg::Rbx));
        // REX.W 8B 03: mod=00 reg=rax(0) r/m=rbx(3)
        assert_eq!(a.bytes(), &[0x48, 0x8B, 0x03]);
    }

    #[test]
    fn mov_mem_reg_disp() {
        let mut a = Assembler::new();
        a.mov(Mem::base_disp(Reg::Rax, 4), Reg::Rdx);
        // REX.W 89 50 04: mod=01 reg=rdx(2) r/m=rax(0), disp8=04
        assert_eq!(a.bytes(), &[0x48, 0x89, 0x50, 0x04]);
    }

    #[test]
    fn mov_high_reg_imm64() {
        let mut a = Assembler::new();
        a.mov(Reg::R8, 0x0000_0001_0000_0000u64);
        // 49 B8 00 00 00 00 01 00 00 00 (REX.B + B8+0, imm64)
        assert_eq!(
            a.bytes(),
            &[0x49, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn add_sub_cmp_imm() {
        let mut a = Assembler::new();
        a.add(Reg::Rax, 1i32);
        a.sub(Reg::Rax, 2i32);
        a.cmp(Reg::Rax, 3i32);
        // Encoder always uses the imm32 form of ALU/CMP instructions.
        assert_eq!(
            a.bytes(),
            &[
                0x48, 0x81, 0xC0, 0x01, 0x00, 0x00, 0x00, // add rax, 1
                0x48, 0x81, 0xE8, 0x02, 0x00, 0x00, 0x00, // sub rax, 2
                0x48, 0x81, 0xF8, 0x03, 0x00, 0x00, 0x00, // cmp rax, 3
            ]
        );
    }

    #[test]
    fn push_pop_call_ret_syscall() {
        let mut a = Assembler::new();
        a.push(Reg::Rbp);
        a.mov(Reg::Rbp, Reg::Rsp);
        a.call(0i32);
        a.pop(Reg::Rbp);
        a.ret();
        a.syscall();
        assert_eq!(
            a.bytes(),
            &[
                0x55, // push rbp
                0x48, 0x89, 0xE5, // mov rbp, rsp
                0xE8, 0x00, 0x00, 0x00, 0x00, // call rel32=0
                0x5D, // pop rbp
                0xC3, // ret
                0x0F, 0x05, // syscall
            ]
        );
    }

    #[test]
    fn lea_rip_relative() {
        let mut a = Assembler::new();
        a.lea(Reg::Rax, Mem::rip_rel(0));
        // REX.W 8D 05 00 00 00 00
        assert_eq!(a.bytes(), &[0x48, 0x8D, 0x05, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn lea_base_index_scale() {
        let mut a = Assembler::new();
        a.lea(Reg::Rax, Mem::base_index_scale(Reg::Rbx, Reg::Rcx, Scale::Four));
        // REX.W 8D 04 8B: mod=00 reg=rax(0) r/m=100 (SIB), SIB scale=10 idx=rcx(1) base=rbx(3)
        assert_eq!(a.bytes(), &[0x48, 0x8D, 0x04, 0x8B]);
    }

    #[test]
    fn jmp_forward_and_backward() {
        let mut a = Assembler::new();
        let start = a.label();
        a.mov(Reg::Rax, 0i32);
        let skip = a.new_label();
        a.jmp(skip);
        a.mov(Reg::Rax, 1i32);
        a.bind(skip);
        a.jmp(start);
        let bytes = a.into_bytes();
        // start at offset 0
        // mov rax, 0: 7 bytes (offsets 0..6)
        // jmp skip (forward): 5 bytes (offsets 7..11), target is after mov rax,1
        // mov rax, 1: 7 bytes (offsets 12..18)
        // skip bound at offset 19
        // jmp start (backward): 5 bytes (offsets 19..23)
        assert_eq!(
            bytes,
            &[
                0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00,
                0xE9, 0x07, 0x00, 0x00, 0x00,
                0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
                0xE9, 0xE8, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn je_jne_labels() {
        let mut a = Assembler::new();
        let l1 = a.new_label();
        a.cmp(Reg::Rax, Reg::Rbx);
        a.je(l1);
        a.mov(Reg::Rax, 0i32);
        a.bind(l1);
        a.jne(l1); // backward jump to same label
        let bytes = a.into_bytes();
        // cmp rax, rbx: 48 39 D8 (3 bytes, offsets 0..2)
        // je l1 forward: target = offset 16, pc = 9, rel = 7
        // mov rax,0: 7 bytes (offsets 9..15)
        // l1 bound at offset 16
        // jne l1 backward: target = 16, pc = 22, rel = -6 = 0xFA
        assert_eq!(
            bytes,
            &[
                0x48, 0x39, 0xD8,
                0x0F, 0x84, 0x07, 0x00, 0x00, 0x00,
                0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00,
                0x0F, 0x85, 0xFA, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn high_register_mov() {
        let mut a = Assembler::new();
        a.mov(Reg::R8, Reg::R9);
        // REX.W + REX.B + REX.R? dst=r8(b=1), src=r9(r=1). Actually mov r/m64, r64:
        // reg field = src(r9=1), r/m = dst(r8=0). REX.R=1, REX.B=1 -> 0x4D
        // 89 C8: mod=11 reg=001 r/m=000 -> C8
        assert_eq!(a.bytes(), &[0x4D, 0x89, 0xC8]);
    }

    #[test]
    fn setcc_al() {
        let mut a = Assembler::new();
        a.setcc(Cond::E, Reg::Rax.r8());
        // 0F 94 C0
        assert_eq!(a.bytes(), &[0x0F, 0x94, 0xC0]);
    }

    #[test]
    fn shift_imm8() {
        let mut a = Assembler::new();
        a.shl(Reg::Rax, 3);
        assert_eq!(a.bytes(), &[0x48, 0xC1, 0xE0, 0x03]);
    }

    #[test]
    fn encode_inst_round_trip() {
        let mut buf = Vec::new();
        encode_inst(
            &mut buf,
            Inst::MovRM64Imm32 {
                dst: Operand::Reg(Reg::Rcx),
                imm: 0xABCDEF00u32 as i32,
            },
        );
        assert_eq!(buf, &[0x48, 0xC7, 0xC1, 0x00, 0xEF, 0xCD, 0xAB]);
    }
}

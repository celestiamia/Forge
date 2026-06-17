//! Self-contained 32-bit x86 instruction encoder.
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
        a.mov(Reg::Eax, 0x12345678i32);
        // C7 C0 78 56 34 12
        assert_eq!(a.bytes(), &[0xC7, 0xC0, 0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn mov_mem_imm32() {
        let mut a = Assembler::new();
        a.mov(Mem::base(Reg::Eax), 0x12345678i32);
        // C7 00 78 56 34 12
        assert_eq!(a.bytes(), &[0xC7, 0x00, 0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn mov_reg_reg() {
        let mut a = Assembler::new();
        a.mov(Reg::Ecx, Reg::Eax);
        // 89 C1: mod=11 reg=eax(0) r/m=ecx(1)
        assert_eq!(a.bytes(), &[0x89, 0xC1]);
    }

    #[test]
    fn mov_reg_mem_base() {
        let mut a = Assembler::new();
        a.mov(Reg::Eax, Mem::base(Reg::Ebx));
        // 8B 03: mod=00 reg=eax(0) r/m=ebx(3)
        assert_eq!(a.bytes(), &[0x8B, 0x03]);
    }

    #[test]
    fn mov_mem_reg_disp() {
        let mut a = Assembler::new();
        a.mov(Mem::base_disp(Reg::Eax, 4), Reg::Edx);
        // 89 50 04: mod=01 reg=edx(2) r/m=eax(0), disp8=04
        assert_eq!(a.bytes(), &[0x89, 0x50, 0x04]);
    }

    #[test]
    fn add_sub_cmp_imm() {
        let mut a = Assembler::new();
        a.add(Reg::Eax, 1i32);
        a.sub(Reg::Eax, 2i32);
        a.cmp(Reg::Eax, 3i32);
        assert_eq!(
            a.bytes(),
            &[
                0x81, 0xC0, 0x01, 0x00, 0x00, 0x00, // add eax, 1
                0x81, 0xE8, 0x02, 0x00, 0x00, 0x00, // sub eax, 2
                0x81, 0xF8, 0x03, 0x00, 0x00, 0x00, // cmp eax, 3
            ]
        );
    }

    #[test]
    fn push_pop_call_ret_syscall() {
        let mut a = Assembler::new();
        a.push(Reg::Ebp);
        a.mov(Reg::Ebp, Reg::Esp);
        a.call(0i32);
        a.pop(Reg::Ebp);
        a.ret();
        a.syscall();
        assert_eq!(
            a.bytes(),
            &[
                0x55, // push ebp
                0x89, 0xE5, // mov ebp, esp
                0xE8, 0x00, 0x00, 0x00, 0x00, // call rel32=0
                0x5D, // pop ebp
                0xC3, // ret
                0x0F, 0x05, // syscall
            ]
        );
    }

    #[test]
    fn lea_base_disp() {
        let mut a = Assembler::new();
        a.lea(Reg::Eax, Mem::base_disp(Reg::Ebp, 8));
        // 8D 45 08
        assert_eq!(a.bytes(), &[0x8D, 0x45, 0x08]);
    }

    #[test]
    fn jmp_forward_and_backward() {
        let mut a = Assembler::new();
        let start = a.label();
        a.mov(Reg::Eax, 0i32);
        let skip = a.new_label();
        a.jmp(skip);
        a.mov(Reg::Eax, 1i32);
        a.bind(skip);
        a.jmp(start);
        let bytes = a.into_bytes();
        // start at offset 0
        // mov eax, 0: 6 bytes (offsets 0..5)
        // jmp skip (forward): 5 bytes (offsets 6..10), target is after mov eax,1
        // mov eax, 1: 6 bytes (offsets 11..16)
        // skip bound at offset 17
        // jmp start (backward): 5 bytes (offsets 17..21)
        assert_eq!(
            bytes,
            &[
                0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00,
                0xE9, 0x06, 0x00, 0x00, 0x00,
                0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
                0xE9, 0xEA, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn je_jne_labels() {
        let mut a = Assembler::new();
        let l1 = a.new_label();
        a.cmp(Reg::Eax, Reg::Ebx);
        a.je(l1);
        a.mov(Reg::Eax, 0i32);
        a.bind(l1);
        a.jne(l1);
        let bytes = a.into_bytes();
        // cmp eax, ebx: 39 C3 (2 bytes)
        // je l1 forward: target = offset 14, pc = 8, rel = 6
        // mov eax,0: 6 bytes (offsets 8..13)
        // l1 bound at offset 14
        // jne l1 backward: target = 14, pc = 20, rel = -6 = 0xFA
        assert_eq!(
            bytes,
            &[
                0x39, 0xD8,
                0x0F, 0x84, 0x06, 0x00, 0x00, 0x00,
                0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00,
                0x0F, 0x85, 0xFA, 0xFF, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn setcc_al() {
        let mut a = Assembler::new();
        a.setcc(Cond::E, Reg::Eax.r8());
        assert_eq!(a.bytes(), &[0x0F, 0x94, 0xC0]);
    }

    #[test]
    fn shift_imm8() {
        let mut a = Assembler::new();
        a.shl(Reg::Eax, 3);
        assert_eq!(a.bytes(), &[0xC1, 0xE0, 0x03]);
    }

    #[test]
    fn encode_inst_round_trip() {
        let mut buf = Vec::new();
        encode_inst(
            &mut buf,
            Inst::MovRM32Imm32 {
                dst: Operand::Reg(Reg::Ecx),
                imm: 0xABCDEF00u32 as i32,
            },
        );
        assert_eq!(buf, &[0xC7, 0xC1, 0x00, 0xEF, 0xCD, 0xAB]);
    }

    #[test]
    fn int_imm8() {
        let mut a = Assembler::new();
        a.int(0x80u8);
        assert_eq!(a.bytes(), &[0xCD, 0x80]);
    }

    #[test]
    fn rdtsc_encoding() {
        let mut a = Assembler::new();
        a.rdtsc();
        assert_eq!(a.bytes(), &[0x0F, 0x31]);
    }
}

//! X86-64 register definitions and subregister helpers.

/// A general-purpose or special x86-64 register.
///
/// The enum represents the physical register. Subregister helpers (`r8`, `r16`,
/// `r32`, `r64`) return the same register and are intended as semantic markers
/// in the assembler API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reg {
    Rax,
    Rcx,
    Rdx,
    Rbx,
    Rsp,
    Rbp,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    Rip,
    Xmm0,
    Xmm1,
    Xmm2,
    Xmm3,
    Xmm4,
    Xmm5,
    Xmm6,
    Xmm7,
}

impl Reg {
    /// 3-bit register encoding used in ModR/M, SIB, and opcode register fields.
    pub fn enc(self) -> u8 {
        match self {
            Reg::Rax | Reg::R8 | Reg::Xmm0 => 0,
            Reg::Rcx | Reg::R9 | Reg::Xmm1 => 1,
            Reg::Rdx | Reg::R10 | Reg::Xmm2 => 2,
            Reg::Rbx | Reg::R11 | Reg::Xmm3 => 3,
            Reg::Rsp | Reg::R12 | Reg::Xmm4 => 4,
            Reg::Rbp | Reg::R13 | Reg::Xmm5 => 5,
            Reg::Rsi | Reg::R14 | Reg::Xmm6 => 6,
            Reg::Rdi | Reg::R15 | Reg::Xmm7 => 7,
            Reg::Rip => 5, // RIP-relative uses r/m == 101, handled specially in memory encoding.
        }
    }

    /// True for R8..R15 and XMM8..XMM15 (we only have XMM0..XMM7 here).
    pub fn is_high(self) -> bool {
        matches!(
            self,
            Reg::R8 | Reg::R9 | Reg::R10 | Reg::R11 | Reg::R12 | Reg::R13 | Reg::R14 | Reg::R15
        )
    }

    /// True if this is an XMM register.
    pub fn is_xmm(self) -> bool {
        matches!(
            self,
            Reg::Xmm0
                | Reg::Xmm1
                | Reg::Xmm2
                | Reg::Xmm3
                | Reg::Xmm4
                | Reg::Xmm5
                | Reg::Xmm6
                | Reg::Xmm7
        )
    }

    pub fn is_rip(self) -> bool {
        self == Reg::Rip
    }

    // Subregister helpers: they return the same physical register. The operand
    // size is determined by the instruction that consumes the register.
    pub fn r64(self) -> Self {
        self
    }
    pub fn r32(self) -> Self {
        self
    }
    pub fn r16(self) -> Self {
        self
    }
    pub fn r8(self) -> Self {
        self
    }

    // Named constructors for convenience.
    pub const RAX: Self = Reg::Rax;
    pub const RCX: Self = Reg::Rcx;
    pub const RDX: Self = Reg::Rdx;
    pub const RBX: Self = Reg::Rbx;
    pub const RSP: Self = Reg::Rsp;
    pub const RBP: Self = Reg::Rbp;
    pub const RSI: Self = Reg::Rsi;
    pub const RDI: Self = Reg::Rdi;
    pub const R8_: Self = Reg::R8;
    pub const R9_: Self = Reg::R9;
    pub const R10_: Self = Reg::R10;
    pub const R11_: Self = Reg::R11;
    pub const R12_: Self = Reg::R12;
    pub const R13_: Self = Reg::R13;
    pub const R14_: Self = Reg::R14;
    pub const R15_: Self = Reg::R15;
    pub const RIP: Self = Reg::Rip;
    pub const XMM0: Self = Reg::Xmm0;
    pub const XMM1: Self = Reg::Xmm1;
    pub const XMM2: Self = Reg::Xmm2;
    pub const XMM3: Self = Reg::Xmm3;
    pub const XMM4: Self = Reg::Xmm4;
    pub const XMM5: Self = Reg::Xmm5;
    pub const XMM6: Self = Reg::Xmm6;
    pub const XMM7: Self = Reg::Xmm7;
}

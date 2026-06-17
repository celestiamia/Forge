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
}

impl Reg {
    /// 3-bit register encoding used in ModR/M, SIB, and opcode register fields.
    pub fn enc(self) -> u8 {
        match self {
            Reg::Rax | Reg::R8 => 0,
            Reg::Rcx | Reg::R9 => 1,
            Reg::Rdx | Reg::R10 => 2,
            Reg::Rbx | Reg::R11 => 3,
            Reg::Rsp | Reg::R12 => 4,
            Reg::Rbp | Reg::R13 => 5,
            Reg::Rsi | Reg::R14 => 6,
            Reg::Rdi | Reg::R15 => 7,
            Reg::Rip => 5, // RIP-relative uses r/m == 101, handled specially in memory encoding.
        }
    }

    /// True for R8..R15, which require a REX prefix bit.
    pub fn is_high(self) -> bool {
        matches!(
            self,
            Reg::R8
                | Reg::R9
                | Reg::R10
                | Reg::R11
                | Reg::R12
                | Reg::R13
                | Reg::R14
                | Reg::R15
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

}

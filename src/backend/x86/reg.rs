//! 32-bit x86 register definitions.

/// A general-purpose 32-bit x86 register.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Reg {
    Eax,
    Ecx,
    Edx,
    Ebx,
    Esp,
    Ebp,
    Esi,
    Edi,
}

#[allow(dead_code)]
impl Reg {
    /// 3-bit register encoding used in ModR/M, SIB, and opcode register fields.
    pub fn enc(self) -> u8 {
        match self {
            Reg::Eax => 0,
            Reg::Ecx => 1,
            Reg::Edx => 2,
            Reg::Ebx => 3,
            Reg::Esp => 4,
            Reg::Ebp => 5,
            Reg::Esi => 6,
            Reg::Edi => 7,
        }
    }

    /// The low 8-bit subregister.
    pub fn r8(self) -> Self {
        self
    }

    /// The low 16-bit subregister.
    pub fn r16(self) -> Self {
        self
    }

    /// The 32-bit register itself.
    pub fn r32(self) -> Self {
        self
    }

    pub const EAX: Self = Reg::Eax;
    pub const ECX: Self = Reg::Ecx;
    pub const EDX: Self = Reg::Edx;
    pub const EBX: Self = Reg::Ebx;
    pub const ESP: Self = Reg::Esp;
    pub const EBP: Self = Reg::Ebp;
    pub const ESI: Self = Reg::Esi;
    pub const EDI: Self = Reg::Edi;
}

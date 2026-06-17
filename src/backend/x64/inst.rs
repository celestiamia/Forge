//! Instruction and operand representations.

use crate::backend::x64::Reg;

/// Scale factor for indexed memory addressing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    One = 1,
    Two = 2,
    Four = 4,
    Eight = 8,
}

impl Scale {
    pub fn bits(self) -> u8 {
        match self {
            Scale::One => 0b00,
            Scale::Two => 0b01,
            Scale::Four => 0b10,
            Scale::Eight => 0b11,
        }
    }
}

/// Memory addressing modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mem {
    /// `[disp32]` (absolute 32-bit displacement, no base/index).
    Disp32(i32),
    /// `[base]`
    Base(Reg),
    /// `[base + disp]`
    BaseDisp(Reg, i32),
    /// `[base + index*scale]`
    BaseIndexScale(Reg, Reg, Scale),
    /// `[base + index*scale + disp]`
    BaseIndexScaleDisp(Reg, Reg, Scale, i32),
    /// `[rip + disp32]`
    RipRel(i32),
}

impl Mem {
    pub fn disp32(disp: i32) -> Self {
        Mem::Disp32(disp)
    }
    pub fn base(r: Reg) -> Self {
        Mem::Base(r)
    }
    pub fn base_disp(r: Reg, d: i32) -> Self {
        Mem::BaseDisp(r, d)
    }
    pub fn base_index_scale(b: Reg, i: Reg, s: Scale) -> Self {
        Mem::BaseIndexScale(b, i, s)
    }
    pub fn base_index_scale_disp(b: Reg, i: Reg, s: Scale, d: i32) -> Self {
        Mem::BaseIndexScaleDisp(b, i, s, d)
    }
    pub fn rip_rel(disp: i32) -> Self {
        Mem::RipRel(disp)
    }
}

/// An operand: register, memory location, or immediate of various sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    Reg(Reg),
    Mem(Mem),
    Imm8(i8),
    Imm16(i16),
    Imm32(i32),
    Imm64(i64),
}

/// Trait for converting common Rust values and register/memory expressions
/// into operands.
pub trait IntoOp {
    fn into_op(self) -> Operand;
}

impl IntoOp for Reg {
    fn into_op(self) -> Operand {
        Operand::Reg(self)
    }
}

impl IntoOp for Mem {
    fn into_op(self) -> Operand {
        Operand::Mem(self)
    }
}

impl IntoOp for i8 {
    fn into_op(self) -> Operand {
        Operand::Imm8(self)
    }
}
impl IntoOp for i16 {
    fn into_op(self) -> Operand {
        Operand::Imm16(self)
    }
}
impl IntoOp for i32 {
    fn into_op(self) -> Operand {
        Operand::Imm32(self)
    }
}
impl IntoOp for i64 {
    fn into_op(self) -> Operand {
        Operand::Imm64(self)
    }
}
impl IntoOp for u8 {
    fn into_op(self) -> Operand {
        Operand::Imm8(self as i8)
    }
}
impl IntoOp for u16 {
    fn into_op(self) -> Operand {
        Operand::Imm16(self as i16)
    }
}
impl IntoOp for u32 {
    fn into_op(self) -> Operand {
        Operand::Imm32(self as i32)
    }
}
impl IntoOp for u64 {
    fn into_op(self) -> Operand {
        Operand::Imm64(self as i64)
    }
}
impl IntoOp for usize {
    fn into_op(self) -> Operand {
        Operand::Imm64(self as i64)
    }
}

/// Condition codes for Jcc and SETcc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    O,
    No,
    B,
    Ae,
    E,
    Ne,
    Be,
    A,
    S,
    Ns,
    P,
    Np,
    L,
    Ge,
    Le,
    G,
}

impl Cond {
    /// Secondary opcode byte offset added to 0x80 for Jcc/SETcc.
    pub fn opcode_offset(self) -> u8 {
        match self {
            Cond::O => 0x0,
            Cond::No => 0x1,
            Cond::B => 0x2,
            Cond::Ae => 0x3,
            Cond::E => 0x4,
            Cond::Ne => 0x5,
            Cond::Be => 0x6,
            Cond::A => 0x7,
            Cond::S => 0x8,
            Cond::Ns => 0x9,
            Cond::P => 0xA,
            Cond::Np => 0xB,
            Cond::L => 0xC,
            Cond::Ge => 0xD,
            Cond::Le => 0xE,
            Cond::G => 0xF,
        }
    }
}

/// Label identifier used by the assembler for jump/call fixups.
pub type Label = u32;

/// Target for control-flow instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JmpTarget {
    Rel32(i32),
    Label(Label),
}

pub trait IntoJmpTarget {
    fn into_jmp_target(self) -> JmpTarget;
}

impl IntoJmpTarget for Label {
    fn into_jmp_target(self) -> JmpTarget {
        JmpTarget::Label(self)
    }
}

impl IntoJmpTarget for i32 {
    fn into_jmp_target(self) -> JmpTarget {
        JmpTarget::Rel32(self)
    }
}

/// ALU operation kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AluOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
}

/// Shift operation kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShiftOp {
    Shl,
    Shr,
    Sar,
}

/// High-level instruction representation. This enum covers the operations
/// required by the x64 backend; `encode::encode_inst` lowers it to bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inst {
    MovRM64Imm32 { dst: Operand, imm: i32 },
    MovRM64R64 { dst: Operand, src: Reg },
    MovR64RM64 { dst: Reg, src: Operand },
    MovR64Imm64 { dst: Reg, imm: i64 },

    AluRM64R64 { op: AluOp, dst: Operand, src: Reg },
    AluRM64Imm32 { op: AluOp, dst: Operand, imm: i32 },

    CmpRM64R64 { dst: Operand, src: Reg },
    CmpRM64Imm32 { dst: Operand, imm: i32 },

    TestRM64R64 { dst: Operand, src: Reg },
    TestRM64Imm32 { dst: Operand, imm: i32 },

    Neg(Operand),
    Inc(Operand),
    Dec(Operand),

    Push(Reg),
    Pop(Reg),

    CallRel32(i32),
    Ret,
    JmpRel32(i32),
    Jcc { cond: Cond, target: JmpTarget },
    Setcc { cond: Cond, dst: Operand },

    Syscall,

    Lea { dst: Reg, src: Mem },

    Cwd,
    Cdq,
    Cqo,

    ImulR64RM64 { dst: Reg, src: Operand },
    Idiv(Operand),

    ShiftRM64Imm8 { op: ShiftOp, dst: Operand, imm: i8 },

    MovzxR64RM8 { dst: Reg, src: Operand },
    MovsxR64RM8 { dst: Reg, src: Operand },
    MovsxR64RM16 { dst: Reg, src: Operand },
    MovsxdR64RM32 { dst: Reg, src: Operand },
}

//! Instruction and operand representations for 32-bit x86.

use crate::backend::x86::Reg;

/// Scale factor for indexed memory addressing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
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

/// Memory addressing modes for 32-bit x86.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
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
}

#[allow(dead_code)]
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

/// Condition codes for Jcc and SETcc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
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

/// High-level instruction representation for 32-bit x86.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Inst {
    MovRM32Imm32 { dst: Operand, imm: i32 },
    MovRM32R32 { dst: Operand, src: Reg },
    MovR32RM32 { dst: Reg, src: Operand },
    MovR32Imm32 { dst: Reg, imm: i32 },

    AluRM32R32 { op: AluOp, dst: Operand, src: Reg },
    AluRM32Imm32 { op: AluOp, dst: Operand, imm: i32 },

    CmpRM32R32 { dst: Operand, src: Reg },
    CmpRM32Imm32 { dst: Operand, imm: i32 },

    TestRM32R32 { dst: Operand, src: Reg },
    TestRM32Imm32 { dst: Operand, imm: i32 },

    Neg(Operand),
    Inc(Operand),
    Dec(Operand),

    PushReg(Reg),
    PushImm32(i32),
    Pop(Reg),

    CallRel32(i32),
    Ret,
    JmpRel32(i32),
    Jcc { cond: Cond, target: JmpTarget },
    Setcc { cond: Cond, dst: Operand },

    Syscall,
    Int(u8),

    Lea { dst: Reg, src: Mem },

    Cdq,

    ImulR32RM32 { dst: Reg, src: Operand },
    Idiv(Operand),
    Div(Operand),

    ShiftRM32Imm8 { op: ShiftOp, dst: Operand, imm: i8 },

    MovzxR32RM8 { dst: Reg, src: Operand },
    MovsxR32RM8 { dst: Reg, src: Operand },
    MovsxR32RM16 { dst: Reg, src: Operand },

    MovRM8R8 { dst: Mem, src: Reg },
    MovRM16R16 { dst: Mem, src: Reg },
    MovRM32R32Store { dst: Mem, src: Reg },

    Rdtsc,
}

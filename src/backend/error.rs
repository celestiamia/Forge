//! Common error types for the Forge compiler backend.
//!
//! These errors are designed to wrap cleanly in `anyhow::Error` for propagation
//! through the compiler pipeline while providing structured, actionable diagnostics.

use crate::backend::ir::{BinOp, Type};
use crate::backend::x64::{
    JmpTarget as JmpTarget64, Label as Label64, Mem as Mem64, Operand as Operand64,
};
use crate::backend::x86::{JmpTarget as JmpTarget32, Mem as Mem32, Operand as Operand32};
use thiserror::Error;

/// Errors that can occur during instruction encoding.
#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("invalid r/m operand: {operand}")]
    InvalidRmOperand { operand: String },

    #[error("register {reg} cannot be used as an index register")]
    InvalidIndexRegister { reg: String },

    #[error("unsupported operand combination for {instruction}: dst={dst}, src={src}")]
    InvalidOperands {
        instruction: &'static str,
        dst: String,
        src: String,
    },

    #[error("immediate value {value} too large for {instruction} with memory destination")]
    ImmTooLarge {
        instruction: &'static str,
        value: i64,
    },

    #[error("jump with label must be emitted via Assembler, not raw encoder")]
    LabelJumpInEncoder,

    #[error("invalid mod_bits value: {mod_bits} (expected 00, 01, or 10)")]
    InvalidModBits { mod_bits: u8 },
}

/// Errors that can occur during assembly.
#[derive(Debug, Error)]
pub enum AssemblerError {
    #[error("assembler finished with {count} unresolved labels: {labels}")]
    UnresolvedLabels { count: usize, labels: String },
}

/// Errors that can occur during code generation.
#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("unhandled binary operation: {op:?}")]
    UnhandledBinOp { op: BinOp },

    #[error("unhandled slot width: {width}")]
    UnhandledSlotWidth { width: u8 },

    #[error("missing label offset for label {label}")]
    MissingLabelOffset { label: u32 },

    #[error("missing global label for variable: {name}")]
    MissingGlobalLabel { name: String },

    #[error("missing rodata label")]
    MissingRodataLabel,

    #[error("unsupported type for codegen: {ty:?}")]
    UnsupportedType { ty: Type },

    #[error("function {name} has more than 6 arguments (not yet supported)")]
    TooManyArguments { name: String },

    #[error("inline assembly not implemented for this target")]
    AsmNotImplemented,

    #[error("floating point not implemented for this target")]
    FloatNotImplemented,
}

/// Errors related to IR type misuse.
#[derive(Debug, Error)]
pub enum IRTypeError {
    #[error("width_bits called on non-scalar type: {ty:?}")]
    NonScalarWidthBits { ty: Type },
}

/// Errors that can occur during object file writing.
#[derive(Debug, Error)]
pub enum ObjectWriteError {
    #[error("missing label offset for start label {label}")]
    MissingStartLabel { label: u32 },

    #[error("missing label offset for rodata start label {label}")]
    MissingRodataStartLabel { label: u32 },

    #[error("missing global label for variable: {name}")]
    MissingGlobalLabel { name: String },
}

/// Format a 64-bit operand for error messages.
pub(super) fn fmt_operand64(op: &Operand64) -> String {
    match op {
        Operand64::Reg(r) => format!("Reg({:?})", r),
        Operand64::Mem(m) => format!("Mem({})", fmt_mem64(m)),
        Operand64::Imm8(v) => format!("Imm8({})", v),
        Operand64::Imm16(v) => format!("Imm16({})", v),
        Operand64::Imm32(v) => format!("Imm32({})", v),
        Operand64::Imm64(v) => format!("Imm64({})", v),
    }
}

/// Format a 32-bit operand for error messages.
pub(super) fn fmt_operand32(op: &Operand32) -> String {
    match op {
        Operand32::Reg(r) => format!("Reg({:?})", r),
        Operand32::Mem(m) => format!("Mem({})", fmt_mem32(m)),
        Operand32::Imm8(v) => format!("Imm8({})", v),
        Operand32::Imm16(v) => format!("Imm16({})", v),
        Operand32::Imm32(v) => format!("Imm32({})", v),
        Operand32::Imm64(v) => format!("Imm64({})", v),
    }
}

/// Format a 64-bit memory operand for error messages.
pub(super) fn fmt_mem64(m: &Mem64) -> String {
    match m {
        Mem64::Disp32(d) => format!("[disp32={}]", d),
        Mem64::Base(r) => format!("[{:?}]", r),
        Mem64::BaseDisp(r, d) => format!("[{:?}+{}]", r, d),
        Mem64::BaseIndexScale(b, i, s) => format!("[{:?}+{:?}*{:?}]", b, i, s),
        Mem64::BaseIndexScaleDisp(b, i, s, d) => format!("[{:?}+{:?}*{:?}+{}]", b, i, s, d),
        Mem64::RipRel(d) => format!("[rip+{}]", d),
    }
}

/// Format a 32-bit memory operand for error messages.
pub(super) fn fmt_mem32(m: &Mem32) -> String {
    match m {
        Mem32::Disp32(d) => format!("[disp32={}]", d),
        Mem32::Base(r) => format!("[{:?}]", r),
        Mem32::BaseDisp(r, d) => format!("[{:?}+{}]", r, d),
        Mem32::BaseIndexScale(b, i, s) => format!("[{:?}+{:?}*{:?}]", b, i, s),
        Mem32::BaseIndexScaleDisp(b, i, s, d) => format!("[{:?}+{:?}*{:?}+{}]", b, i, s, d),
    }
}

/// Format a 64-bit JmpTarget for error messages.
pub(super) fn fmt_jmp_target64(t: &JmpTarget64) -> String {
    match t {
        JmpTarget64::Rel32(v) => format!("Rel32({})", v),
        JmpTarget64::Label(l) => format!("Label(L{})", l),
    }
}

/// Format a 32-bit JmpTarget for error messages.
pub(super) fn fmt_jmp_target32(t: &JmpTarget32) -> String {
    match t {
        JmpTarget32::Rel32(v) => format!("Rel32({})", v),
        JmpTarget32::Label(l) => format!("Label(L{})", l),
    }
}

/// Format a list of labels for unresolved labels error.
pub(super) fn fmt_labels(labels: &[Label64]) -> String {
    let mut s = String::new();
    s.push('[');
    for (i, l) in labels.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("L{}", l));
    }
    s.push(']');
    s
}

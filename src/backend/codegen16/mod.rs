//! 16-bit real-mode code cgerator for bare-metal boot targets.
//!
//! This backend consumes the same typed IR as the x86-64 backend and emits
//! flat 16-bit x86 machine code suitable for a PC boot sector.  It supports a
//! small but complete subset of Forge: functions, calls, scalar locals,
//! control flow, raw pointer load/store, arithmetic, and string literals.

pub(super) use anyhow::{Result, anyhow, bail};
pub(super) use std::collections::{HashMap, HashSet};

pub(super) use crate::backend::ir::{
    BinOp, Expr, ExprKind, Func, LValue, Literal, Program, Stmt, StructDef, Type,
};

/// Compile an IR program to a 16-bit real-mode flat binary payload.
///
/// The returned bytes are the raw machine code and data; the caller is
/// responsible for padding to a boot sector and appending the signature.
pub fn compile_program(prog: &Program) -> Result<Vec<u8>> {
    let load_base = prog
        .config
        .as_ref()
        .map(|c| c.load_base)
        .unwrap_or(0x7C00);
    let mut cg = CodeGen16::new(prog, load_base);
    cg.emit_program()?;
    cg.finish()
}

pub(super) struct CodeGen16<'p> {
    prog: &'p Program,
    asm: Encoder,
    locals: HashMap<String, Slot16>,
    arrays: HashMap<String, i8>,
    frame_size: u8,
    func_labels: HashMap<String, u32>,
    string_labels: HashMap<String, u32>,
    ret_label: u32,
    loop_end_stack: Vec<u32>,
    loop_head_stack: Vec<u32>,
    /// Names of `_dev_*` runtime helpers the program actually calls; only
    /// these get emitted.
    referenced: HashSet<String>,
}

#[derive(Clone, Copy)]
pub(super) struct Slot16 {
    offset: i8,
    width: u8,
    signed: bool,
}

mod asm;
mod expr;
mod layout;
mod program;
mod stmt;
pub(super) use asm::*;
pub(super) use program::BUILTIN_FUNCS;

impl<'p> CodeGen16<'p> {
    pub(super) fn new(prog: &'p Program, load_base: u16) -> Self {
        Self {
            prog,
            asm: Encoder::new(load_base),
            locals: HashMap::new(),
            arrays: HashMap::new(),
            frame_size: 0,
            func_labels: HashMap::new(),
            string_labels: HashMap::new(),
            ret_label: 0,
            loop_end_stack: Vec::new(),
            loop_head_stack: Vec::new(),
            referenced: HashSet::new(),
        }
    }

    pub(super) fn string_label(&mut self, s: &str) -> u32 {
        if let Some(&lab) = self.string_labels.get(s) {
            return lab;
        }
        let lab = self.asm.new_label();
        self.string_labels.insert(s.to_string(), lab);
        lab
    }
}
pub(super) fn type_info(ty: &Type) -> Result<(u8, bool)> {
    match ty {
        Type::I8 => Ok((1, true)),
        Type::U8 | Type::Char | Type::Bool => Ok((1, false)),
        Type::I16 => Ok((2, true)),
        Type::U16 | Type::Ptr(_) => Ok((2, false)),
        _ => bail!("type {:?} is not supported by the 16-bit backend", ty),
    }
}

pub(super) fn type_size_16(prog: &Program, ty: &Type) -> usize {
    match ty {
        Type::I8 | Type::U8 | Type::Char | Type::Bool => 1,
        Type::I16 | Type::U16 | Type::Ptr(_) => 2,
        Type::I32 | Type::U32 => 4,
        Type::I64 | Type::U64 => 8,
        Type::Struct(name) => {
            let def = match prog.structs.iter().find(|s| s.name == *name) {
                Some(d) => d,
                None => return 2,
            };
            let layout = layout_struct(def);
            layout.size as usize
        }
        _ => 2,
    }
}

pub(super) fn align_up_u8(value: u8, align: u8) -> u8 {
    if align == 0 || value.is_multiple_of(align) {
        value
    } else {
        value + align - value % align
    }
}

pub(super) fn layout_struct(s: &StructDef) -> StructLayout {
    let mut offset = 0u16;
    let mut offsets = Vec::with_capacity(s.fields.len());
    for (_, ty) in &s.fields {
        let size = match type_info(ty) {
            Ok((w, _)) => w as u16,
            Err(_) => 2, // unsupported fields default to 16-bit
        };
        offset = align_up_u16(offset, size.max(1) as u16);
        offsets.push(offset);
        offset += size;
    }
    let align = s
        .fields
        .iter()
        .map(|(_, ty)| match type_info(ty) {
            Ok((w, _)) => w.max(1),
            Err(_) => 1,
        })
        .max()
        .unwrap_or(1);
    StructLayout {
        size: align_up_u16(offset, align as u16),
        align: align as u16,
        offsets,
    }
}

pub(super) fn align_up_u16(value: u16, align: u16) -> u16 {
    if align == 0 {
        return value;
    }
    value.div_ceil(align) * align
}

pub(super) fn cond_for_cmp(op: BinOp, ty: &Type) -> Result<Cond> {
    let signed = ty.is_signed();
    Ok(match op {
        BinOp::Eq => Cond::E,
        BinOp::Ne => Cond::Ne,
        BinOp::Lt if signed => Cond::L,
        BinOp::Le if signed => Cond::Le,
        BinOp::Gt if signed => Cond::G,
        BinOp::Ge if signed => Cond::Ge,
        BinOp::Lt => Cond::B,
        BinOp::Le => Cond::Be,
        BinOp::Gt => Cond::A,
        BinOp::Ge => Cond::Ae,
        _ => bail!("unsupported comparison operator"),
    })
}

pub(super) struct StructLayout {
    #[allow(dead_code)]
    size: u16,
    #[allow(dead_code)]
    align: u16,
    offsets: Vec<u16>,
}

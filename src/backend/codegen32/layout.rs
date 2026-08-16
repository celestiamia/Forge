use super::*;

/// Compute the layout of every struct in the program, recursively laying out
/// nested structs first.  Memoized; rejects by-value struct cycles and unknown
/// struct references with clean errors instead of panicking.
pub(super) fn compute_struct_layouts(
    structs: &[StructDef],
) -> Result<HashMap<String, StructLayout>> {
    let by_name: HashMap<&str, &StructDef> = structs.iter().map(|s| (s.name.as_str(), s)).collect();
    let mut layouts: HashMap<String, StructLayout> = HashMap::new();
    let mut visiting: Vec<String> = Vec::new();
    for s in structs {
        compute_layout_recursive(s, &by_name, &mut layouts, &mut visiting)?;
    }
    Ok(layouts)
}

pub(super) fn compute_layout_recursive(
    s: &StructDef,
    by_name: &HashMap<&str, &StructDef>,
    layouts: &mut HashMap<String, StructLayout>,
    visiting: &mut Vec<String>,
) -> Result<StructLayout> {
    if let Some(l) = layouts.get(&s.name) {
        return Ok(l.clone());
    }
    if visiting.iter().any(|n| n == &s.name) {
        bail!(
            "struct `{}` contains itself by value; recursive structs are not supported",
            s.name
        );
    }
    visiting.push(s.name.clone());
    for (_, ty) in &s.fields {
        if let Type::Struct(name) = ty {
            let nested = by_name
                .get(name.as_str())
                .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?;
            compute_layout_recursive(nested, by_name, layouts, visiting)?;
        }
    }
    visiting.pop();
    let layout = layout_struct(s, layouts);
    layouts.insert(s.name.clone(), layout.clone());
    Ok(layout)
}

impl<'p> CodeGen<'p> {
    pub(super) fn alloc_slot(&mut self, size: usize, align: usize) -> Slot {
        let aligned = align_up(self.frame_size, align);
        self.frame_size = aligned + size;
        Slot {
            offset: -(self.frame_size as i32),
            size,
        }
    }

    pub(super) fn alloc_named_slot(&mut self, name: &str, size: usize, align: usize) -> Slot {
        let slot = self.alloc_slot(size, align);
        self.locals.insert(name.to_string(), slot.clone());
        slot
    }

    pub(super) fn store_scalar(&mut self, offset: i32) -> Result<()> {
        self.asm.mov(Mem::base_disp(Reg::Ebp, offset), Reg::Eax)?;
        Ok(())
    }

    pub(super) fn store_width(&mut self, width: u32, addr: Reg, value: Reg) -> Result<()> {
        let mem = Mem::base(addr);
        match width {
            8 => self.asm.store8(mem, value)?,
            16 => self.asm.store16(mem, value)?,
            32 => self.asm.store32(mem, value)?,
            _ => self.asm.mov(mem, value)?,
        }
        Ok(())
    }

    pub(super) fn lvalue_store_width(&self, lv: &LValue) -> u32 {
        match lv {
            LValue::Deref(ptr) => match &ptr.ty {
                Type::Ptr(inner) => scalar_width(inner),
                _ => 32,
            },
            LValue::Field { base, field } => {
                let name = match &base.ty {
                    Type::Ptr(inner) => match inner.as_ref() {
                        Type::Struct(n) => n.clone(),
                        _ => return 32,
                    },
                    _ => return 32,
                };
                self.prog
                    .structs
                    .iter()
                    .find(|s| s.name == name)
                    .and_then(|s| s.fields.get(*field))
                    .map(|(_, t)| scalar_width(t))
                    .unwrap_or(32)
            }
            LValue::Var(_) => 32,
        }
    }

    /// Whether `ty` is a synthetic `__enum_*` struct.  Enum values on x86_32
    /// are 4-byte pointers to a stack temp holding tag+payload, so they keep
    /// scalar (pointer-value) semantics; only real user structs are
    /// address-bearing inline structs.
    pub(super) fn is_enum_struct(ty: &Type) -> bool {
        matches!(ty, Type::Struct(name) if name.starts_with("__enum_"))
    }

    /// Byte size (≥4) of a value of `ty`, treating structs as their full layout
    /// size.  Returns `None` for void/unsized types.
    pub(super) fn value_size(&self, ty: &Type) -> Option<usize> {
        match ty {
            Type::Struct(name) => self
                .struct_layouts
                .get(name)
                .map(|l| l.size.max(4)),
            _ => Some(type_size(ty, &self.struct_layouts).max(4)),
        }
    }

    /// Copy `size` bytes from `[src_reg]` to `[dst_reg]` using 4-byte moves
    /// and a trailing 1/2/3-byte move for the remainder.  `src_reg` and
    /// `dst_reg` are addresses (not the data themselves).
    pub(super) fn copy_mem_to_mem(&mut self, dst: Reg, src: Reg, size: usize) -> Result<()> {
        let mut offset = 0i32;
        let mut remaining = size;
        while remaining >= 4 {
            self.asm
                .mov(Reg::Ecx, Mem::base_disp(src, offset))?;
            self.asm
                .store32(Mem::base_disp(dst, offset), Reg::Ecx)?;
            offset += 4;
            remaining -= 4;
        }
        if remaining > 0 {
            match remaining {
                1 => {
                    self.asm.movzx8(Reg::Ecx, Mem::base_disp(src, offset))?;
                    self.asm.store8(Mem::base_disp(dst, offset), Reg::Ecx)?;
                }
                2 => {
                    self.asm.mov(Reg::Ecx, Mem::base_disp(src, offset))?;
                    self.asm.store16(Mem::base_disp(dst, offset), Reg::Ecx)?;
                }
                3 => {
                    self.asm.movzx8(Reg::Ecx, Mem::base_disp(src, offset))?;
                    self.asm.store8(Mem::base_disp(dst, offset), Reg::Ecx)?;
                    let o = offset + 1;
                    self.asm.mov(Reg::Ecx, Mem::base_disp(src, o))?;
                    self.asm.store16(Mem::base_disp(dst, o), Reg::Ecx)?;
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    pub(super) fn load_from_addr(&mut self, ty: &Type) -> Result<()> {
        match ty {
            Type::I8 => self.asm.movsx8(Reg::Eax, Mem::base(Reg::Eax))?,
            Type::U8 | Type::Char | Type::Bool => self.asm.movzx8(Reg::Eax, Mem::base(Reg::Eax))?,
            Type::I16 => self.asm.movsx16(Reg::Eax, Mem::base(Reg::Eax))?,
            Type::U16 => self.asm.movzx16(Reg::Eax, Mem::base(Reg::Eax))?,
            Type::I32 | Type::U32 | Type::F32 => self.asm.mov(Reg::Eax, Mem::base(Reg::Eax))?,
            _ => self.asm.mov(Reg::Eax, Mem::base(Reg::Eax))?,
        }
        Ok(())
    }

    pub(super) fn struct_ptr_layout(&self, ty: &Type) -> Result<(String, StructLayout)> {
        let name = match ty {
            Type::Ptr(inner) => match inner.as_ref() {
                Type::Struct(n) => n.clone(),
                _ => bail!("gep/load on non-struct pointer: {:?}", ty),
            },
            Type::Struct(n) => n.clone(),
            _ => bail!("field access on non-struct type: {:?}", ty),
        };
        let lay = self
            .struct_layouts
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", name))?
            .clone();
        Ok((name, lay))
    }

    pub(super) fn field_offset(&self, struct_name: &str, idx: usize) -> Result<usize> {
        let lay = self
            .struct_layouts
            .get(struct_name)
            .ok_or_else(|| anyhow::anyhow!("unknown struct: {}", struct_name))?;
        lay.offsets
            .get(idx)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("field index {} out of range", idx))
    }
}

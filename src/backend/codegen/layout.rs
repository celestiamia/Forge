use super::*;

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
        self.asm.mov(Mem::base_disp(Reg::Rbp, offset), Reg::Rax)?;
        Ok(())
    }

    pub(super) fn store_rdx_64(&mut self) -> Result<()> {
        self.asm.mov(Mem::base(Reg::Rdx), Reg::Rax)
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
                _ => 64,
            },
            LValue::Field { base, field } => {
                let name = match &base.ty {
                    Type::Ptr(inner) => match inner.as_ref() {
                        Type::Struct(n) => n.clone(),
                        _ => return 64,
                    },
                    _ => return 64,
                };
                self.prog
                    .structs
                    .iter()
                    .find(|s| s.name == name)
                    .and_then(|s| s.fields.get(*field))
                    .map(|(_, t)| scalar_width(t))
                    .unwrap_or(64)
            }
            LValue::Var(_) => 64,
        }
    }

    pub(super) fn load_from_addr(&mut self, ty: &Type) -> Result<()> {
        match ty {
            Type::I8 => self.asm.movsx8(Reg::Rax, Mem::base(Reg::Rax))?,
            Type::U8 | Type::Char | Type::Bool => self.asm.movzx8(Reg::Rax, Mem::base(Reg::Rax))?,
            Type::I16 => self.asm.movsx16(Reg::Rax, Mem::base(Reg::Rax))?,
            Type::U16 => self.asm.movzx16(Reg::Rax, Mem::base(Reg::Rax))?,
            Type::I32 => self.asm.movsxd(Reg::Rax, Mem::base(Reg::Rax))?,
            Type::U32 => self.asm.mov32(Reg::Rax, Mem::base(Reg::Rax))?,
            _ => self.asm.mov(Reg::Rax, Mem::base(Reg::Rax))?,
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

use super::*;

impl<'p> CodeGen16<'p> {
    pub(super) fn alloc_named(&mut self, name: &str, ty: &Type) -> Result<Slot16> {
        let (size, signed) = type_info(ty)?;
        let align = size.max(1);
        let offset = self.alloc_slot(size, align);
        let slot = Slot16 {
            offset,
            width: size,
            signed,
        };
        self.locals.insert(name.to_string(), slot);
        Ok(slot)
    }

    pub(super) fn alloc_slot(&mut self, size: u8, align: u8) -> i8 {
        let aligned = align_up_u8(self.frame_size, align);
        self.frame_size = aligned + size;
        -(self.frame_size as i8)
    }

    pub(super) fn field_offset(&self, ptr_ty: &Type, field: usize) -> Result<u16> {
        let name = match ptr_ty {
            Type::Ptr(inner) => match inner.as_ref() {
                Type::Struct(n) => n.clone(),
                _ => bail!("field access on non-struct pointer"),
            },
            Type::Struct(n) => n.clone(),
            _ => bail!("field access on non-struct type"),
        };
        let def = self
            .prog
            .structs
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("unknown struct: {}", name))?;
        let layout = layout_struct(def);
        layout
            .offsets
            .get(field)
            .copied()
            .ok_or_else(|| anyhow!("field index out of range"))
    }

    pub(super) fn load_slot(&mut self, slot: Slot16) -> Result<()> {
        match slot.width {
            1 => {
                self.asm.load8_bp(Reg8::Al, slot.offset);
                if slot.signed {
                    self.asm.cbw();
                } else {
                    self.asm.xor_ah_ah();
                }
            }
            2 => self.asm.load16_bp(Reg16::Ax, slot.offset),
            _ => bail!("unhandled slot width: {}", slot.width),
        }
        Ok(())
    }

    pub(super) fn store_slot(&mut self, slot: Slot16, _src16: Reg16, src8: Reg8) -> Result<()> {
        match slot.width {
            1 => self.asm.store8_bp(slot.offset, src8),
            2 => self.asm.store16_bp(slot.offset, Reg16::Ax),
            _ => bail!("unhandled slot width: {}", slot.width),
        }
        Ok(())
    }

}

use super::*;

impl<'p> CodeGen16<'p> {
    pub(super) fn emit_program(&mut self) -> Result<()> {
        let mut funcs: Vec<&Func> = Vec::new();
        let mut start: Option<&Func> = None;
        for f in &self.prog.funcs {
            if f.name == "_start" {
                start = Some(f);
            } else {
                funcs.push(f);
            }
        }
        let start =
            start.ok_or_else(|| anyhow!("flat binary boot target requires a _start function"))?;

        let start_lab = self.asm.new_label();
        self.func_labels.insert("_start".to_string(), start_lab);
        for f in &funcs {
            let lab = self.asm.new_label();
            self.func_labels.insert(f.name.clone(), lab);
        }
        let teletype_lab = self.asm.new_label();
        let halt_lab = self.asm.new_label();
        let load_char_lab = self.asm.new_label();
        let serial_lab = self.asm.new_label();
        self.func_labels
            .insert("_dev_bios_teletype".to_string(), teletype_lab);
        self.func_labels.insert("_dev_halt".to_string(), halt_lab);
        self.func_labels
            .insert("_dev_load_char".to_string(), load_char_lab);
        self.func_labels
            .insert("_dev_serial_putc".to_string(), serial_lab);

        self.emit_func(start, true)?;
        for f in &funcs {
            self.emit_func(f, false)?;
        }

        self.emit_builtins(teletype_lab, halt_lab, load_char_lab, serial_lab)?;

        let mut strings: Vec<(u32, &str)> = self
            .string_labels
            .iter()
            .map(|(s, lab)| (*lab, s.as_str()))
            .collect();
        strings.sort_by_key(|(lab, _)| *lab);
        for (lab, s) in strings {
            self.asm.bind(lab);
            self.asm.db_str(s);
        }

        Ok(())
    }

    pub(super) fn finish(self) -> Result<Vec<u8>> {
        self.asm.into_bytes()
    }

    pub(super) fn emit_func(&mut self, f: &Func, is_start: bool) -> Result<()> {
        let lab = *self.func_labels.get(&f.name).unwrap();
        self.asm.bind(lab);

        self.locals.clear();
        self.arrays.clear();
        self.frame_size = 0;
        self.scan_func(f)?;
        let frame = align_up_u8(self.frame_size, 2);

        if is_start {
            self.emit_segment_setup()?;
        }

        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        if frame > 0 {
            self.asm.sub_sp_imm(frame as i16)?;
        }

        for (i, (name, _ty)) in f.params.iter().enumerate() {
            let slot = *self
                .locals
                .get(name)
                .ok_or_else(|| anyhow!("missing param slot: {}", name))?;
            let arg_off = (4 + i * 2) as i8;
            self.asm.load16_bp(Reg16::Ax, arg_off);
            self.store_slot(slot, Reg16::Ax, Reg8::Al)?;
        }

        self.ret_label = self.asm.new_label();
        for s in &f.body {
            self.emit_stmt(s)?;
        }
        self.asm.bind(self.ret_label);
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp); // mov sp, bp
        self.asm.pop(Reg16::Bp);
        self.asm.ret();
        Ok(())
    }

    pub(super) fn emit_segment_setup(&mut self) -> Result<()> {
        self.asm.xor_ax_ax();
        self.asm.mov_seg_ax(SegReg::Ds)?;
        self.asm.mov_seg_ax(SegReg::Es)?;
        self.asm.mov_seg_ax(SegReg::Ss)?;
        self.asm.mov16_imm(Reg16::Sp, 0x7C00);
        Ok(())
    }

    pub(super) fn emit_builtins(
        &mut self,
        teletype_lab: u32,
        halt_lab: u32,
        load_char_lab: u32,
        serial_lab: u32,
    ) -> Result<()> {
        self.asm.bind(teletype_lab);
        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        self.asm.load8_bp(Reg8::Al, 4);
        self.asm.mov8_imm(Reg8::Ah, 0x0E);
        self.asm.mov8_imm(Reg8::Bh, 0x00);
        self.asm.mov8_imm(Reg8::Bl, 0x07);
        self.asm.int(0x10);
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
        self.asm.pop(Reg16::Bp);
        self.asm.ret();

        self.asm.bind(halt_lab);
        self.asm.cli();
        self.asm.hlt();

        self.asm.bind(load_char_lab);
        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        self.asm.load16_bp(Reg16::Si, 4);
        self.asm.load8_si(Reg8::Al);
        self.asm.xor_ah_ah();
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
        self.asm.pop(Reg16::Bp);
        self.asm.ret();

        self.asm.bind(serial_lab);
        self.asm.push(Reg16::Bp);
        self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
        self.asm.load8_bp(Reg8::Al, 4);
        self.asm.out_imm8_al(0xE9); // Bochs/QEMU debug port
        self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
        self.asm.pop(Reg16::Bp);
        self.asm.ret();
        Ok(())
    }

    pub(super) fn scan_func(&mut self, f: &Func) -> Result<()> {
        for (name, ty) in &f.params {
            self.alloc_named(name, ty)?;
        }
        for s in &f.body {
            self.scan_stmt(s)?;
        }
        Ok(())
    }

    pub(super) fn scan_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Let { name, ty, init: _ } => {
                let _ = self.alloc_named(name, ty)?;
            }
            Stmt::StackAlloc {
                name,
                elem_ty,
                count,
            } => {
                let (size, _signed) = type_info(elem_ty)?;
                let raw_size = size as usize * *count;
                let align = size.max(1);
                let raw_off = self.alloc_slot(raw_size as u8, align);
                let ptr_off = self.alloc_named(name, &Type::Ptr(Box::new(elem_ty.clone())))?;
                self.arrays.insert(name.clone(), raw_off);
                let _ = ptr_off;
            }
            Stmt::If { then, else_, .. } => {
                for s in then {
                    self.scan_stmt(s)?;
                }
                if let Some(b) = else_ {
                    for s in b {
                        self.scan_stmt(s)?;
                    }
                }
            }
            Stmt::While { body, .. } | Stmt::Unsafe(body) => {
                for s in body {
                    self.scan_stmt(s)?;
                }
            }
            Stmt::For {
                init, body, step, ..
            } => {
                if let Some(i) = init {
                    self.scan_stmt(i)?;
                }
                for s in body {
                    self.scan_stmt(s)?;
                }
                let _ = step;
            }
            _ => {}
        }
        Ok(())
    }
}

use super::*;

/// `_dev_*` runtime helpers the 16-bit backend can emit.  Only the ones the
/// program actually calls are emitted (tracked via [`CodeGen16::referenced`]).
pub const BUILTIN_FUNCS: [&str; 12] = [
    "_dev_bios_teletype",
    "_dev_serial_putc",
    "_dev_load_char",
    "_dev_halt",
    "_dev_bios_key",
    "_dev_bios_disk_read",
    "_dev_bios_disk_reset",
    "_dev_bios_reboot",
    "_dev_bios_clear",
    "_dev_jump",
    "_dev_bios_disk_read_lba",
    "_dev_enter_pmode",
];

impl<'p> CodeGen16<'p> {
    pub(super) fn emit_program(&mut self) -> Result<()> {
        // Flat binary entry: the `ENTRY` directive from a linker script,
        // falling back to the conventional `_start`.
        let entry_name = self
            .prog
            .config
            .as_ref()
            .map(|c| c.entry.as_str())
            .unwrap_or("_start");
        let mut funcs: Vec<&Func> = Vec::new();
        let mut start: Option<&Func> = None;
        for f in &self.prog.funcs {
            if f.name == entry_name {
                start = Some(f);
            } else {
                funcs.push(f);
            }
        }
        let start = start
            .ok_or_else(|| anyhow!("flat binary boot target requires a {} function", entry_name))?;

        let start_lab = self.asm.new_label();
        self.func_labels.insert(entry_name.to_string(), start_lab);
        for f in &funcs {
            let lab = self.asm.new_label();
            self.func_labels.insert(f.name.clone(), lab);
        }
        for name in BUILTIN_FUNCS {
            let lab = self.asm.new_label();
            self.func_labels.insert(name.to_string(), lab);
        }

        self.emit_func(start, true)?;
        for f in &funcs {
            self.emit_func(f, false)?;
        }

        self.emit_builtins()?;

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

        // Args are pushed left-to-right by the caller, so the first argument
        // ends up at the deepest stack offset: param i lives at
        // 4 + (n - 1 - i) * 2.
        let n = f.params.len();
        for (i, (name, _ty)) in f.params.iter().enumerate() {
            let slot = *self
                .locals
                .get(name)
                .ok_or_else(|| anyhow!("missing param slot: {}", name))?;
            let arg_off = (4 + (n - 1 - i) * 2) as i8;
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

    pub(super) fn emit_builtins(&mut self) -> Result<()> {
        for name in BUILTIN_FUNCS {
            if !self.referenced.contains(name) {
                continue;
            }
            let lab = *self
                .func_labels
                .get(name)
                .ok_or_else(|| anyhow!("missing builtin label: {}", name))?;
            self.asm.bind(lab);
            match name {
                "_dev_bios_teletype" => {
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
                }
                "_dev_halt" => {
                    self.asm.cli();
                    self.asm.hlt();
                }
                "_dev_load_char" => {
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load16_bp(Reg16::Si, 4);
                    self.asm.load8_si(Reg8::Al);
                    self.asm.xor_ah_ah();
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_serial_putc" => {
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load8_bp(Reg8::Al, 4);
                    self.asm.out_imm8_al(0xE9); // Bochs/QEMU debug port
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_bios_key" => {
                    // INT 16h AH=0: block until a key, return its ASCII code.
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.mov8_imm(Reg8::Ah, 0x00);
                    self.asm.int(0x16);
                    self.asm.xor_ah_ah();
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_bios_disk_reset" => {
                    // INT 13h AH=0: reset/recalibrate the drive.
                    // Arg: drive.  Returns the BIOS status byte (0 = ok).
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load8_bp(Reg8::Dl, 4); // drive
                    self.asm.mov8_imm(Reg8::Ah, 0x00);
                    self.asm.int(0x13);
                    self.asm.xor_ah_ah();
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_bios_disk_read" => {
                    // INT 13h AH=2: read CHS sectors into ES:BX.
                    // Args: drive, cyl, head, sector (1-based), count, buffer
                    // (flat address).  Args are pushed left-to-right, so the
                    // buffer is the first push and sits at [bp+4].
                    // ES:BX is derived from the flat buffer address:
                    // ES = addr >> 4, BX = addr & 0xF.
                    // Returns 0 on success or the BIOS status byte on failure.
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load16_bp(Reg16::Bx, 4); // buffer
                    self.asm.mov16_rr(Reg16::Ax, Reg16::Bx);
                    self.asm.emit_slice(&[0xD1, 0xE8]); // shr ax, 1
                    self.asm.emit_slice(&[0xD1, 0xE8]); // shr ax, 1
                    self.asm.emit_slice(&[0xD1, 0xE8]); // shr ax, 1
                    self.asm.emit_slice(&[0xD1, 0xE8]); // shr ax, 1
                    self.asm.mov_seg_ax(SegReg::Es)?; // es = buf >> 4
                    self.asm.emit_slice(&[0x83, 0xE3, 0x0F]); // and bx, 0x0F
                    self.asm.load8_bp(Reg8::Dl, 14); // drive
                    self.asm.load8_bp(Reg8::Ch, 12); // cyl
                    self.asm.load8_bp(Reg8::Dh, 10); // head
                    self.asm.load8_bp(Reg8::Cl, 8); // sector
                    self.asm.load8_bp(Reg8::Al, 6); // count
                    self.asm.mov8_imm(Reg8::Ah, 0x02);
                    self.asm.int(0x13);
                    let ok_lab = self.asm.new_label();
                    self.asm.jcc_short_lab(0x73, ok_lab); // jnc
                    self.asm.emit_slice(&[0x8A, 0xC4]); // mov al, ah (status)
                    self.asm.xor_ah_ah();
                    let done_lab = self.asm.new_label();
                    self.asm.jmp_short_lab(done_lab);
                    self.asm.bind(ok_lab);
                    self.asm.xor_ax_ax();
                    self.asm.bind(done_lab);
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_bios_reboot" => {
                    // INT 19h: reboot the machine.  Never returns.
                    self.asm.int(0x19);
                }
                "_dev_bios_clear" => {
                    // INT 10h AH=0, AL=3: switch to 80x25 text mode, clearing
                    // the screen.
                    self.asm.mov16_imm(Reg16::Ax, 0x0003);
                    self.asm.int(0x10);
                    self.asm.ret();
                }
                "_dev_jump" => {
                    // Far jump to segment 0:addr (arg).
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load16_bp(Reg16::Ax, 4);
                    self.asm.emit_slice(&[0x6A, 0x00]); // push 0 (segment)
                    self.asm.push(Reg16::Ax);
                    self.asm.emit_slice(&[0xCB]); // retf
                }
                "_dev_bios_disk_read_lba" => {
                    // INT 13h AH=42h: LBA disk read via a Disk Address
                    // Packet.  Args: drive, lba_lo, lba_hi, count, es, bx
                    // (buffer segment:offset).  Args are pushed left-to-right,
                    // so bx sits at [bp+4] and drive at [bp+14].
                    //
                    // The 16-byte DAP is built on the stack (SS:SP), which is
                    // a real address because ForgeOS stages run with
                    // DS=ES=SS=0.  SeaBIOS's int13ext_s lays the packet out
                    // as size(0) reserved(1) count(2) offset(4) segment(6)
                    // lba(8) - NOT the lba-at-4 layout of the original IBM
                    // spec - so high field words are pushed first and SP ends
                    // up pointing at byte 0 (packet size).
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.push_imm16(0x0000); // dap[14..16] extended (hi)
                    self.asm.push_imm16(0x0000); // dap[12..14] extended (lo)
                    self.asm.load16_bp(Reg16::Ax, 10); // lba_hi
                    self.asm.push(Reg16::Ax);
                    self.asm.load16_bp(Reg16::Ax, 12); // lba_lo
                    self.asm.push(Reg16::Ax);
                    self.asm.load16_bp(Reg16::Ax, 6); // es (segment)
                    self.asm.push(Reg16::Ax);
                    self.asm.load16_bp(Reg16::Ax, 4); // bx (offset)
                    self.asm.push(Reg16::Ax);
                    self.asm.load16_bp(Reg16::Ax, 8); // count
                    self.asm.push(Reg16::Ax);
                    self.asm.push_imm16(0x0010); // dap[0..2] size + reserved
                    self.asm.mov16_rm(Reg16::Si, Reg16::Sp); // si = dap
                    self.asm.load8_bp(Reg8::Dl, 14); // drive
                    self.asm.mov8_imm(Reg8::Ah, 0x42);
                    self.asm.int(0x13);
                    // Check the carry flag BEFORE touching the stack: `add sp`
                    // clobbers CF and would turn every failed read into a
                    // success.  The DAP is dropped on both paths below.
                    let ok_lab = self.asm.new_label();
                    self.asm.jcc_short_lab(0x73, ok_lab); // jnc
                    self.asm.emit_slice(&[0x8A, 0xC4]); // mov al, ah (status)
                    self.asm.xor_ah_ah();
                    self.asm.add_sp_imm(16)?; // drop the DAP
                    let done_lab = self.asm.new_label();
                    self.asm.jmp_short_lab(done_lab);
                    self.asm.bind(ok_lab);
                    self.asm.xor_ax_ax();
                    self.asm.add_sp_imm(16)?; // drop the DAP
                    self.asm.bind(done_lab);
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_enter_pmode" => {
                    // Switch from real mode to 32-bit protected mode and jump
                    // to a kernel entry address.  Args: entry_lo, entry_hi
                    // (the 32-bit entry point, split because 16-bit Forge
                    // cannot hold such constants).  Args are pushed
                    // left-to-right, so entry_hi sits at [bp+4] and entry_lo
                    // at [bp+6].
                    //
                    // The stub is fully self-contained: it enables A20,
                    // installs a flat 4 GB GDT (emitted inline in the image,
                    // which is RAM), sets PE in CR0, far-jumps through
                    // selector 0x08 to a small 32-bit trampoline right after
                    // the jump, reloads DS/ES/SS with the data selector
                    // (selector 0 is the null descriptor and faults), parks
                    // the stack, copies the kernel staging buffer (0x8000,
                    // where INT 13h could write it - see the LBA stub) to
                    // the kernel's link address 0x100000, and finally jumps
                    // to the kernel.
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.cli();
                    // Fast A20: read port 0x92, set bit 1, write back.
                    self.asm.emit_slice(&[0xE4, 0x92]); // in al, 0x92
                    self.asm.emit_slice(&[0x0C, 0x02]); // or al, 2
                    self.asm.emit_slice(&[0xE6, 0x92]); // out 0x92, al
                    let gdtr_lab = self.asm.new_label();
                    self.asm.emit_slice(&[0x0F, 0x01, 0x16]); // lgdt [abs16]
                    self.asm.imm16_label(gdtr_lab);
                    // mov eax, cr0; or eax, 1; mov cr0, eax
                    self.asm.emit_slice(&[0x66, 0x0F, 0x20, 0xC0]);
                    self.asm.emit_slice(&[0x66, 0x83, 0xC8, 0x01]);
                    self.asm.emit_slice(&[0x66, 0x0F, 0x22, 0xC0]);
                    // far jump: 66 EA <imm32 trampoline> <selector 0x08>
                    let tramp_lab = self.asm.new_label();
                    self.asm.emit_slice(&[0x66, 0xEA]);
                    self.asm.imm32_label(tramp_lab);
                    self.asm.emit_slice(&[0x08, 0x00]);
                    // --- 32-bit trampoline ---
                    // From here on the CPU is in 32-bit mode (CS = 0x08, D=1):
                    // 32-bit instructions must NOT carry the operand-size
                    // prefix (66 makes them 16-bit here), only the 16-bit
                    // movs below need it.
                    self.asm.bind(tramp_lab);
                    self.asm.emit_slice(&[0x66, 0xB8, 0x10, 0x00]); // mov ax, 0x10
                    self.asm.emit_slice(&[0x8E, 0xD8]); // mov ds, ax
                    self.asm.emit_slice(&[0x8E, 0xC0]); // mov es, ax
                    self.asm.emit_slice(&[0x8E, 0xD0]); // mov ss, ax
                    self.asm.emit_slice(&[0xBC]); // mov esp, imm32
                    self.asm.emit_imm32(0x00070000); // kernel stack (below VGA)
                    // Relocate the kernel image from its low-memory load
                    // buffer (0x8000) to its link address (0x100000).  The
                    // copy must happen here: INT 13h AH=42h PIO transfers
                    // address their buffer via a 16-bit ES:DI pair
                    // (ES = buf>>4, DI = buf&15), so SeaBIOS can only write
                    // into the low 1 MiB - and this is running in flat
                    // 32-bit mode with A20 enabled, where 0x100000 is
                    // reachable.
                    self.asm.emit_slice(&[0xBE]); // mov esi, imm32
                    self.asm.emit_imm32(0x00008000); // source (staging)
                    self.asm.emit_slice(&[0xBF]); // mov edi, imm32
                    self.asm.emit_imm32(0x00100000); // dest (kernel link)
                    self.asm.emit_slice(&[0xB9]); // mov ecx, imm32
                    self.asm.emit_imm32(4096 / 4); // 8 sectors of dwords
                    self.asm.emit_slice(&[0xF3, 0xA5]); // rep movsd
                    // eax = (entry_hi << 16) | entry_lo
                    self.asm.emit_slice(&[0x66, 0x8B, 0x45, 0x04]); // mov ax, [bp+4]
                    self.asm.emit_slice(&[0xC1, 0xE0, 0x10]); // shl eax, 16
                    self.asm.emit_slice(&[0x66, 0x8B, 0x45, 0x06]); // mov ax, [bp+6]
                    self.asm.emit_slice(&[0xFF, 0xE0]); // jmp eax
                    // --- GDTR: limit 23, base = GDT address ---
                    self.asm.bind(gdtr_lab);
                    self.asm.emit_imm16(23);
                    let gdt_lab = self.asm.new_label();
                    self.asm.imm32_label(gdt_lab);
                    // --- GDT: null, flat code (0x08), flat data (0x10) ---
                    self.asm.bind(gdt_lab);
                    self.asm.emit_slice(&[0x00; 8]);
                    self.asm
                        .emit_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9A, 0xCF, 0x00]);
                    self.asm
                        .emit_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00]);
                }
                other => bail!("unknown builtin {}", other),
            }
        }
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
                let size = type_size_16(self.prog, elem_ty) as u8;
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

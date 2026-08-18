use super::*;

/// `_dev_*` runtime helpers the 16-bit backend can emit.  Only the ones the
/// program actually calls are emitted (tracked via [`CodeGen16::referenced`]).
pub const BUILTIN_FUNCS: [&str; 20] = [
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
    "_dev_enter_long_mode",
    // Port I/O (byte and word — native 16-bit widths)
    "_dev_outb",
    "_dev_inb",
    "_dev_outw",
    "_dev_inw",
    // Interrupt control (STI/CLI/IRET are single-byte opcodes; INT nn is
    // emitted inline by the lowerer via ExprKind::IntImm, not via a call).
    "_dev_iret",
    "_dev_sti",
    "_dev_cli",
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
                        .emit_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00]); // data32
                }
                "_dev_enter_long_mode" => {
                    // Switch from real mode all the way to 64-bit long mode
                    // and jump to a kernel entry.  Args: entry_lo, entry_hi
                    // (the 32-bit entry point split because 16-bit Forge cannot
                    // hold such constants; e.g. an x86_64 raw kernel linked at
                    // 0x100000 is entered as `_dev_enter_long_mode(0, 0x0010)`).
                    //
                    // Lays down the 16-bit prologue of `_dev_enter_pmode` (A20,
                    // 32-bit flat GDT, CR0.PE, far jmp 0x08 -> 32-bit trampoline),
                    // then in the 32-bit trampoline copies the staging buffer
                    // (0x8000, where the BIOS read the kernel) to the kernel's
                    // link address 0x100000, stashes the entry at 0x8FF8, builds
                    // 4-level identity page tables, enables PAE/EFER.LME/CR0.PG,
                    // loads a 64-bit GDT, and far-jumps to a 64-bit trampoline
                    // that reads the stashed entry and jumps there.
                    //
                    // Page structures live at fixed, 4 KiB-aligned low addresses
                    // (QEMU zeroes RAM at reset): PML4@0xA000, PDPT@0xB000,
                    // 64-bit GDT@0xC000, its gdtr@0xC018, the entry stash@0x8FF8
                    // (in the staging region, freed by the rep movsd above).
                    //
                    // PDPT[0]=0x83 is a 1 GiB page covering 0..1 GiB.  This is
                    // what makes the transition correct: with LME+PAE+PG the CPU
                    // briefly runs in PAE 3-level mode (CR3 read as PDPT) until
                    // the far jmp loads the 64-bit code segment; reading PDPT[0]
                    // as a PDE then also yields a 2 MiB page @0, so both the
                    // `jmp` after CR0.PG and the 64-bit trampoline resolve.  The
                    // 4-level walk (PML4[0] -> PDPT -> 1 GiB page) covers them
                    // once IA-32e mode is active.
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    // Detect a 64-bit-capable CPU before attempting the switch.
                    // CPUID.80000001:EDX[bit 29] (LM) is set only when the CPU
                    // implements long mode.  On a 32-bit-only CPU the switch
                    // below would #GP and triple-fault with no diagnostics, so
                    // print "No 64-bit CPU" via the BIOS teletype and halt.
                    let no_lm_msg =
                        self.string_label("No 64-bit CPU\r\n\0");
                    let err = self.asm.new_label();
                    // Robust long-mode detection: first read the max extended CPUID
                    // leaf (0x80000000) and only test the LM bit when leaf
                    // 0x80000001 is actually supported.  On a 32-bit-only CPU
                    // that leaf is absent and returns leaf-0/vendor EDX, whose bit 29
                    // is spuriously set -- without this guard we'd dive into the
                    // switch and triple-fault on any 32-bit host/CPU model.
                    let switch = self.asm.new_label();
                    self.asm.emit_slice(&[0x66, 0xB8]); // mov eax, 0x80000000
                    self.asm.emit_imm32(0x80000000);
                    self.asm.emit_slice(&[0x0F, 0xA2]); // cpuid
                    self.asm.emit_slice(&[0x66, 0x3D]); // cmp eax, 0x80000001
                    self.asm.emit_imm32(0x80000001);
                    self.asm.jcc_short_lab(0x72, err); // jb err (unsigned below)
                    self.asm.emit_slice(&[0x66, 0xB8]); // mov eax, 0x80000001
                    self.asm.emit_imm32(0x80000001);
                    self.asm.emit_slice(&[0x0F, 0xA2]); // cpuid
                    self.asm.emit_slice(&[0x66, 0xF7, 0xC2]); // test edx, imm32 (1<<29 = LM)
                    self.asm.emit_imm32(1 << 29);
                    self.asm.je_short_lab(err); // jz err (LM clear)
                    // All jumps to `err` are short (it is bound immediately below),
                    // so the short-jump widener is never exercised here.
                    self.asm.jmp_short_lab(switch); // skip the error block on success
                    self.asm.bind(err);
                    // BIOS teletopy loop: print "No 64-bit CPU", then halt.  This
                    // matches the stage-2 loader's `_dev_bios_teletopy` channel
                    // (int 0x10, VGA) used for all early-boot diagnostics and is
                    // visible on a VGA/CGA display.  DS=0 (set by _start's segment
                    // setup) so the string's 16-bit absolute address reads the
                    // image directly.
                    self.asm.emit_slice(&[0xBE]); // mov si, imm16 (msg address)
                    self.asm.imm16_label(no_lm_msg);
                    let msg_loop = self.asm.new_label();
                    let halt = self.asm.new_label();
                    self.asm.bind(msg_loop);
                    self.asm.emit_slice(&[0xAC]); // lodsb
                    self.asm.emit_slice(&[0x3C, 0x00]); // cmp al,0
                    self.asm.je_short_lab(halt);
                    self.asm.emit_slice(&[0xB4, 0x0E]); // mov ah,0x0E (teletopy)
                    self.asm.emit_slice(&[0xB3, 0x07]); // mov bl,0x07
                    self.asm.emit_slice(&[0xB7, 0x00]); // mov bh,0
                    self.asm.emit_slice(&[0xCD, 0x10]); // int 0x10 (VGA)
                    self.asm.jmp_short_lab(msg_loop);
                    self.asm.bind(halt);
                    self.asm.emit_slice(&[0xFA]); // cli
                    self.asm.emit_slice(&[0xF4]); // hlt
                    self.asm.jmp_short_lab(halt); // spin
                    // --- long mode IS supported: begin the switch ---
                    self.asm.bind(switch);
                    // Fast A20: in al,0x92; or al,2; out 0x92,al.
                    self.asm.emit_slice(&[0xE4, 0x92]);
                    self.asm.emit_slice(&[0x0C, 0x02]);
                    self.asm.emit_slice(&[0xE6, 0x92]);
                    // GDTR for the 16->32 GDT (gdt32), loaded inline below.
                    let gdtr32 = self.asm.new_label();
                    self.asm.emit_slice(&[0x0F, 0x01, 0x16]); // lgdt [moffs16]
                    self.asm.imm16_label(gdtr32);
                    // mov eax,cr0; or eax,1; mov cr0,eax   (CR0.PE)
                    self.asm.emit_slice(&[0x66, 0x0F, 0x20, 0xC0]);
                    self.asm.emit_slice(&[0x66, 0x83, 0xC8, 0x01]);
                    self.asm.emit_slice(&[0x66, 0x0F, 0x22, 0xC0]);
                    // far jmp 66 EA <imm32 trampoline> <0x08>
                    let tramp32 = self.asm.new_label();
                    self.asm.emit_slice(&[0x66, 0xEA]);
                    self.asm.imm32_label(tramp32);
                    self.asm.emit_slice(&[0x08, 0x00]);
                    // --- 32-bit trampoline (CS = 0x08, D = 1, base 0) ---
                    self.asm.bind(tramp32);
                    self.asm.emit_slice(&[0x66, 0xB8, 0x10, 0x00]); // mov ax, 0x10
                    self.asm.emit_slice(&[0x8E, 0xD8]); // mov ds, ax
                    self.asm.emit_slice(&[0x8E, 0xC0]); // mov es, ax
                    self.asm.emit_slice(&[0x8E, 0xD0]); // mov ss, ax
                    self.asm.emit_slice(&[0x8E, 0xE0]); // mov fs, ax
                    self.asm.emit_slice(&[0x8E, 0xE8]); // mov gs, ax
                    self.asm.emit_slice(&[0xBC]); // mov esp, imm32
                    self.asm.emit_imm32(0x00090000); // ring-0 stack (below VGA)
                    // Relocate the kernel from staging (0x8000) to 0x100000.
                    self.asm.emit_slice(&[0xBE]); // mov esi, 0x8000
                    self.asm.emit_imm32(0x00008000);
                    self.asm.emit_slice(&[0xBF]); // mov edi, 0x100000
                    self.asm.emit_imm32(0x00100000);
                    self.asm.emit_slice(&[0xB9]); // mov ecx, 1024 (8 sectors)
                    self.asm.emit_imm32(1024);
                    self.asm.emit_slice(&[0xF3, 0xA5]); // rep movsd
                    // eax = (entry_hi << 16) | entry_lo, then stash to 0x8FF8
                    self.asm.emit_slice(&[0x66, 0x8B, 0x45, 0x04]); // mov ax,[bp+4]
                    self.asm.emit_slice(&[0xC1, 0xE0, 0x10]); // shl eax, 16
                    self.asm.emit_slice(&[0x66, 0x8B, 0x45, 0x06]); // mov ax,[bp+6]
                    self.asm.emit_slice(&[0x89, 0x05, 0xF8, 0x8F, 0x00, 0x00]); // mov [0x8FF8],eax
                    // PML4 @ 0xA000: [0] -> PDPT @ 0xB000 (P|RW); rest 0.
                    self.asm.emit_slice(&[0xC7, 0x05, 0x00, 0xA0, 0x00, 0x00]);
                    self.asm.emit_imm32(0x0000B003);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x08, 0xA0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x10, 0xA0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x18, 0xA0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    // PDPT @ 0xB000: [0] = 1 GiB page @ 0 (P|RW|PS) -- see note.
                    self.asm.emit_slice(&[0xC7, 0x05, 0x00, 0xB0, 0x00, 0x00]);
                    self.asm.emit_imm32(0x00000083);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x08, 0xB0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x10, 0xB0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x18, 0xB0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    // CR3 = PML4; CR4 |= PAE; EFER.LME = 1.
                    self.asm.emit_slice(&[0xB8]); // mov eax, 0xA000
                    self.asm.emit_imm32(0x0000A000);
                    self.asm.emit_slice(&[0x0F, 0x22, 0xD8]); // mov cr3, eax
                    self.asm.emit_slice(&[0x0F, 0x20, 0xE0]); // mov eax, cr4
                    self.asm.emit_slice(&[0x83, 0xC8, 0x20]); // or eax, 0x20 (PAE)
                    self.asm.emit_slice(&[0x0F, 0x22, 0xE0]); // mov cr4, eax
                    self.asm.emit_slice(&[0xB9]); // mov ecx, 0xC0000080 (EFER)
                    self.asm.emit_imm32(0xC0000080);
                    self.asm.emit_slice(&[0x0F, 0x32]); // rdmsr
                    self.asm.emit_slice(&[0x81, 0xC8, 0x00, 0x01, 0x00, 0x00]); // or eax,0x100 (LME)
                    self.asm.emit_slice(&[0x0F, 0x30]); // wrmsr
                    // 64-bit GDT @ 0xC000 (null, code64 0x08, data64 0x10).
                    self.asm.emit_slice(&[0xC7, 0x05, 0x00, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x04, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x08, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0x0000FFFF); // code64 lo
                    self.asm.emit_slice(&[0xC7, 0x05, 0x0C, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0x00AF9A00); // code64 hi
                    self.asm.emit_slice(&[0xC7, 0x05, 0x10, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0x0000FFFF); // data64 lo
                    self.asm.emit_slice(&[0xC7, 0x05, 0x14, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0x008F9200); // data64 hi
                    // gdtr64 @ 0xC018: limit 0x17, base 0xC000, upper zeros.
                    self.asm.emit_slice(&[0xC7, 0x05, 0x18, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0xC0000017);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x1C, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0xC7, 0x05, 0x20, 0xC0, 0x00, 0x00]);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0x0F, 0x01, 0x15]); // lgdt [moffs32] -> [0xC018]
                    self.asm.emit_imm32(0x0000C018);
                    // CR0 |= PG, then far jmp 0x08 (64-bit code) -> tramp64.
                    self.asm.emit_slice(&[0x0F, 0x20, 0xC0]); // mov eax, cr0
                    self.asm.emit_slice(&[0x0D, 0x00, 0x00, 0x00, 0x80]); // or eax,0x80000000
                    self.asm.emit_slice(&[0x0F, 0x22, 0xC0]); // mov cr0, eax
                    let tramp64 = self.asm.new_label();
                    self.asm.emit_slice(&[0xEA]); // far jmp 0x08:tramp64 (32-bit offset)
                    self.asm.imm32_label(tramp64);
                    self.asm.emit_slice(&[0x08, 0x00]);
                    // --- 64-bit trampoline (CS = 0x08, L = 1, base 0) ---
                    self.asm.bind(tramp64);
                    self.asm.emit_slice(&[0x48, 0xBC]); // mov rax, 0x90000
                    self.asm.emit_imm32(0x00090000);
                    self.asm.emit_imm32(0);
                    self.asm.emit_slice(&[0x48, 0x8B, 0x04, 0x25, 0xF8, 0x8F, 0x00, 0x00]); // mov rax,[0x8FF8]
                    self.asm.emit_slice(&[0xFF, 0xE0]); // jmp rax
                    // --- 16->32 GDT (inline) + GDTR32, read by the lgdt above ---
                    self.asm.bind(gdtr32);
                    self.asm.emit_imm16(23); // limit = 3 * 8 - 1
                    let gdt32 = self.asm.new_label();
                    self.asm.imm32_label(gdt32); // base = gdt32 (load_base + off)
                    self.asm.bind(gdt32);
                    self.asm.emit_slice(&[0x00; 8]); // null
                    self.asm.emit_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9A, 0xCF, 0x00]); // code32
                    self.asm.emit_slice(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00]); // data32
                }
                "_dev_outw" => {
                    // push bp; mov bp,sp; mov dx,[bp+6]; mov ax,[bp+4]; out dx,ax; ret
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load16_bp(Reg16::Dx, 6);  // port
                    self.asm.load16_bp(Reg16::Ax, 4);  // value
                    self.asm.out_dx_ax();
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_inw" => {
                    // push bp; mov bp,sp; mov dx,[bp+4]; in ax,dx; ret
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load16_bp(Reg16::Dx, 4);  // port
                    self.asm.in_ax_dx();
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_iret" => {
                    self.asm.iret();
                }
                "_dev_sti" => {
                    self.asm.sti();
                }
                "_dev_cli" => {
                    self.asm.cli();
                }
                "_dev_outb" => {
                    // push bp; mov bp,sp; mov dx,[bp+6]; mov al,[bp+4]; out dx,al; ret
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load16_bp(Reg16::Dx, 6);  // port
                    self.asm.load8_bp(Reg8::Al, 4);    // value
                    self.asm.out_dx_al();
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
                }
                "_dev_inb" => {
                    // push bp; mov bp,sp; mov dx,[bp+4]; in al,dx; xor ah,ah; ret
                    self.asm.push(Reg16::Bp);
                    self.asm.mov16_rr(Reg16::Bp, Reg16::Sp);
                    self.asm.load16_bp(Reg16::Dx, 4);  // port
                    self.asm.in_al_dx();
                    self.asm.xor_ah_ah();
                    self.asm.mov16_rm(Reg16::Sp, Reg16::Bp);
                    self.asm.pop(Reg16::Bp);
                    self.asm.ret();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `_dev_enter_long_mode` must emit the validated 16->32->64 switch bytes:
    /// the 32-bit trampoline (CR3/CR4/PAE/EFER.LME via wrmsr/lgdt/CR0.PG/far
    /// jmp to a 64-bit code segment) and the validated 64-bit trampoline
    /// `mov rax,0x90000; mov rax,[0x8FF8]; jmp rax` plus the 1 GiB PDPT page
    /// (0x83) that makes the PAE-3-level transition window resolve.
    #[test]
    fn long_mode_builtin_emits_validated_bytes() {
        let start = Func {
            name: "_start".to_string(),
            params: vec![],
            ret: Type::Void,
            body: vec![Stmt::Expr(Expr::new(
                ExprKind::Call {
                    func: "_dev_enter_long_mode".to_string(),
                    args: vec![
                        Expr::new(ExprKind::Lit(Literal::Int(0)), Type::U16),
                        Expr::new(ExprKind::Lit(Literal::Int(0x10)), Type::U16),
                    ],
                },
                Type::Void,
            ))],
        };
        let prog = Program {
            name: "t".to_string(),
            structs: vec![],
            enums: vec![],
            globals: vec![],
            externs: vec![],
            funcs: vec![start],
            hosted: false,
            target: Some("x86_16".to_string()),
            arch: Some("x86_16".to_string()),
            obj_format: Some("flat".to_string()),
            config: None,
        };
        let bytes = compile_program(&prog).expect("compile");

        let find = |sub: &[u8]| bytes.windows(sub.len()).position(|w| w == sub);

        // cpuid 0x80000000; cmp eax,0x80000001; jb -> error block.  The error
        // block sits adjacent to the checks (all branches to it are short), so a
        // 32-bit-only CPU prints "No 64-bit CPU" and halts instead of triple-
        // faulting in the long-mode switch.
        assert!(
            find(&[0x66, 0xB8, 0x00, 0x00, 0x00, 0x80, 0x0F, 0xA2]).is_some(),
            "missing CPUID max-extended-leaf probe (0x80000000)"
        );
        assert!(
            find(&[0x66, 0x3D, 0x01, 0x00, 0x00, 0x80, 0x72]).is_some(),
            "missing: cmp eax,0x80000001; jb err (32-bit CPU guard)"
        );
        assert!(
            find(&[0x66, 0xF7, 0xC2, 0x00, 0x00, 0x00, 0x20]).is_some(),
            "missing: test edx, (1<<29) (LM bit) -- 0x66 0xF7 0xC2 (test r32,imm32), not 0x81 (xor)"
        );
        assert!(find("No 64-bit CPU\r\n\0".as_bytes()).is_some(), "missing 32-bit CPU guard message");
        // The error block prints the message via BIOS teletopy (int 0x10, VGA),
        // matching the stage-2 loader's `_dev_bios_teletopy` channel: lodsb; cmp
        // al,0; je halt; mov ah,0x0E; mov bl,0x07; mov bh,0; int 0x10; jmp loop;
        // then cli; hlt; spin.  Single-VGA-channel keeps the output un-doubled
        // under `-nographic` (which mirrors both VGA and COM1 to stdio).
        assert!(find(&[0xAC, 0x3C, 0x00]).is_some(), "missing lodsb + cmp al,0 message loop");
        assert!(
            find(&[0xB4, 0x0E, 0xB3, 0x07, 0xB7, 0x00, 0xCD, 0x10]).is_some(),
            "missing BIOS teletopy int 0x10 in the guard error block"
        );

        // 16-bit prologue (success path): lgdt [abs16]; CR0.PE; 32-bit far jmp.
        assert!(
            find(&[0x0F, 0x01, 0x16]).is_some(),
            "missing 16->32 lgdt"
        );
        assert!(find(&[0x66, 0xEA]).is_some(), "missing far jmp 66 EA to 32-bit trampoline");
        assert!(
            find(&[0x66, 0x0F, 0x22, 0xC0]).is_some(),
            "missing 16-bit-prologue CR0.PE (mov cr0,eax)"
        );
        // 32-bit trampoline: CR3 = PML4 (mov eax,0xA000; mov cr3,eax).
        assert!(
            find(&[0xB8, 0x00, 0xA0, 0x00, 0x00, 0x0F, 0x22, 0xD8]).is_some(),
            "missing CR3 <- 0xA000 setup"
        );
        // CR4 |= PAE (mov eax,cr4; or eax,0x20; mov cr4,eax).
        assert!(
            find(&[0x0F, 0x20, 0xE0, 0x83, 0xC8, 0x20, 0x0F, 0x22, 0xE0]).is_some(),
            "missing CR4 PAE enable"
        );
        // EFER.LME via wrmsr: ecx=IA32_EFER (0xC0000080), rdmsr; or eax,0x100;
        // wrmsr.  (0xC0000080 -- NOT 0x40000080, which leaves LME clear and
        // makes the far jmp to a 64-bit code segment #GP / triple-fault.)
        assert!(
            find(&[0xB9, 0x80, 0x00, 0x00, 0xC0]).is_some(),
            "missing mov ecx,0xC0000080 (EFER); got the wrong MSR and LME stays clear"
        );
        assert!(
            find(&[0x81, 0xC8, 0x00, 0x01, 0x00, 0x00, 0x0F, 0x30]).is_some(),
            "missing EFER.LME via wrmsr"
        );
        // 1 GiB PDPT page covering 0..1 GiB (identity map) -- makes the PAE
        // 3-level transition window resolve as a 2 MiB PDE.
        assert!(
            find(&[0xC7, 0x05, 0x00, 0xB0, 0x00, 0x00, 0x83, 0x00, 0x00, 0x00]).is_some(),
            "missing 1 GiB PDPT[0]=0x83 page"
        );
        // lgdt [moffs32] (loads the 64-bit GDT) + CR0 |= PG + far jmp to 64-bit CS.
        assert!(
            find(&[0x0F, 0x20, 0xC0, 0x0D, 0x00, 0x00, 0x00, 0x80, 0x0F, 0x22, 0xC0, 0xEA]).is_some(),
            "missing CR0.PG + far jmp to 64-bit code"
        );
        // Validated 64-bit trampoline: mov rax,0x90000; mov rax,[0x8FF8]; jmp rax.
        assert!(
            find(&[0x48, 0xBC, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00]).is_some(),
            "missing mov rax,0x90000"
        );
        assert!(
            find(&[0x48, 0x8B, 0x04, 0x25, 0xF8, 0x8F, 0x00, 0x00, 0xFF, 0xE0]).is_some(),
            "missing validated mov rax,[0x8FF8]; jmp rax trampoline"
        );
    }
}

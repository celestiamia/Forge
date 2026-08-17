use super::*;

impl<'p> CodeGen<'p> {
    /// Emit the freestanding runtime helpers the program declared `extern`
    /// (see `FREESTANDING_FUNCS`).  These are minimal port-I/O and halt
    /// primitives: no Linux syscalls, no BIOS.  They take their arguments
    /// from the stack with a regular cdecl frame (`push ebp; mov ebp,esp`
    /// prologue) because the caller pushes real values at the call site.
    pub(super) fn emit_freestanding_runtime(&mut self) -> Result<()> {
        if let Some(&lab) = self.func_labels.get("_dev_outb") {
            self.bind_label(lab);
            // push ebp; mov ebp, esp
            // mov dx, [ebp+8]   (port)
            // mov al, [ebp+12]  (value)
            // out dx, al
            // leave; ret
            self.asm.append_bytes(&[0x55, 0x89, 0xE5]);
            self.asm.append_bytes(&[0x66, 0x8B, 0x55, 0x08]);
            self.asm.append_bytes(&[0x8A, 0x45, 0x0C]);
            self.asm.append_bytes(&[0xEE]);
            self.asm.append_bytes(&[0xC9, 0xC3]);
        }
        if let Some(&lab) = self.func_labels.get("_dev_inb") {
            self.bind_label(lab);
            // push ebp; mov ebp, esp
            // mov dx, [ebp+8]   (port)
            // in al, dx
            // movzx eax, al
            // leave; ret
            self.asm.append_bytes(&[0x55, 0x89, 0xE5]);
            self.asm.append_bytes(&[0x66, 0x8B, 0x55, 0x08]);
            self.asm.append_bytes(&[0xEC]);
            self.asm.append_bytes(&[0x0F, 0xB6, 0xC0]);
            self.asm.append_bytes(&[0xC9, 0xC3]);
        }
        if let Some(&lab) = self.func_labels.get("_dev_halt") {
            self.bind_label(lab);
            // cli; hlt
            self.asm.append_bytes(&[0xFA, 0xF4]);
        }
        Ok(())
    }

    pub(super) fn emit_runtime(&mut self, start_label: u32) -> Result<()> {
        let e = *self.func_labels.get("_dev_exit").unwrap();
        self.bind_label(e);
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?;
        self.asm.mov(Reg::Eax, 1i32)?; // sys_exit
        self.asm.int(0x80)?;

        let s = *self.func_labels.get("_dev_socket").unwrap();
        self.bind_label(s);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.sub(Reg::Esp, 16i32)?; // 4-slot args array
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8))?; // domain
        self.asm.mov(Mem::base_disp(Reg::Ebp, -16), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12))?; // type
        self.asm.mov(Mem::base_disp(Reg::Ebp, -12), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 16))?; // protocol
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax)?;
        self.asm.mov(Reg::Ebx, 1i32)?;
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -16))?;
        self.asm.mov(Reg::Eax, 102i32)?; // sys_socketcall
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let b = *self.func_labels.get("_dev_bind").unwrap();
        self.bind_label(b);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.sub(Reg::Esp, 16i32)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Mem::base_disp(Reg::Ebp, -16), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12))?; // addr
        self.asm.mov(Mem::base_disp(Reg::Ebp, -12), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 16))?; // addrlen
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax)?;
        self.asm.mov(Reg::Ebx, 2i32)?;
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -16))?;
        self.asm.mov(Reg::Eax, 102i32)?;
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let li = *self.func_labels.get("_dev_listen").unwrap();
        self.bind_label(li);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.sub(Reg::Esp, 8i32)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12))?; // backlog
        self.asm.mov(Mem::base_disp(Reg::Ebp, -4), Reg::Eax)?;
        self.asm.mov(Reg::Ebx, 4i32)?;
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -8))?;
        self.asm.mov(Reg::Eax, 102i32)?;
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let a = *self.func_labels.get("_dev_accept").unwrap();
        self.bind_label(a);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.sub(Reg::Esp, 16i32)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Mem::base_disp(Reg::Ebp, -16), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12))?; // addr
        self.asm.mov(Mem::base_disp(Reg::Ebp, -12), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 16))?; // addrlen
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax)?;
        self.asm.mov(Reg::Ebx, 5i32)?;
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -16))?;
        self.asm.mov(Reg::Eax, 102i32)?;
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let re = *self.func_labels.get("_dev_read").unwrap();
        self.bind_label(re);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 3i32)?;
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12))?; // buf
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16))?; // count
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let w = *self.func_labels.get("_dev_write").unwrap();
        self.bind_label(w);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 4i32)?;
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12))?; // buf
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16))?; // count
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let cl = *self.func_labels.get("_dev_close").unwrap();
        self.bind_label(cl);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 6i32)?;
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let op = *self.func_labels.get("_dev_open").unwrap();
        self.bind_label(op);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 5i32)?; // sys_open
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // path
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12))?; // flags
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16))?; // mode
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let ls = *self.func_labels.get("_dev_lseek").unwrap();
        self.bind_label(ls);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 19i32)?; // sys_lseek
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12))?; // offset
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16))?; // whence
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let un = *self.func_labels.get("_dev_unlink").unwrap();
        self.bind_label(un);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 10i32)?; // sys_unlink
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // path
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let fk = *self.func_labels.get("_dev_fork").unwrap();
        self.bind_label(fk);
        self.asm.mov(Reg::Eax, 2i32)?; // sys_fork
        self.asm.int(0x80)?;
        self.asm.ret()?;

        let wp = *self.func_labels.get("_dev_waitpid").unwrap();
        self.bind_label(wp);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 114i32)?; // sys_wait4 (waitpid semantics)
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // pid
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12))?; // status
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16))?; // options
        self.asm.mov(Reg::Esi, 0i32)?; // rusage = NULL
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let fc = *self.func_labels.get("_dev_fcntl").unwrap();
        self.bind_label(fc);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Eax, 55i32)?; // sys_fcntl (i386 __NR_fcntl=55; 72 is x86_64-only, 72=i386 sigsuspend)
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12))?; // cmd
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16))?; // arg
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let ss = *self.func_labels.get("_dev_setsockopt").unwrap();
        self.bind_label(ss);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.sub(Reg::Esp, 20i32)?; // 5-slot args array for socketcall
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8))?; // fd
        self.asm.mov(Mem::base_disp(Reg::Ebp, -20), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12))?; // level
        self.asm.mov(Mem::base_disp(Reg::Ebp, -16), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 16))?; // optname
        self.asm.mov(Mem::base_disp(Reg::Ebp, -12), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 20))?; // optval
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax)?;
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 24))?; // optlen
        self.asm.mov(Mem::base_disp(Reg::Ebp, -4), Reg::Eax)?;
        self.asm.mov(Reg::Ebx, 14i32)?; // SYS_SETSOCKOPT
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -20))?;
        self.asm.mov(Reg::Eax, 102i32)?; // sys_socketcall
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let p = *self.func_labels.get("_dev_puts").unwrap();
        self.bind_label(p);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.push(Reg::Esi)?; // callee-saved
        self.asm.push(Reg::Edi)?; // callee-saved
        self.asm.mov(Reg::Esi, Mem::base_disp(Reg::Ebp, 8))?; // s
        self.asm.mov(Reg::Edi, Reg::Esi)?; // original pointer
        self.asm.xor(Reg::Ecx, Reg::Ecx)?; // length
        let loop_lab = self.asm.new_label();
        let done_lab = self.asm.new_label();
        self.bind_label(loop_lab);
        self.asm.movzx8(Reg::Eax, Mem::base(Reg::Esi))?;
        self.asm.test(Reg::Eax, Reg::Eax)?;
        self.asm.je(done_lab)?;
        self.asm.inc(Reg::Esi)?;
        self.asm.inc(Reg::Ecx)?;
        self.asm.jmp(loop_lab)?;
        self.bind_label(done_lab);
        self.asm.mov(Reg::Eax, 4i32)?; // sys_write
        self.asm.mov(Reg::Ebx, 1i32)?; // stdout
        self.asm.mov(Reg::Edx, Reg::Ecx)?; // len
        self.asm.mov(Reg::Ecx, Reg::Edi)?; // buf
        self.asm.int(0x80)?;
        self.asm.pop(Reg::Edi)?;
        self.asm.pop(Reg::Esi)?;
        self.asm.leave();
        self.asm.ret()?;

        let pc = *self.func_labels.get("_dev_putchar").unwrap();
        self.bind_label(pc);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.push(Reg::Eax)?; // allocate 4-byte slot for the character
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8))?; // c
        self.asm.mov(Mem::base_disp(Reg::Ebp, -4), Reg::Eax)?; // store low byte (and zeros)
        self.asm.mov(Reg::Eax, 4i32)?; // sys_write
        self.asm.mov(Reg::Ebx, 1i32)?; // stdout
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -4))?; // buffer address
        self.asm.mov(Reg::Edx, 1i32)?; // count
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let c = *self.func_labels.get("_dev_getchar").unwrap();
        self.bind_label(c);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.sub(Reg::Esp, 4i32)?; // buffer
        self.asm.mov(Reg::Eax, 3i32)?; // sys_read
        self.asm.mov(Reg::Ebx, 0i32)?; // stdin
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -4))?;
        self.asm.mov(Reg::Edx, 1i32)?;
        self.asm.int(0x80)?;
        self.asm.cmp(Reg::Eax, 1i32)?;
        let ok_lab = self.asm.new_label();
        self.asm.je(ok_lab)?;
        self.asm.mov(Reg::Eax, -1i32)?;
        self.asm.leave();
        self.asm.ret()?;
        self.bind_label(ok_lab);
        self.asm.movzx8(Reg::Eax, Mem::base_disp(Reg::Ebp, -4))?;
        self.asm.leave();
        self.asm.ret()?;

        let r = *self.func_labels.get("_dev_rand").unwrap();
        self.bind_label(r);
        self.asm.rdtsc()?; // edx:eax
        self.asm.ret()?;

        let t = *self.func_labels.get("_dev_gettimeofday").unwrap();
        self.bind_label(t);
        self.asm.push(Reg::Ebp)?;
        self.asm.mov(Reg::Ebp, Reg::Esp)?;
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8))?; // tv
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12))?; // tz
        self.asm.mov(Reg::Eax, 78i32)?; // sys_gettimeofday (i386)
        self.asm.int(0x80)?;
        self.asm.leave();
        self.asm.ret()?;

        let lb = *self.func_labels.get("_dev_lfence").unwrap();
        self.bind_label(lb);
        self.asm.lfence()?;
        self.asm.ret()?;

        let sb = *self.func_labels.get("_dev_sfence").unwrap();
        self.bind_label(sb);
        self.asm.sfence()?;
        self.asm.ret()?;

        let mb = *self.func_labels.get("_dev_mfence").unwrap();
        self.bind_label(mb);
        self.asm.mfence()?;
        self.asm.ret()?;

        if let Some(&a) = self.func_labels.get("_dev_alloc") {
            self.bind_label(a);
            self.asm.push(Reg::Ebp)?;
            self.asm.mov(Reg::Ebp, Reg::Esp)?;
            // [ebp-4] = &heap_state (anchor), [ebp-8] = request size.
            self.asm.sub(Reg::Esp, 8i32)?;
            self.heap_state_reg(Reg::Eax)?; // eax = &heap_state (patched)
            self.asm.mov(Mem::base_disp(Reg::Ebp, -4), Reg::Eax)?;

            // ---- lazy heap initialisation -----------------------------------
            self.asm
                .mov(Reg::Ecx, Mem::base_disp(Reg::Eax, HP_LIMIT as i32))?;
            let after_init = self.asm.new_label();
            self.asm.test(Reg::Ecx, Reg::Ecx)?;
            self.asm.jne(after_init)?; // already initialised
            // heap_base was pre-patched to `arena_base`; limit = base + size.
            self.asm
                .mov(Reg::Edx, Mem::base_disp(Reg::Eax, HP_BASE as i32))?;
            let heap_size = self.heap_size();
            self.asm.mov(Reg::Ecx, heap_size as i32)?;
            self.asm.add(Reg::Ecx, Reg::Edx)?;
            self.asm
                .mov(Mem::base_disp(Reg::Eax, HP_LIMIT as i32), Reg::Ecx)?; // limit
            // Whole arena = one free block: header at base, size = arena - hdr.
            self.asm
                .mov(Reg::Ecx, (heap_size as i32) - H_HDR_SIZE as i32)?;
            self.asm.mov(Mem::base(Reg::Edx), Reg::Ecx)?; // *(base) = size (free)
            self.asm
                .lea(Reg::Ecx, Mem::base_disp(Reg::Edx, H_HDR_SIZE as i32))?;
            self.asm
                .mov(Mem::base_disp(Reg::Eax, HP_FREE_HEAD as i32), Reg::Ecx)?; // free_head = base+4
            self.bind_label(after_init);

            // ---- round request up to 4 bytes (min 4) ------------------------
            self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 8))?; // size
            self.asm.add(Reg::Ecx, 3i32)?;
            self.asm.and(Reg::Ecx, !3i32)?; // 4-byte aligned
            let min_ok = self.asm.new_label();
            self.asm.cmp(Reg::Ecx, 4i32)?;
            self.asm.jcc(Cond::Ae, min_ok)?;
            self.asm.mov(Reg::Ecx, 4i32)?;
            self.bind_label(min_ok);
            self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Ecx)?; // store req

            // ---- first-fit search over the free list -------------------------
            let walk = self.asm.new_label();
            let walk_next = self.asm.new_label();
            let nofit = self.asm.new_label();
            let done = self.asm.new_label();
            let split = self.asm.new_label();
            let tw_head = self.asm.new_label();
            let tw_done = self.asm.new_label();
            let sp_head = self.asm.new_label();
            let sp_done = self.asm.new_label();

            self.bind_label(walk);
            self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, -4))?; // &state
            self.asm
                .mov(Reg::Eax, Mem::base_disp(Reg::Eax, HP_FREE_HEAD as i32))?; // cur
            self.asm.xor(Reg::Edx, Reg::Edx)?; // prev = 0
            self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, -8))?; // req

            let walk_loop = self.asm.new_label();
            self.bind_label(walk_loop);
            self.asm.test(Reg::Eax, Reg::Eax)?;
            self.asm.je(nofit)?; // cur == 0 -> exhausted
            self.asm
                .mov(Reg::Edi, Mem::base_disp(Reg::Eax, -(H_HDR_SIZE as i32)))?; // hdr
            self.asm.mov(Reg::Esi, Reg::Edi)?;
            self.asm.and(Reg::Esi, !3i32)?; // size
            self.asm.mov(Reg::Ebx, Mem::base(Reg::Eax))?; // nxt = *(cur)
            self.asm.cmp(Reg::Esi, Reg::Ecx)?; // size vs req
            self.asm.jcc(Cond::B, walk_next)?; // too small

            // split if (size - req) >= SPLIT_THRESHOLD
            self.asm.mov(Reg::Edi, Reg::Esi)?;
            self.asm.sub(Reg::Edi, Reg::Ecx)?; // edi = size - req
            self.asm.cmp(Reg::Edi, SPLIT_THRESHOLD as i32)?;
            self.asm.jcc(Cond::Ae, split)?;

            // ---- take_whole: unlink cur, used hdr = size|USED, return cur ----
            self.asm.cmp(Reg::Edx, 0i32)?;
            self.asm.je(tw_head)?;
            self.asm.mov(Mem::base(Reg::Edx), Reg::Ebx)?; // *(prev) = nxt
            self.asm.jmp(tw_done)?;
            self.bind_label(tw_head);
            self.asm.mov(Reg::Edi, Mem::base_disp(Reg::Ebp, -4))?; // &state
            self.asm
                .mov(Mem::base_disp(Reg::Edi, HP_FREE_HEAD as i32), Reg::Ebx)?; // free_head = nxt
            self.bind_label(tw_done);
            self.asm.mov(Reg::Edi, Reg::Esi)?;
            self.asm.or(Reg::Edi, H_USED as i32)?;
            self.asm
                .mov(Mem::base_disp(Reg::Eax, -(H_HDR_SIZE as i32)), Reg::Edi)?;
            self.asm.jmp(done)?;

            // ---- split: unlink cur, splice in the remainder -------------------
            self.bind_label(split);
            // rem = cur + HDR + req ; *(rem) = nxt ; *(rem-HDR) = size-req-HDR
            self.asm
                .lea(Reg::Edi, Mem::base_disp(Reg::Eax, H_HDR_SIZE as i32))?;
            self.asm.add(Reg::Edi, Reg::Ecx)?; // edi = rem
            self.asm.mov(Mem::base(Reg::Edi), Reg::Ebx)?; // *(rem) = nxt
            self.asm.sub(Reg::Esi, Reg::Ecx)?;
            self.asm.sub(Reg::Esi, H_HDR_SIZE as i32)?; // rem size = size - req - HDR
            self.asm
                .mov(Mem::base_disp(Reg::Edi, -(H_HDR_SIZE as i32)), Reg::Esi)?;
            self.asm.cmp(Reg::Edx, 0i32)?;
            self.asm.je(sp_head)?;
            self.asm.mov(Mem::base(Reg::Edx), Reg::Edi)?; // *(prev) = rem
            self.asm.jmp(sp_done)?;
            self.bind_label(sp_head);
            self.asm.mov(Reg::Esi, Mem::base_disp(Reg::Ebp, -4))?; // &state
            self.asm
                .mov(Mem::base_disp(Reg::Esi, HP_FREE_HEAD as i32), Reg::Edi)?; // free_head = rem
            self.bind_label(sp_done);
            // used header at cur-HDR = req | USED
            self.asm.mov(Reg::Edi, Reg::Ecx)?;
            self.asm.or(Reg::Edi, H_USED as i32)?;
            self.asm
                .mov(Mem::base_disp(Reg::Eax, -(H_HDR_SIZE as i32)), Reg::Edi)?;
            self.asm.jmp(done)?;

            // ---- advance over a too-small free block --------------------------
            self.bind_label(walk_next);
            self.asm.mov(Reg::Edx, Reg::Eax)?; // prev = cur
            self.asm.mov(Reg::Eax, Reg::Ebx)?; // cur = nxt
            self.asm.jmp(walk_loop)?;

            // ---- free list exhausted: return null -----------------------------
            self.bind_label(nofit);
            self.asm.xor(Reg::Eax, Reg::Eax)?; // NULL

            self.bind_label(done);
            self.asm.leave();
            self.asm.ret()?;
        }

        if let Some(&f) = self.func_labels.get("_dev_free") {
            self.bind_label(f);
            self.asm.push(Reg::Ebp)?;
            self.asm.mov(Reg::Ebp, Reg::Esp)?;
            self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 8))?; // p
            self.asm.test(Reg::Ecx, Reg::Ecx)?;
            let ret = self.asm.new_label();
            self.asm.je(ret)?; // free(null) is a no-op
            // mark free: clear the USED flag bit -> hdr = size
            self.asm
                .mov(Reg::Edx, Mem::base_disp(Reg::Ecx, -(H_HDR_SIZE as i32)))?;
            self.asm.and(Reg::Edx, !3i32)?;
            self.asm
                .mov(Mem::base_disp(Reg::Ecx, -(H_HDR_SIZE as i32)), Reg::Edx)?;
            // prepend: next = free_head; *(p) = next; free_head = p
            self.heap_state_reg(Reg::Eax)?; // eax = &heap_state (patched)
            self.asm
                .mov(Reg::Edx, Mem::base_disp(Reg::Eax, HP_FREE_HEAD as i32))?;
            self.asm.mov(Mem::base(Reg::Ecx), Reg::Edx)?;
            self.asm
                .mov(Mem::base_disp(Reg::Eax, HP_FREE_HEAD as i32), Reg::Ecx)?;
            self.bind_label(ret);
            self.asm.leave();
            self.asm.ret()?;
        }

        self.bind_label(start_label);
        let main_name = self
            .prog
            .config
            .as_ref()
            .map(|c| c.entry.as_str())
            .unwrap_or("_forge_main");
        let main_lab = *self
            .func_labels
            .get(main_name)
            .ok_or_else(|| anyhow::anyhow!("hosted mode requires a {} function", main_name))?;
        // cdecl startup: the kernel leaves argc at [esp] and the argv array at
        // [esp+4].  Load argc, take the address of the argv array, push argv
        // then argc, so main receives (argc, argv); a `pub def main()` with no
        // parameters simply ignores them.
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Esp, 0))?;
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Esp, 4))?;
        self.asm.push(Reg::Ecx)?;
        self.asm.push(Reg::Eax)?;
        self.asm.call(main_lab)?;
        self.asm.add(Reg::Esp, 8i32)?;
        self.asm.mov(Reg::Ebx, Reg::Eax)?;
        self.asm.mov(Reg::Eax, 1i32)?;
        self.asm.int(0x80)?;

        Ok(())
    }
}

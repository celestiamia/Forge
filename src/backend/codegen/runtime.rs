use super::*;

impl<'p> CodeGen<'p> {
    pub(super) fn emit_runtime(&mut self, start_label: u32) -> Result<()> {
        self.emit_dev_write()?;
        self.emit_dev_puts()?;
        self.emit_dev_putchar()?;
        self.emit_dev_getchar()?;
        self.emit_dev_rand()?;
        self.emit_dev_exit()?;
        self.emit_dev_lfence()?;
        self.emit_dev_sfence()?;
        self.emit_dev_mfence()?;
        self.emit_dev_socket()?;
        self.emit_dev_bind()?;
        self.emit_dev_listen()?;
        self.emit_dev_accept()?;
        self.emit_dev_read()?;
        self.emit_dev_close()?;
        self.emit_dev_open()?;
        self.emit_dev_lseek()?;
        self.emit_dev_unlink()?;
        self.emit_dev_fork()?;
        self.emit_dev_fcntl()?;
        self.emit_dev_setsockopt()?;
        self.emit_gc_runtime()?;
        self.emit_forge_pow();
        self.emit_entry_point(start_label)?;
        Ok(())
    }

    fn emit_dev_write(&mut self) -> Result<()> {
        let w = *self.func_labels.get("_dev_write").unwrap();
        self.bind_label(w);
        self.asm.mov(Reg::Rax, 1i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_puts(&mut self) -> Result<()> {
        let p = *self.func_labels.get("_dev_puts").unwrap();
        self.bind_label(p);
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.push(Reg::Rdi);
        self.asm.mov(Reg::Rsi, Reg::Rdi);
        self.asm.xor(Reg::Rcx, Reg::Rcx);
        let loop_lab = self.asm.new_label();
        let done_lab = self.asm.new_label();
        self.bind_label(loop_lab);
        self.asm.movzx8(Reg::Rdx, Mem::base(Reg::Rsi));
        self.asm.test(Reg::Rdx, Reg::Rdx);
        self.asm.je(done_lab);
        self.asm.inc(Reg::Rsi);
        self.asm.inc(Reg::Rcx);
        self.asm.jmp(loop_lab);
        self.bind_label(done_lab);
        self.asm.mov(Reg::Rdi, 1i32);
        self.asm.mov(Reg::Rsi, Mem::base_disp(Reg::Rbp, -8));
        self.asm.mov(Reg::Rdx, Reg::Rcx);
        let write_lab = *self.func_labels.get("_dev_write").unwrap();
        self.asm.call(write_lab);
        self.asm.leave();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_putchar(&mut self) -> Result<()> {
        let pc = *self.func_labels.get("_dev_putchar").unwrap();
        self.bind_label(pc);
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.push(Reg::Rdi);
        self.asm.mov(Reg::Rdi, 1i32);
        self.asm.lea(Reg::Rsi, Mem::base_disp(Reg::Rbp, -8));
        self.asm.mov(Reg::Rdx, 1i32);
        self.asm.mov(Reg::Rax, 1i32);
        self.asm.syscall();
        self.asm.leave();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_getchar(&mut self) -> Result<()> {
        let c = *self.func_labels.get("_dev_getchar").unwrap();
        self.bind_label(c);
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.sub(Reg::Rsp, 16);
        self.asm.mov(Reg::Rdi, 0i32);
        self.asm.lea(Reg::Rsi, Mem::base_disp(Reg::Rbp, -8));
        self.asm.mov(Reg::Rdx, 1i32);
        self.asm.mov(Reg::Rax, 0i32);
        self.asm.syscall();
        self.asm.cmp(Reg::Rax, 1i32);
        let ok_lab = self.asm.new_label();
        let eof_lab = self.asm.new_label();
        self.asm.je(ok_lab);
        self.bind_label(eof_lab);
        self.asm.mov(Reg::Rax, -1i32);
        self.asm.leave();
        self.asm.ret();
        self.bind_label(ok_lab);
        self.asm.movzx8(Reg::Rax, Mem::base_disp(Reg::Rbp, -8));
        self.asm.leave();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_rand(&mut self) -> Result<()> {
        let r = *self.func_labels.get("_dev_rand").unwrap();
        self.bind_label(r);
        let seed_addr_offset = self.asm.len() + 2;
        self.rand_seed_patch = Some(seed_addr_offset);
        self.asm.movabs(Reg::Rax, 0);
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.mov(Reg::R11, Mem::base(Reg::Rax));
        self.asm.mov(Reg::R10, 1103515245i32);
        self.asm.imul(Reg::R11, Reg::R10);
        self.asm.add(Reg::R11, 12345i32);
        self.asm.mov(Mem::base(Reg::Rax), Reg::R11);
        self.asm.shr(Reg::R11, 16i8);
        self.asm.mov(Reg::Rax, 0x7fffffffi32);
        self.asm.and(Reg::Rax, Reg::R11);
        self.asm.leave();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_exit(&mut self) -> Result<()> {
        let e = *self.func_labels.get("_dev_exit").unwrap();
        self.bind_label(e);
        self.asm.mov(Reg::Rax, 60i32);
        self.asm.syscall();
        Ok(())
    }

    fn emit_dev_lfence(&mut self) -> Result<()> {
        let lb = *self.func_labels.get("_dev_lfence").unwrap();
        self.bind_label(lb);
        self.asm.lfence();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_sfence(&mut self) -> Result<()> {
        let sb = *self.func_labels.get("_dev_sfence").unwrap();
        self.bind_label(sb);
        self.asm.sfence();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_mfence(&mut self) -> Result<()> {
        let mb = *self.func_labels.get("_dev_mfence").unwrap();
        self.bind_label(mb);
        self.asm.mfence();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_socket(&mut self) -> Result<()> {
        let s = *self.func_labels.get("_dev_socket").unwrap();
        self.bind_label(s);
        self.asm.mov(Reg::Rax, 41i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_bind(&mut self) -> Result<()> {
        let b = *self.func_labels.get("_dev_bind").unwrap();
        self.bind_label(b);
        self.asm.mov(Reg::Rax, 49i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_listen(&mut self) -> Result<()> {
        let li = *self.func_labels.get("_dev_listen").unwrap();
        self.bind_label(li);
        self.asm.mov(Reg::Rax, 50i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_accept(&mut self) -> Result<()> {
        let a = *self.func_labels.get("_dev_accept").unwrap();
        self.bind_label(a);
        self.asm.mov(Reg::Rax, 43i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_read(&mut self) -> Result<()> {
        let re = *self.func_labels.get("_dev_read").unwrap();
        self.bind_label(re);
        self.asm.mov(Reg::Rax, 0i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_close(&mut self) -> Result<()> {
        let cl = *self.func_labels.get("_dev_close").unwrap();
        self.bind_label(cl);
        self.asm.mov(Reg::Rax, 3i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_open(&mut self) -> Result<()> {
        let op = *self.func_labels.get("_dev_open").unwrap();
        self.bind_label(op);
        self.asm.mov(Reg::Rax, 2i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_lseek(&mut self) -> Result<()> {
        let ls = *self.func_labels.get("_dev_lseek").unwrap();
        self.bind_label(ls);
        self.asm.mov(Reg::Rax, 8i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_unlink(&mut self) -> Result<()> {
        let un = *self.func_labels.get("_dev_unlink").unwrap();
        self.bind_label(un);
        self.asm.mov(Reg::Rax, 87i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_fork(&mut self) -> Result<()> {
        let fk = *self.func_labels.get("_dev_fork").unwrap();
        self.bind_label(fk);
        self.asm.mov(Reg::Rax, 57i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_fcntl(&mut self) -> Result<()> {
        let fc = *self.func_labels.get("_dev_fcntl").unwrap();
        self.bind_label(fc);
        self.asm.mov(Reg::Rax, 72i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    fn emit_dev_setsockopt(&mut self) -> Result<()> {
        let ss = *self.func_labels.get("_dev_setsockopt").unwrap();
        self.bind_label(ss);
        self.asm.mov(Reg::Rax, 54i32);
        self.asm.syscall();
        self.asm.ret();
        Ok(())
    }

    /// Integer exponentiation: rdi^rsi → rax
    /// Simple loop-based: result = 1; while exp > 0: result *= base; exp--
    fn emit_forge_pow(&mut self) {
        let lab = *self.func_labels.get("__forge_pow").unwrap();
        self.bind_label(lab);
        // rdi = base, rsi = exp
        self.asm.push(Reg::Rbp);
        self.asm.mov(Reg::Rbp, Reg::Rsp);
        self.asm.mov(Reg::Rax, 1i32); // result = 1
        let loop_lab = self.asm.new_label();
        let end_lab = self.asm.new_label();
        self.bind_label(loop_lab);
        self.asm.test(Reg::Rsi, Reg::Rsi);
        self.asm.je(end_lab);
        self.asm.imul(Reg::Rax, Reg::Rdi);
        self.asm.dec(Reg::Rsi);
        self.asm.jmp(loop_lab);
        self.bind_label(end_lab);
        self.asm.pop(Reg::Rbp);
        self.asm.ret();
    }

    fn emit_entry_point(&mut self, start_label: u32) -> Result<()> {
        self.bind_label(start_label);
        // Capture the stack high-water mark before the runtime touches the stack
        // so the conservative GC knows the top of its root range.
        self.emit_gc_stack_top_capture()?;
        let main_lab = *self
            .func_labels
            .get("_forge_main")
            .ok_or_else(|| anyhow::anyhow!("hosted mode requires a main function"))?;
        // The Linux ABI starts the process with argc at [rsp] and the argv
        // array at [rsp+8].  Pass both to main; a `pub def main()` that
        // declares no parameters simply ignores them.
        self.asm.mov(Reg::Rdi, Mem::base(Reg::Rsp));
        self.asm.lea(Reg::Rsi, Mem::base_disp(Reg::Rsp, 8));
        self.asm.call(main_lab);
        self.asm.mov(Reg::Rdi, Reg::Rax);
        let exit_lab = *self.func_labels.get("_dev_exit").unwrap();
        self.asm.call(exit_lab);
        Ok(())
    }
}

use super::*;

impl<'p> CodeGen<'p> {
    pub(super) fn emit_runtime(&mut self, start_label: u32) -> Result<()> {
        let e = *self.func_labels.get("_dev_exit").unwrap();
        self.bind_label(e);
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8));
        self.asm.mov(Reg::Eax, 1i32); // sys_exit
        self.asm.int(0x80);

        let s = *self.func_labels.get("_dev_socket").unwrap();
        self.bind_label(s);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.sub(Reg::Esp, 16i32); // 4-slot args array
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8)); // domain
        self.asm.mov(Mem::base_disp(Reg::Ebp, -16), Reg::Eax);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12)); // type
        self.asm.mov(Mem::base_disp(Reg::Ebp, -12), Reg::Eax);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 16)); // protocol
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax);
        self.asm.mov(Reg::Ebx, 1i32);
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -16));
        self.asm.mov(Reg::Eax, 102i32); // sys_socketcall
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let b = *self.func_labels.get("_dev_bind").unwrap();
        self.bind_label(b);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.sub(Reg::Esp, 16i32);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8)); // fd
        self.asm.mov(Mem::base_disp(Reg::Ebp, -16), Reg::Eax);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12)); // addr
        self.asm.mov(Mem::base_disp(Reg::Ebp, -12), Reg::Eax);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 16)); // addrlen
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax);
        self.asm.mov(Reg::Ebx, 2i32);
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -16));
        self.asm.mov(Reg::Eax, 102i32);
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let li = *self.func_labels.get("_dev_listen").unwrap();
        self.bind_label(li);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.sub(Reg::Esp, 8i32);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8)); // fd
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12)); // backlog
        self.asm.mov(Mem::base_disp(Reg::Ebp, -4), Reg::Eax);
        self.asm.mov(Reg::Ebx, 4i32);
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -8));
        self.asm.mov(Reg::Eax, 102i32);
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let a = *self.func_labels.get("_dev_accept").unwrap();
        self.bind_label(a);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.sub(Reg::Esp, 16i32);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8)); // fd
        self.asm.mov(Mem::base_disp(Reg::Ebp, -16), Reg::Eax);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 12)); // addr
        self.asm.mov(Mem::base_disp(Reg::Ebp, -12), Reg::Eax);
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 16)); // addrlen
        self.asm.mov(Mem::base_disp(Reg::Ebp, -8), Reg::Eax);
        self.asm.mov(Reg::Ebx, 5i32);
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -16));
        self.asm.mov(Reg::Eax, 102i32);
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let re = *self.func_labels.get("_dev_read").unwrap();
        self.bind_label(re);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.mov(Reg::Eax, 3i32);
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8)); // fd
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12)); // buf
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16)); // count
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let w = *self.func_labels.get("_dev_write").unwrap();
        self.bind_label(w);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.mov(Reg::Eax, 4i32);
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8)); // fd
        self.asm.mov(Reg::Ecx, Mem::base_disp(Reg::Ebp, 12)); // buf
        self.asm.mov(Reg::Edx, Mem::base_disp(Reg::Ebp, 16)); // count
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let cl = *self.func_labels.get("_dev_close").unwrap();
        self.bind_label(cl);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.mov(Reg::Eax, 6i32);
        self.asm.mov(Reg::Ebx, Mem::base_disp(Reg::Ebp, 8)); // fd
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let p = *self.func_labels.get("_dev_puts").unwrap();
        self.bind_label(p);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.push(Reg::Esi); // callee-saved
        self.asm.push(Reg::Edi); // callee-saved
        self.asm.mov(Reg::Esi, Mem::base_disp(Reg::Ebp, 8)); // s
        self.asm.mov(Reg::Edi, Reg::Esi); // original pointer
        self.asm.xor(Reg::Ecx, Reg::Ecx); // length
        let loop_lab = self.asm.new_label();
        let done_lab = self.asm.new_label();
        self.bind_label(loop_lab);
        self.asm.movzx8(Reg::Eax, Mem::base(Reg::Esi));
        self.asm.test(Reg::Eax, Reg::Eax);
        self.asm.je(done_lab);
        self.asm.inc(Reg::Esi);
        self.asm.inc(Reg::Ecx);
        self.asm.jmp(loop_lab);
        self.bind_label(done_lab);
        self.asm.mov(Reg::Eax, 4i32); // sys_write
        self.asm.mov(Reg::Ebx, 1i32); // stdout
        self.asm.mov(Reg::Edx, Reg::Ecx); // len
        self.asm.mov(Reg::Ecx, Reg::Edi); // buf
        self.asm.int(0x80);
        self.asm.pop(Reg::Edi);
        self.asm.pop(Reg::Esi);
        self.asm.leave();
        self.asm.ret();

        let pc = *self.func_labels.get("_dev_putchar").unwrap();
        self.bind_label(pc);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.push(Reg::Eax); // allocate 4-byte slot for the character
        self.asm.mov(Reg::Eax, Mem::base_disp(Reg::Ebp, 8)); // c
        self.asm.mov(Mem::base_disp(Reg::Ebp, -4), Reg::Eax); // store low byte (and zeros)
        self.asm.mov(Reg::Eax, 4i32); // sys_write
        self.asm.mov(Reg::Ebx, 1i32); // stdout
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -4)); // buffer address
        self.asm.mov(Reg::Edx, 1i32); // count
        self.asm.int(0x80);
        self.asm.leave();
        self.asm.ret();

        let c = *self.func_labels.get("_dev_getchar").unwrap();
        self.bind_label(c);
        self.asm.push(Reg::Ebp);
        self.asm.mov(Reg::Ebp, Reg::Esp);
        self.asm.sub(Reg::Esp, 4i32); // buffer
        self.asm.mov(Reg::Eax, 3i32); // sys_read
        self.asm.mov(Reg::Ebx, 0i32); // stdin
        self.asm.lea(Reg::Ecx, Mem::base_disp(Reg::Ebp, -4));
        self.asm.mov(Reg::Edx, 1i32);
        self.asm.int(0x80);
        self.asm.cmp(Reg::Eax, 1i32);
        let ok_lab = self.asm.new_label();
        self.asm.je(ok_lab);
        self.asm.mov(Reg::Eax, -1i32);
        self.asm.leave();
        self.asm.ret();
        self.bind_label(ok_lab);
        self.asm.movzx8(Reg::Eax, Mem::base_disp(Reg::Ebp, -4));
        self.asm.leave();
        self.asm.ret();

        let r = *self.func_labels.get("_dev_rand").unwrap();
        self.bind_label(r);
        self.asm.rdtsc(); // edx:eax
        self.asm.ret();

        let lb = *self.func_labels.get("_dev_lfence").unwrap();
        self.bind_label(lb);
        self.asm.lfence();
        self.asm.ret();

        let sb = *self.func_labels.get("_dev_sfence").unwrap();
        self.bind_label(sb);
        self.asm.sfence();
        self.asm.ret();

        let mb = *self.func_labels.get("_dev_mfence").unwrap();
        self.bind_label(mb);
        self.asm.mfence();
        self.asm.ret();

        if let Some(&a) = self.func_labels.get("_dev_alloc") {
            self.bind_label(a);
            self.asm.push(Reg::Ebp);
            self.asm.mov(Reg::Ebp, Reg::Esp);
            let patch_off = self.asm.len() + 2; // C7 /0 imm32, offset of imm32
            self.alloc_ptr_patch = Some(patch_off);
            self.asm.mov(Reg::Eax, 0i32); // eax = &bump_ptr (patched)
            self.asm.mov(Reg::Ecx, Mem::base(Reg::Eax)); // current ptr
            self.asm.mov(Reg::Edx, Reg::Ecx);
            self.asm.add(Reg::Edx, Mem::base_disp(Reg::Ebp, 8)); // new ptr = current + size
            self.asm.mov(Mem::base(Reg::Eax), Reg::Edx); // store new ptr
            self.asm.mov(Reg::Eax, Reg::Ecx); // return previous ptr
            self.asm.leave();
            self.asm.ret();
        }

        if let Some(&f) = self.func_labels.get("_dev_free") {
            self.bind_label(f);
            self.asm.ret();
        }

        self.bind_label(start_label);
        let main_lab = *self
            .func_labels
            .get("_forge_main")
            .ok_or_else(|| anyhow::anyhow!("hosted mode requires a main function"))?;
        self.asm.call(main_lab);
        self.asm.mov(Reg::Ebx, Reg::Eax);
        self.asm.mov(Reg::Eax, 1i32);
        self.asm.int(0x80);

        Ok(())
    }

}

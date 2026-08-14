//! Garbage-collected heap runtime for the x86-64 hosted backend.
//!
//! The compiler emits a small, self-contained memory manager as part of the
//! hosted runtime.  It is activated whenever a program imports any of the
//! `_dev_*` memory-management helpers (`std.alloc` and the new `std.gc`).
//!
//! ## Heap layout
//!
//! The heap is a single contiguous, zero-initialised region in `.bss` of size
//! `HEAP_ARENA_SIZE` (4 MiB).  The first allocation lazily initialises it as one
//! big free block; the free-list head, live-byte counter and statistics all live
//! in the `gc_state` block that the compiler patches into each helper.
//!
//! Allocation uses first-fit free-list traversal with splitting; `free`
//! prepends to the free list.  When the free list cannot satisfy a request,
//! `_dev_alloc` triggers a conservative mark-and-sweep collection and retries
//! once.
//!
//! ## Conservative mark-and-sweep collection
//!
//! Roots are scanned from `[rsp, stack_top]` (the stack) and `.rodata` (module
//! globals + string literals).  Any 8-byte word whose value happens to point
//! inside a live block marks it.  Unmarked live blocks are swept onto the free
//! list.  The compiler always spills local pointers to stack slots, so no live
//! pointer is ever held only in a register across a collection, which keeps the
//! conservative scan sound.
//!
//! ## Block header
//!
//! The 8-byte header sits immediately before the payload returned to the caller.
//! Bit 0 is `USED`, bit 1 is the GC `MARK` flag; the remaining bits hold the
//! 8-byte-aligned payload size.  Free blocks reuse the first 8 bytes of their
//! payload to store the free-list `next` pointer.
//!
//! Register convention (all caller-saved; no callee-saved reg is used, so no
//! save/restore is required): RSI = `&gc_state` anchor, RDI = request/arg, RAX =
//! free-list cursor / `&gc_state`, R8/R9/R10/R11 = hdr/size/ptr/next, RDX/RCX =
//! scratch.

use super::*;
use crate::backend::x64::{Cond, Scale};

/// Minimum payload size for a split remainder to be reusable: an 8-byte header
/// plus at least 8 bytes to hold the `next` pointer.
const MIN_SPLIT_PAYLOAD: i32 = 8;
/// `size - req` must reach this (header + next-pointer payload) to allow splitting.
const SPLIT_THRESHOLD: i32 = MIN_SPLIT_PAYLOAD + H_HDR_SIZE as i32; // 16

impl<'p> CodeGen<'p> {
    /// Emit the full GC runtime.  Only emitted when the program references any
    /// `_dev_*` memory-management helper (`gc_enabled`).
    pub(super) fn emit_gc_runtime(&mut self) -> Result<()> {
        if !self.gc_enabled {
            return Ok(());
        }
        self.emit_dev_alloc_gc()?;
        self.emit_dev_free_gc()?;
        self.emit_gc_collect()?;
        self.emit_gc_leak_check()?;
        self.emit_gc_stat(GC_ALLOC_COUNT, "_dev_gc_alloc_count")?;
        self.emit_gc_stat(GC_FREE_COUNT, "_dev_gc_free_count")?;
        self.emit_gc_stat(GC_GC_COUNT, "_dev_gc_collections")?;
        self.emit_gc_stat(GC_LIVE_BYTES, "_dev_gc_heap_live")?;
        self.emit_gc_capacity()?;
        Ok(())
    }

    /// Emit `movabs dst, <gc_state vaddr>` (patched later).  Returns the code
    /// offset of the 64-bit immediate so it can be patched once `.data` layout
    /// is known.
    fn gc_state_reg(&mut self, dst: Reg) -> Result<usize> {
        let patch = self.asm.len() + 2; // REX.W + opcode -> imm64 at +2
        self.asm.movabs(dst, 0)?;
        self.gc_state_patches.push(patch);
        Ok(patch)
    }

    /// Capture the stack high-water mark (RSP on entry to `_start`) into
    /// `gc_state.stack_top`.  Must run at the very top of `_start`, before the
    /// entry point reads argc/argv from the stack.
    pub(super) fn emit_gc_stack_top_capture(&mut self) -> Result<()> {
        if !self.gc_enabled {
            return Ok(());
        }
        self.gc_state_reg(Reg::Rax)?; // RAX = &gc_state
        self.asm.lea(Reg::R10, Mem::base(Reg::Rsp))?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_STACK_TOP as i32), Reg::R10)?;
        Ok(())
    }

    fn emit_gc_stat(&mut self, field: u64, name: &str) -> Result<()> {
        let lab = *self
            .func_labels
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("missing gc helper label: {}", name))?;
        self.bind_label(lab);
        self.gc_state_reg(Reg::Rax)?;
        self.asm
            .mov(Reg::Rax, Mem::base_disp(Reg::Rax, field as i32))?;
        self.asm.ret()?;
        Ok(())
    }

    fn emit_gc_capacity(&mut self) -> Result<()> {
        let lab = *self.func_labels.get("_dev_gc_heap_capacity").unwrap();
        self.bind_label(lab);
        self.gc_state_reg(Reg::Rax)?; // RAX = &gc_state
        self.asm
            .mov(Reg::Rdx, Mem::base_disp(Reg::Rax, GC_HEAP_BASE as i32))?; // base
        self.asm
            .mov(Reg::Rax, Mem::base_disp(Reg::Rax, GC_HEAP_LIMIT as i32))?; // limit
        self.asm.sub(Reg::Rax, Reg::Rdx)?; // capacity = limit - base
        self.asm.ret()?;
        Ok(())
    }

    /// Shared mark pass.  On entry the caller must have set:
    ///   * RAX = &gc_state, RDI = `heap_base`, RSI = `heap_limit`
    ///   * R8 = `heap_base` (block cursor initialised to the base)
    ///   * a standard `push rbp; mov rbp,rsp; sub rsp, N` frame with spill
    ///     slots [rbp-8]=size, [rbp-16]=payload, [rbp-24]=end available
    ///   * `marks_done` is bound by the caller immediately after this method
    ///     returns; the pass jumps there once every block has been visited.
    fn emit_mark_pass(&mut self, marks_done: u32) -> Result<()> {
        let mark_walk = self.asm.new_label();
        let mark_next = self.asm.new_label();
        let scan_stack = self.asm.new_label();
        let stack_next = self.asm.new_label();
        let rodata_setup = self.asm.new_label();
        let rodata_loop = self.asm.new_label();
        let rodata_next = self.asm.new_label();

        self.bind_label(mark_walk);
        self.asm.cmp(Reg::R8, Reg::Rsi)?;
        self.asm.jcc(Cond::Ae, marks_done)?; // H >= limit -> done
        self.asm.mov(Reg::R9, Mem::base(Reg::R8))?; // hdr
        self.asm.mov(Reg::R10, Reg::R9)?;
        self.asm.and(Reg::R10, !7i32)?; // size
        self.asm.mov(Mem::base_disp(Reg::Rbp, -8), Reg::R10)?; // save size
        self.asm.test(Reg::R9, H_USED as i32)?;
        self.asm.je(mark_next)?; // free block: just advance
        // used block: clear mark
        self.asm.mov(Reg::R11, Reg::R9)?;
        self.asm.and(Reg::R11, !H_MARK as i32)?; // clear only MARK (keep USED + size)
        self.asm.mov(Mem::base(Reg::R8), Reg::R11)?;
        // p = H + 8 ; end = p + size
        self.asm.lea(Reg::R10, Mem::base_disp(Reg::R8, 8))?; // p
        self.asm.mov(Mem::base_disp(Reg::Rbp, -16), Reg::R10)?; // save p
        self.asm.mov(Reg::R10, Mem::base_disp(Reg::Rbp, -8))?; // size
        self.asm.mov(Reg::R11, Mem::base_disp(Reg::Rbp, -16))?; // p
        self.asm.add(Reg::R11, Reg::R10)?; // end = p + size
        self.asm.mov(Mem::base_disp(Reg::Rbp, -24), Reg::R11)?; // save end

        // Scan stack roots.  Scan from RBP (not RSP) so this GC helper's own
        // spill slots ([rbp-8..-24] hold the current block's size/p/end) are
        // excluded — otherwise every block would self-mark via its own `p` slot.
        // The caller's frame lives above [rbp], so all real roots are still hit.
        self.asm.lea(Reg::Rdx, Mem::base(Reg::Rbp))?; // cursor (low)
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rax, GC_STACK_TOP as i32))?; // top
        self.bind_label(scan_stack);
        self.asm.cmp(Reg::Rdx, Reg::Rcx)?;
        self.asm.jcc(Cond::Ae, rodata_setup)?; // stack scan done
        self.asm.mov(Reg::R9, Mem::base(Reg::Rdx))?; // w
        self.asm.mov(Reg::R10, Mem::base_disp(Reg::Rbp, -16))?; // p
        self.asm.cmp(Reg::R10, Reg::R9)?; // p - w
        self.asm.jcc(Cond::A, stack_next)?; // w < p -> skip
        self.asm.mov(Reg::R11, Mem::base_disp(Reg::Rbp, -24))?; // end
        self.asm.cmp(Reg::R9, Reg::R11)?; // w - end
        self.asm.jcc(Cond::Ae, stack_next)?; // w >= end -> skip
        self.asm.mov(Reg::R9, Mem::base(Reg::R8))?;
        self.asm.or(Reg::R9, H_MARK as i32)?;
        self.asm.mov(Mem::base(Reg::R8), Reg::R9)?;
        self.bind_label(stack_next);
        self.asm.add(Reg::Rdx, 8i32)?;
        self.asm.jmp(scan_stack)?;

        // Scan rodata roots [rd_start, rd_end].
        self.bind_label(rodata_setup);
        self.asm
            .mov(Reg::Rdx, Mem::base_disp(Reg::Rax, GC_RD_START as i32))?;
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rax, GC_RD_END as i32))?;
        self.bind_label(rodata_loop);
        self.asm.cmp(Reg::Rdx, Reg::Rcx)?;
        self.asm.jcc(Cond::Ae, mark_next)?; // rodata scan done -> next block
        self.asm.mov(Reg::R9, Mem::base(Reg::Rdx))?; // w
        self.asm.mov(Reg::R10, Mem::base_disp(Reg::Rbp, -16))?; // p
        self.asm.cmp(Reg::R10, Reg::R9)?; // p - w
        self.asm.jcc(Cond::A, rodata_next)?; // w < p -> skip
        self.asm.mov(Reg::R11, Mem::base_disp(Reg::Rbp, -24))?; // end
        self.asm.cmp(Reg::R9, Reg::R11)?; // w - end
        self.asm.jcc(Cond::Ae, rodata_next)?; // w >= end -> skip
        self.asm.mov(Reg::R9, Mem::base(Reg::R8))?;
        self.asm.or(Reg::R9, H_MARK as i32)?;
        self.asm.mov(Mem::base(Reg::R8), Reg::R9)?;
        self.bind_label(rodata_next);
        self.asm.add(Reg::Rdx, 8i32)?;
        self.asm.jmp(rodata_loop)?;

        // Advance to the next block.
        self.bind_label(mark_next);
        self.asm.mov(Reg::R10, Mem::base_disp(Reg::Rbp, -8))?; // size
        self.asm.add(Reg::R10, H_HDR_SIZE as i32)?; // 8 + size
        self.asm.add(Reg::R8, Reg::R10)?; // H += 8 + size
        self.asm.jmp(mark_walk)?;
        // `marks_done` is bound by the caller.
        Ok(())
    }

    // ----------------------------------------------------------------------
    // `_dev_alloc(size)` -> payload pointer  (RDI = size, RAX = result)
    // ----------------------------------------------------------------------
    fn emit_dev_alloc_gc(&mut self) -> Result<()> {
        let lab = *self.func_labels.get("_dev_alloc").unwrap();
        let gc_lab = *self.func_labels.get("_dev_gc_collect").unwrap();
        self.bind_label(lab);

        // [rbp-8] = rounded request size ; [rbp-16] = "tried gc" flag.
        self.asm.push(Reg::Rbp)?;
        self.asm.mov(Reg::Rbp, Reg::Rsp)?;
        self.asm.sub(Reg::Rsp, 16i32)?;
        self.asm.mov(Mem::base_disp(Reg::Rbp, -8), Reg::Rdi)?; // save size

        // ---- lazy heap initialisation -------------------------------------
        self.gc_state_reg(Reg::Rsi)?; // RSI = &gc_state (anchor)
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rsi, GC_HEAP_LIMIT as i32))?;
        let after_init = self.asm.new_label();
        self.asm.test(Reg::Rcx, Reg::Rcx)?;
        self.asm.jne(after_init)?; // already initialised
        // heap_base was pre-patched to `arena_base`; limit = base + HEAP_ARENA_SIZE.
        self.asm
            .mov(Reg::Rdi, Mem::base_disp(Reg::Rsi, GC_HEAP_BASE as i32))?; // base
        self.asm.mov(Reg::Rcx, HEAP_ARENA_SIZE as i32)?;
        self.asm.add(Reg::Rcx, Reg::Rdi)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rsi, GC_HEAP_LIMIT as i32), Reg::Rcx)?; // limit
        // Whole arena = one free block: header at base, size = arena - hdr.
        self.asm
            .mov(Reg::Rcx, (HEAP_ARENA_SIZE as i32) - H_HDR_SIZE as i32)?;
        self.asm.mov(Mem::base(Reg::Rdi), Reg::Rcx)?; // *(base) = size_flag (free)
        self.asm.lea(Reg::Rcx, Mem::base_disp(Reg::Rdi, 8))?;
        self.asm
            .mov(Mem::base_disp(Reg::Rsi, GC_FREE_HEAD as i32), Reg::Rcx)?; // free_head = base+8
        self.bind_label(after_init);

        // ---- round request up to 8 bytes (min 8) -------------------------
        self.asm.mov(Reg::Rdi, Mem::base_disp(Reg::Rbp, -8))?; // reload size
        self.asm.add(Reg::Rdi, 7i32)?;
        self.asm.and(Reg::Rdi, !7i32)?; // 8-byte aligned
        let min_ok = self.asm.new_label();
        self.asm.cmp(Reg::Rdi, 8i32)?;
        self.asm.jcc(Cond::Ae, min_ok)?;
        self.asm.mov(Reg::Rdi, 8i32)?;
        self.bind_label(min_ok);
        self.asm.mov(Mem::base_disp(Reg::Rbp, -8), Reg::Rdi)?; // store req
        self.asm.mov(Mem::base_disp(Reg::Rbp, -16), 0i32)?; // tried_gc = 0

        let walk = self.asm.new_label();
        let walk_next = self.asm.new_label();
        let nofit = self.asm.new_label();
        let fail = self.asm.new_label();
        let alloc_done = self.asm.new_label();
        let retry = self.asm.new_label();
        let split = self.asm.new_label();
        let tw_head = self.asm.new_label();
        let tw_done = self.asm.new_label();
        let sp_head = self.asm.new_label();
        let sp_link = self.asm.new_label();

        // ---- first-fit search over the free list (also the gc retry target) -
        self.bind_label(retry);
        self.gc_state_reg(Reg::Rsi)?; // reload anchor (clobbered by gc_collect)
        self.asm
            .mov(Reg::Rax, Mem::base_disp(Reg::Rsi, GC_FREE_HEAD as i32))?; // cur
        self.asm.mov(Reg::Rdi, Mem::base_disp(Reg::Rbp, -8))?; // req
        self.asm.xor(Reg::R10, Reg::R10)?; // prev = 0

        self.bind_label(walk);
        self.asm.test(Reg::Rax, Reg::Rax)?;
        self.asm.je(nofit)?; // cur == 0 -> exhausted
        self.asm
            .mov(Reg::R8, Mem::base_disp(Reg::Rax, -(H_HDR_SIZE as i32)))?; // hdr
        self.asm.mov(Reg::R9, Reg::R8)?;
        self.asm.and(Reg::R9, !7i32)?; // size
        // `nxt` must be loaded up front: the too-small path (`walk_next`)
        // advances with it, so a load only on the fit path would leave a stale
        // value behind.
        self.asm.mov(Reg::R11, Mem::base(Reg::Rax))?; // nxt = *(cur)
        self.asm.cmp(Reg::R9, Reg::Rdi)?; // size vs req
        self.asm.jcc(Cond::B, walk_next)?; // too small

        // split if (size - req) >= SPLIT_THRESHOLD
        self.asm.mov(Reg::Rcx, Reg::R9)?;
        self.asm.sub(Reg::Rcx, Reg::Rdi)?; // rcx = size - req
        self.asm.cmp(Reg::Rcx, SPLIT_THRESHOLD)?;
        self.asm.jcc(Cond::Ae, split)?;

        // ---- take_whole: unlink cur, used hdr = size|H_USED, return cur ----
        self.asm.cmp(Reg::R10, 0i32)?;
        self.asm.je(tw_head)?;
        self.asm.mov(Mem::base(Reg::R10), Reg::R11)?; // *(prev) = nxt
        self.asm.jmp(tw_done)?;
        self.bind_label(tw_head);
        self.asm
            .mov(Mem::base_disp(Reg::Rsi, GC_FREE_HEAD as i32), Reg::R11)?; // free_head = nxt
        self.bind_label(tw_done);
        self.asm.mov(Reg::R8, Reg::R9)?;
        self.asm.or(Reg::R8, H_USED as i32)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, -(H_HDR_SIZE as i32)), Reg::R8)?;
        self.asm.jmp(alloc_done)?;

        // ---- split: unlink cur, splice in the remainder ----------------------
        self.bind_label(split);
        self.asm.lea(
            Reg::Rcx,
            Mem::base_index_scale_disp(Reg::Rax, Reg::Rdi, Scale::One, 8),
        )?;
        self.asm.mov(Mem::base(Reg::Rcx), Reg::R11)?; // *(rem) = nxt
        self.asm.cmp(Reg::R10, 0i32)?;
        self.asm.je(sp_head)?;
        self.asm.mov(Mem::base(Reg::R10), Reg::Rcx)?; // *(prev) = rem
        self.asm.jmp(sp_link)?;
        self.bind_label(sp_head);
        self.asm
            .mov(Mem::base_disp(Reg::Rsi, GC_FREE_HEAD as i32), Reg::Rcx)?; // free_head = rem
        self.bind_label(sp_link);
        // used header at cur-8 = req | USED
        self.asm.mov(Reg::R8, Reg::Rdi)?;
        self.asm.or(Reg::R8, H_USED as i32)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, -(H_HDR_SIZE as i32)), Reg::R8)?;
        // remainder header at cur+req = (size - req - 8) (free)
        self.asm.mov(Reg::R8, Reg::R9)?;
        self.asm.sub(Reg::R8, Reg::Rdi)?;
        self.asm.sub(Reg::R8, H_HDR_SIZE as i32)?;
        self.asm.mov(
            Mem::base_index_scale(Reg::Rax, Reg::Rdi, Scale::One),
            Reg::R8,
        )?;

        // ---- stats + return ---------------------------------------------
        self.bind_label(alloc_done);
        // RAX = cur (result), RSI = &gc_state, RDI = req.
        self.asm.mov(Reg::Rdx, Mem::base_disp(Reg::Rbp, -8))?; // req
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rsi, GC_LIVE_BYTES as i32))?;
        self.asm.add(Reg::Rcx, Reg::Rdx)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rsi, GC_LIVE_BYTES as i32), Reg::Rcx)?;
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rsi, GC_ALLOC_COUNT as i32))?;
        self.asm.inc(Reg::Rcx)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rsi, GC_ALLOC_COUNT as i32), Reg::Rcx)?;
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rsi, GC_SINCE_GC as i32))?;
        self.asm.add(Reg::Rcx, Reg::Rdx)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rsi, GC_SINCE_GC as i32), Reg::Rcx)?;
        self.asm.leave();
        self.asm.ret()?;

        // ---- free list exhausted: collect once, then retry ----------------
        self.bind_label(nofit);
        self.asm.mov(Reg::Rcx, Mem::base_disp(Reg::Rbp, -16))?;
        self.asm.cmp(Reg::Rcx, 0i32)?;
        self.asm.jne(fail)?;
        self.asm.mov(Mem::base_disp(Reg::Rbp, -16), 1i32)?; // tried_gc = 1
        self.asm.call(gc_lab)?; // clobbers rax..r11; rsi/rdi reloaded at `retry`
        self.asm.jmp(retry)?;

        self.bind_label(fail);
        self.asm.xor(Reg::Rax, Reg::Rax)?; // NULL
        self.asm.leave();
        self.asm.ret()?;

        // ---- advance over a too-small free block --------------------------
        self.bind_label(walk_next);
        self.asm.mov(Reg::R10, Reg::Rax)?; // prev = cur
        self.asm.mov(Reg::Rax, Reg::R11)?; // cur = nxt
        self.asm.jmp(walk)?;

        Ok(())
    }

    // ----------------------------------------------------------------------
    fn emit_dev_free_gc(&mut self) -> Result<()> {
        let lab = *self.func_labels.get("_dev_free").unwrap();
        self.bind_label(lab);
        self.asm.push(Reg::Rbp)?;
        self.asm.mov(Reg::Rbp, Reg::Rsp)?;
        self.asm.test(Reg::Rdi, Reg::Rdi)?;
        let ret = self.asm.new_label();
        self.asm.je(ret)?;
        self.gc_state_reg(Reg::Rax)?; // RAX = &gc_state (RDI = p preserved)
        self.asm
            .mov(Reg::R8, Mem::base_disp(Reg::Rdi, -(H_HDR_SIZE as i32)))?; // hdr
        self.asm.mov(Reg::R9, Reg::R8)?;
        self.asm.and(Reg::R9, !7i32)?; // size
        // mark free: clear all flag bits -> hdr = size
        self.asm
            .mov(Mem::base_disp(Reg::Rdi, -(H_HDR_SIZE as i32)), Reg::R9)?;
        // prepend: next = free_head; *(p) = next; free_head = p
        self.asm
            .mov(Reg::R11, Mem::base_disp(Reg::Rax, GC_FREE_HEAD as i32))?;
        self.asm.mov(Mem::base(Reg::Rdi), Reg::R11)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_FREE_HEAD as i32), Reg::Rdi)?;
        // free_count++
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rax, GC_FREE_COUNT as i32))?;
        self.asm.inc(Reg::Rcx)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_FREE_COUNT as i32), Reg::Rcx)?;
        // live_bytes -= size
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rax, GC_LIVE_BYTES as i32))?;
        self.asm.sub(Reg::Rcx, Reg::R9)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_LIVE_BYTES as i32), Reg::Rcx)?;
        self.bind_label(ret);
        self.asm.leave();
        self.asm.ret()?;
        Ok(())
    }

    // ----------------------------------------------------------------------
    // `_dev_gc_collect()` -> void.  Conservative mark-and-sweep.
    // ----------------------------------------------------------------------
    fn emit_gc_collect(&mut self) -> Result<()> {
        let lab = *self.func_labels.get("_dev_gc_collect").unwrap();
        self.bind_label(lab);
        // Frame with spill slots: [rbp-8]=size, [rbp-16]=payload, [rbp-24]=end.
        self.asm.push(Reg::Rbp)?;
        self.asm.mov(Reg::Rbp, Reg::Rsp)?;
        self.asm.sub(Reg::Rsp, 32i32)?;

        self.gc_state_reg(Reg::Rax)?; // RAX = &gc_state
        self.asm
            .mov(Reg::Rdi, Mem::base_disp(Reg::Rax, GC_HEAP_BASE as i32))?; // base
        self.asm
            .mov(Reg::Rsi, Mem::base_disp(Reg::Rax, GC_HEAP_LIMIT as i32))?; // limit
        let sweep_begin = self.asm.new_label();
        self.asm.test(Reg::Rdi, Reg::Rdi)?;
        self.asm.je(sweep_begin)?; // heap uninitialised
        self.asm.cmp(Reg::Rdi, Reg::Rsi)?;
        self.asm.jcc(Cond::Ae, sweep_begin)?; // base >= limit

        self.asm.mov(Reg::R8, Reg::Rdi)?; // r8 = H = base
        let marks_done = self.asm.new_label();
        self.emit_mark_pass(marks_done)?;
        self.bind_label(marks_done);

        // ---- SWEEP ---------------------------------------------------------
        self.bind_label(sweep_begin);
        self.asm.mov(Reg::R8, Reg::Rdi)?; // H = base
        let sweep_walk = self.asm.new_label();
        let sweep_keep = self.asm.new_label();
        let sweep_next = self.asm.new_label();
        let gc_done = self.asm.new_label();
        self.bind_label(sweep_walk);
        self.asm.cmp(Reg::R8, Reg::Rsi)?;
        self.asm.jcc(Cond::Ae, gc_done)?;
        self.asm.mov(Reg::R9, Mem::base(Reg::R8))?; // hdr
        self.asm.mov(Reg::R10, Reg::R9)?;
        self.asm.and(Reg::R10, !7i32)?; // size
        self.asm.test(Reg::R9, H_USED as i32)?;
        self.asm.je(sweep_next)?; // free block: leave + advance
        self.asm.test(Reg::R9, H_MARK as i32)?;
        self.asm.jne(sweep_keep)?; // marked used -> keep
        // unmarked used -> sweep to free
        self.asm.mov(Mem::base(Reg::R8), Reg::R10)?; // *(H) = size (free)
        self.asm.lea(Reg::Rdx, Mem::base_disp(Reg::R8, 8))?; // payload = H+8
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rax, GC_FREE_HEAD as i32))?;
        self.asm.mov(Mem::base(Reg::Rdx), Reg::Rcx)?; // *(payload) = nxt
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_FREE_HEAD as i32), Reg::Rdx)?; // free_head = payload
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rax, GC_LIVE_BYTES as i32))?;
        self.asm.sub(Reg::Rcx, Reg::R10)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_LIVE_BYTES as i32), Reg::Rcx)?;
        self.asm.jmp(sweep_next)?;
        self.bind_label(sweep_keep);
        self.asm.mov(Reg::R11, Reg::R9)?;
        self.asm.and(Reg::R11, !H_MARK as i32)?; // clear only MARK (keep USED)
        self.asm.mov(Mem::base(Reg::R8), Reg::R11)?;
        self.bind_label(sweep_next);
        self.asm.add(Reg::R8, Reg::R10)?; // H += size
        self.asm.add(Reg::R8, H_HDR_SIZE as i32)?; // H += 8
        self.asm.jmp(sweep_walk)?;

        self.bind_label(gc_done);
        self.asm
            .mov(Reg::Rcx, Mem::base_disp(Reg::Rax, GC_GC_COUNT as i32))?;
        self.asm.inc(Reg::Rcx)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_GC_COUNT as i32), Reg::Rcx)?;
        self.asm
            .mov(Mem::base_disp(Reg::Rax, GC_SINCE_GC as i32), 0i32)?;
        self.asm.leave();
        self.asm.ret()?;
        Ok(())
    }

    // ----------------------------------------------------------------------
    // `_dev_gc_leak_check()` -> uint64.  Marks live blocks, then counts (and
    // reports) the bytes in *used* blocks that were NOT reachable — i.e. true
    // leaks — without reclaiming them.  Marks are cleared afterwards.  Returns
    // 0 when the heap is uninitialised.
    // ----------------------------------------------------------------------
    fn emit_gc_leak_check(&mut self) -> Result<()> {
        let lab = *self.func_labels.get("_dev_gc_leak_check").unwrap();
        self.bind_label(lab);
        self.asm.push(Reg::Rbp)?;
        self.asm.mov(Reg::Rbp, Reg::Rsp)?;
        self.asm.sub(Reg::Rsp, 32i32)?; // spill slots: [rbp-8]=size,[rbp-16]=p,[rbp-24]=end,[rbp-32]=leak bytes

        self.gc_state_reg(Reg::Rax)?;
        self.asm
            .mov(Reg::Rdi, Mem::base_disp(Reg::Rax, GC_HEAP_BASE as i32))?;
        self.asm
            .mov(Reg::Rsi, Mem::base_disp(Reg::Rax, GC_HEAP_LIMIT as i32))?;
        let leak_ret0 = self.asm.new_label();
        self.asm.test(Reg::Rdi, Reg::Rdi)?;
        self.asm.je(leak_ret0)?;
        self.asm.cmp(Reg::Rdi, Reg::Rsi)?;
        self.asm.jcc(Cond::Ae, leak_ret0)?;

        self.asm.mov(Reg::R8, Reg::Rdi)?; // H = base
        let marks_done = self.asm.new_label();
        self.emit_mark_pass(marks_done)?;
        self.bind_label(marks_done);

        // Count walk: tally sizes of unreachable used blocks, then clear marks.
        self.asm.mov(Reg::R8, Reg::Rdi)?; // H = base
        self.asm.mov(Mem::base_disp(Reg::Rbp, -32), 0i32)?; // leak bytes = 0
        let count_walk = self.asm.new_label();
        let count_next = self.asm.new_label();
        let count_live = self.asm.new_label();
        let count_done = self.asm.new_label();
        self.bind_label(count_walk);
        self.asm.cmp(Reg::R8, Reg::Rsi)?;
        self.asm.jcc(Cond::Ae, count_done)?;
        self.asm.mov(Reg::R9, Mem::base(Reg::R8))?; // hdr
        self.asm.mov(Reg::R10, Reg::R9)?;
        self.asm.and(Reg::R10, !7i32)?; // size
        self.asm.test(Reg::R9, H_USED as i32)?;
        self.asm.je(count_next)?; // free block: advance
        self.asm.test(Reg::R9, H_MARK as i32)?;
        self.asm.jne(count_live)?; // marked (live) -> just clear mark
        // unreachable used block: tally it
        self.asm.mov(Reg::R11, Mem::base_disp(Reg::Rbp, -32))?;
        self.asm.add(Reg::R11, Reg::R10)?;
        self.asm.mov(Mem::base_disp(Reg::Rbp, -32), Reg::R11)?;
        // fall through to clear mark
        self.bind_label(count_live);
        self.asm.mov(Reg::R11, Reg::R9)?;
        self.asm.and(Reg::R11, !H_MARK as i32)?; // clear only MARK (keep USED)
        self.asm.mov(Mem::base(Reg::R8), Reg::R11)?;
        self.bind_label(count_next);
        self.asm.add(Reg::R8, Reg::R10)?; // H += size
        self.asm.add(Reg::R8, H_HDR_SIZE as i32)?; // H += 8
        self.asm.jmp(count_walk)?;
        self.bind_label(count_done);

        self.asm.mov(Reg::Rax, Mem::base_disp(Reg::Rbp, -32))?; // return leak bytes
        self.asm.leave();
        self.asm.ret()?;

        self.bind_label(leak_ret0);
        self.asm.xor(Reg::Rax, Reg::Rax)?;
        self.asm.leave();
        self.asm.ret()?;
        Ok(())
    }
}

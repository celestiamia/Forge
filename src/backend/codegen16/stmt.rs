use super::*;

impl<'p> CodeGen16<'p> {
    pub(super) fn emit_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Let { name, init, .. } => {
                let slot = *self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("unknown local: {}", name))?;
                if let Some(e) = init {
                    self.eval_expr(e)?;
                    self.store_slot(slot, Reg16::Ax, Reg8::Al)?;
                }
            }
            Stmt::StackAlloc { name, .. } => {
                let raw_off = *self
                    .arrays
                    .get(name)
                    .ok_or_else(|| anyhow!("missing array base: {}", name))?;
                let slot = *self
                    .locals
                    .get(name)
                    .ok_or_else(|| anyhow!("missing array pointer slot: {}", name))?;
                self.asm.lea_bp(Reg16::Ax, raw_off);
                self.store_slot(slot, Reg16::Ax, Reg8::Al)?;
            }
            Stmt::Assign { lhs, rhs } => {
                self.lvalue_addr(lhs)?;
                self.asm.push(Reg16::Ax);
                self.eval_expr(rhs)?;
                self.asm.pop(Reg16::Si);
                let (width, _) = self.lvalue_width(lhs)?;
                match width {
                    1 => self.asm.store8_si(Reg8::Al),
                    2 => self.asm.store16_si(Reg16::Ax),
                    _ => bail!("unsupported store width: {}", width),
                }
            }
            Stmt::Return(None) => self.asm.jmp_short_lab(self.ret_label),
            Stmt::Return(Some(e)) => {
                self.eval_expr(e)?;
                self.asm.jmp_short_lab(self.ret_label);
            }
            Stmt::Expr(e) => {
                self.eval_expr(e)?;
            }
            Stmt::If { cond, then, else_ } => self.emit_if(cond, then, else_.as_deref())?,
            Stmt::While { cond, body } => self.emit_while(cond, body)?,
            Stmt::For { init, cond, step, body } => self.emit_for(init.as_deref(), cond, step.as_ref(), body)?,
            Stmt::Break => {
                let end = *self.loop_end_stack.last()
                    .ok_or_else(|| anyhow!("break outside of loop"))?;
                self.asm.jmp_short_lab(end);
            }
            Stmt::Continue => {
                let head = *self.loop_head_stack.last()
                    .ok_or_else(|| anyhow!("continue outside of loop"))?;
                self.asm.jmp_short_lab(head);
            }
            Stmt::Unsafe(body) => {
                for s in body {
                    self.emit_stmt(s)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn emit_if(&mut self, cond: &Expr, then: &[Stmt], else_: Option<&[Stmt]>) -> Result<()> {
        let end_lab = self.asm.new_label();
        self.eval_expr(cond)?;
        self.asm.test_ax_ax();
        if let Some(else_body) = else_ {
            let else_lab = self.asm.new_label();
            self.asm.je_short_lab(else_lab);
            for s in then {
                self.emit_stmt(s)?;
            }
            self.asm.jmp_short_lab(end_lab);
            self.asm.bind(else_lab);
            for s in else_body {
                self.emit_stmt(s)?;
            }
        } else {
            self.asm.je_short_lab(end_lab);
            for s in then {
                self.emit_stmt(s)?;
            }
        }
        self.asm.bind(end_lab);
        Ok(())
    }

    pub(super) fn emit_while(&mut self, cond: &Expr, body: &[Stmt]) -> Result<()> {
        let head = self.asm.new_label();
        let end = self.asm.new_label();
        self.loop_head_stack.push(head);
        self.loop_end_stack.push(end);
        self.asm.bind(head);
        self.eval_expr(cond)?;
        self.asm.test_ax_ax();
        self.asm.je_short_lab(end);
        for s in body {
            self.emit_stmt(s)?;
        }
        self.asm.jmp_short_lab(head);
        self.asm.bind(end);
        self.loop_head_stack.pop();
        self.loop_end_stack.pop();
        Ok(())
    }

    pub(super) fn emit_for(
        &mut self,
        init: Option<&Stmt>,
        cond: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
    ) -> Result<()> {
        let head = self.asm.new_label();
        let end = self.asm.new_label();
        if let Some(i) = init {
            self.emit_stmt(i)?;
        }
        self.loop_head_stack.push(head);
        self.loop_end_stack.push(end);
        self.asm.bind(head);
        self.eval_expr(cond)?;
        self.asm.test_ax_ax();
        self.asm.je_short_lab(end);
        for s in body {
            self.emit_stmt(s)?;
        }
        if let Some(st) = step {
            self.eval_expr(st)?;
        }
        self.asm.jmp_short_lab(head);
        self.asm.bind(end);
        self.loop_head_stack.pop();
        self.loop_end_stack.pop();
        Ok(())
    }

}

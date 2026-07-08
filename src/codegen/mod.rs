pub mod runtime;

use crate::ast::*;
use crate::sema::SemaResult;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};
use runtime::RuntimeFns;
use std::collections::HashMap;

/// A local-variable scope: name -> alloca'd stack slot. A stack of these
/// mirrors sema's `SymbolTable` scope stack and interp's `Environment` chain
/// exactly, so variable visibility can't diverge between the three passes.
type Scopes<'ctx> = Vec<HashMap<String, PointerValue<'ctx>>>;

pub struct Codegen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    runtime: RuntimeFns<'ctx>,
    functions: HashMap<String, FunctionValue<'ctx>>,
    sema: &'ctx SemaResult,
}

impl<'ctx> Codegen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str, sema: &'ctx SemaResult) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        let runtime = RuntimeFns::declare(context, &module);
        Codegen {
            context,
            module,
            builder,
            runtime,
            functions: HashMap::new(),
            sema,
        }
    }

    pub fn compile(&mut self, program: &Program) {
        for decl in &program.decls {
            if let Decl::Func(f) = decl {
                self.declare_function(f);
            }
        }
        for decl in &program.decls {
            if let Decl::Func(f) = decl {
                self.compile_function(f);
            }
        }
    }

    pub fn verify(&self) -> Result<(), String> {
        self.module.verify().map_err(|e| e.to_string())
    }

    pub fn print_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }

    /// Exposes the underlying module — Phase 6 needs this to hand off to
    /// `TargetMachine` for object-file emission.
    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    // ---- types ----

    fn llvm_basic_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().into(),
            Type::Float => self.context.f64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            Type::Str => self.context.ptr_type(AddressSpace::default()).into(),
            Type::Array(_) => self.array_struct_type().into(),
            Type::Void => unreachable!("void is not a value-carrying type"),
        }
    }

    /// The `{ i64 length, ptr data }` layout every array uses regardless of
    /// element type — opaque pointers mean `data`'s LLVM type never encodes
    /// the element type, so this struct shape is the same for `int[]`,
    /// `float[]`, etc. The *logical* element type only matters when
    /// generating a GEP into `data`, which is why those call sites consult
    /// `self.sema.types` instead.
    fn array_struct_type(&self) -> inkwell::types::StructType<'ctx> {
        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        self.context
            .struct_type(&[self.context.i64_type().into(), ptr_ty.into()], false)
    }

    fn resolved_type(&self, id: NodeId) -> &Type {
        self.sema
            .types
            .get(&id)
            .expect("sema guarantees every expression has a resolved type")
    }

    // ---- functions ----

    fn declare_function(&mut self, f: &FuncDecl) {
        let param_types: Vec<_> = f
            .params
            .iter()
            .map(|p| self.llvm_basic_type(&p.ty).into())
            .collect();
        let fn_type = if f.return_ty == Type::Void {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.llvm_basic_type(&f.return_ty).fn_type(&param_types, false)
        };
        let function = self.module.add_function(&f.name, fn_type, None);
        self.functions.insert(f.name.clone(), function);
    }

    fn compile_function(&mut self, f: &FuncDecl) {
        let function = self.functions[&f.name];
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        let mut scopes: Scopes<'ctx> = vec![HashMap::new()];
        for (i, param) in f.params.iter().enumerate() {
            let llvm_ty = self.llvm_basic_type(&param.ty);
            let alloca = self.create_entry_alloca(function, &param.name, llvm_ty);
            let param_value = function.get_nth_param(i as u32).unwrap();
            self.builder.build_store(alloca, param_value).unwrap();
            scopes.last_mut().unwrap().insert(param.name.clone(), alloca);
        }

        let terminated = self.codegen_block(&f.body.stmts, function, &mut scopes);
        if !terminated {
            if f.return_ty == Type::Void {
                self.builder.build_return(None).unwrap();
            } else {
                // Unreachable per sema's MissingReturn check: every accepted
                // non-void function has a statement that unconditionally
                // returns on every path.
                self.builder.build_unreachable().unwrap();
            }
        }
    }

    /// Allocas must live in the entry block (not wherever control flow
    /// happens to be) so a loop body never allocates a fresh stack slot on
    /// every iteration — standard LLVM codegen practice.
    fn create_entry_alloca(
        &self,
        function: FunctionValue<'ctx>,
        name: &str,
        ty: BasicTypeEnum<'ctx>,
    ) -> PointerValue<'ctx> {
        let entry = function.get_first_basic_block().unwrap();
        let temp_builder = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(first_instr) => temp_builder.position_before(&first_instr),
            None => temp_builder.position_at_end(entry),
        }
        temp_builder.build_alloca(ty, name).unwrap()
    }

    fn resolve_var(&self, scopes: &Scopes<'ctx>, name: &str) -> PointerValue<'ctx> {
        scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .copied()
            .expect("sema guarantees the identifier is declared")
    }

    // ---- statements ----

    /// Returns `true` if control flow can no longer fall out the bottom of
    /// the current block (it already ended in `ret`/`unreachable`) — the
    /// caller must not append anything further to it. Mirrors sema's
    /// `check_stmt` return-completeness tracking and interp's `Flow`.
    fn codegen_block(
        &mut self,
        stmts: &[Stmt],
        function: FunctionValue<'ctx>,
        scopes: &mut Scopes<'ctx>,
    ) -> bool {
        for stmt in stmts {
            if self.codegen_stmt(stmt, function, scopes) {
                return true; // rest of the block is unreachable dead code
            }
        }
        false
    }

    fn codegen_scoped(
        &mut self,
        stmt: &Stmt,
        function: FunctionValue<'ctx>,
        scopes: &mut Scopes<'ctx>,
    ) -> bool {
        if matches!(stmt, Stmt::Block(_)) {
            self.codegen_stmt(stmt, function, scopes)
        } else {
            scopes.push(HashMap::new());
            let terminated = self.codegen_stmt(stmt, function, scopes);
            scopes.pop();
            terminated
        }
    }

    fn codegen_stmt(
        &mut self,
        stmt: &Stmt,
        function: FunctionValue<'ctx>,
        scopes: &mut Scopes<'ctx>,
    ) -> bool {
        match stmt {
            Stmt::Expr(e) => {
                self.codegen_expr(e, scopes);
                false
            }
            Stmt::VarDecl(vd) => {
                let llvm_ty = self.llvm_basic_type(&vd.ty);
                let alloca = self.create_entry_alloca(function, &vd.name, llvm_ty);
                let init_val = match &vd.init {
                    Some(e) => self
                        .codegen_expr(e, scopes)
                        .expect("sema guarantees a non-void initializer"),
                    None => self.default_value(&vd.ty),
                };
                self.builder.build_store(alloca, init_val).unwrap();
                scopes.last_mut().unwrap().insert(vd.name.clone(), alloca);
                false
            }
            Stmt::Block(b) => {
                scopes.push(HashMap::new());
                let terminated = self.codegen_block(&b.stmts, function, scopes);
                scopes.pop();
                terminated
            }
            Stmt::If(if_stmt) => self.codegen_if(if_stmt, function, scopes),
            Stmt::While(w) => self.codegen_while(w, function, scopes),
            Stmt::For(f) => self.codegen_for(f, function, scopes),
            Stmt::Return(r) => {
                match &r.value {
                    Some(e) => {
                        let val = self
                            .codegen_expr(e, scopes)
                            .expect("sema guarantees a non-void return value");
                        self.builder.build_return(Some(&val)).unwrap();
                    }
                    None => {
                        self.builder.build_return(None).unwrap();
                    }
                }
                true
            }
        }
    }

    fn codegen_if(
        &mut self,
        if_stmt: &IfStmt,
        function: FunctionValue<'ctx>,
        scopes: &mut Scopes<'ctx>,
    ) -> bool {
        let cond = self
            .codegen_expr(&if_stmt.cond, scopes)
            .unwrap()
            .into_int_value();
        let then_bb = self.context.append_basic_block(function, "then");
        let else_bb = self.context.append_basic_block(function, "else");
        let merge_bb = self.context.append_basic_block(function, "ifcont");

        self.builder
            .build_conditional_branch(cond, then_bb, else_bb)
            .unwrap();

        self.builder.position_at_end(then_bb);
        let then_terminated = self.codegen_scoped(&if_stmt.then_branch, function, scopes);
        if !then_terminated {
            self.builder.build_unconditional_branch(merge_bb).unwrap();
        }

        self.builder.position_at_end(else_bb);
        let else_terminated = match &if_stmt.else_branch {
            Some(else_stmt) => self.codegen_scoped(else_stmt, function, scopes),
            None => false,
        };
        if !else_terminated {
            self.builder.build_unconditional_branch(merge_bb).unwrap();
        }

        self.builder.position_at_end(merge_bb);
        if then_terminated && else_terminated {
            // merge_bb has no predecessors; give it a terminator so the
            // module still verifies, and tell the caller this whole
            // if/else counts as terminated (matches sema's `then && else`).
            self.builder.build_unreachable().unwrap();
            true
        } else {
            false
        }
    }

    fn codegen_while(
        &mut self,
        w: &WhileStmt,
        function: FunctionValue<'ctx>,
        scopes: &mut Scopes<'ctx>,
    ) -> bool {
        let cond_bb = self.context.append_basic_block(function, "whilecond");
        let body_bb = self.context.append_basic_block(function, "whilebody");
        let after_bb = self.context.append_basic_block(function, "whileafter");

        self.builder.build_unconditional_branch(cond_bb).unwrap();

        self.builder.position_at_end(cond_bb);
        let cond = self.codegen_expr(&w.cond, scopes).unwrap().into_int_value();
        self.builder
            .build_conditional_branch(cond, body_bb, after_bb)
            .unwrap();

        self.builder.position_at_end(body_bb);
        let body_terminated = self.codegen_scoped(&w.body, function, scopes);
        if !body_terminated {
            self.builder.build_unconditional_branch(cond_bb).unwrap();
        }

        self.builder.position_at_end(after_bb);
        false
    }

    fn codegen_for(
        &mut self,
        f: &ForStmt,
        function: FunctionValue<'ctx>,
        scopes: &mut Scopes<'ctx>,
    ) -> bool {
        scopes.push(HashMap::new());
        self.codegen_stmt(&f.init, function, scopes);

        let cond_bb = self.context.append_basic_block(function, "forcond");
        let body_bb = self.context.append_basic_block(function, "forbody");
        let after_bb = self.context.append_basic_block(function, "forafter");

        self.builder.build_unconditional_branch(cond_bb).unwrap();

        self.builder.position_at_end(cond_bb);
        let cond = self.codegen_expr(&f.cond, scopes).unwrap().into_int_value();
        self.builder
            .build_conditional_branch(cond, body_bb, after_bb)
            .unwrap();

        self.builder.position_at_end(body_bb);
        let body_terminated = self.codegen_scoped(&f.body, function, scopes);
        if !body_terminated {
            self.codegen_expr(&f.update, scopes);
            self.builder.build_unconditional_branch(cond_bb).unwrap();
        }

        self.builder.position_at_end(after_bb);
        scopes.pop();
        false
    }

    fn default_value(&self, ty: &Type) -> BasicValueEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().const_int(0, false).into(),
            Type::Float => self.context.f64_type().const_float(0.0).into(),
            Type::Bool => self.context.bool_type().const_int(0, false).into(),
            Type::Str => self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
            Type::Array(_) => {
                let struct_ty = self.array_struct_type();
                let zero_len = self.context.i64_type().const_int(0, false);
                let null_data = self.context.ptr_type(AddressSpace::default()).const_null();
                struct_ty
                    .const_named_struct(&[zero_len.into(), null_data.into()])
                    .into()
            }
            Type::Void => unreachable!("void is not a value-carrying type"),
        }
    }

    // ---- expressions ----

    /// `None` represents a void-typed result (a `print(...)` call, or a call
    /// to a `void` function) — sema guarantees such a result is never used
    /// anywhere except as a bare expression statement.
    fn codegen_expr(&mut self, expr: &Expr, scopes: &mut Scopes<'ctx>) -> Option<BasicValueEnum<'ctx>> {
        match expr {
            Expr::IntLiteral { value, .. } => {
                Some(self.context.i64_type().const_int(*value as u64, true).into())
            }
            Expr::FloatLiteral { value, .. } => {
                Some(self.context.f64_type().const_float(*value).into())
            }
            Expr::BoolLiteral { value, .. } => Some(
                self.context
                    .bool_type()
                    .const_int(*value as u64, false)
                    .into(),
            ),
            Expr::StringLiteral { value, .. } => Some(
                self.builder
                    .build_global_string_ptr(value, "strlit")
                    .unwrap()
                    .as_pointer_value()
                    .into(),
            ),
            Expr::Null { .. } => {
                Some(self.context.ptr_type(AddressSpace::default()).const_null().into())
            }
            Expr::Ident { name, .. } => {
                let ptr = self.resolve_var(scopes, name);
                let ty = self.llvm_basic_type(self.resolved_type(expr.id()));
                Some(self.builder.build_load(ty, ptr, name).unwrap())
            }
            Expr::Binary { op, left, right, .. } => self.codegen_binary(*op, left, right, scopes),
            Expr::Unary { op, operand, .. } => self.codegen_unary(*op, operand, scopes),
            Expr::Assign { target, value, .. } => self.codegen_assign(target, value, scopes),
            Expr::Call { callee, args, .. } => self.codegen_call(callee, args, scopes),
            Expr::Index { array, index, .. } => {
                let ptr = self.codegen_index_ptr(array, index, scopes);
                let elem_ty = self.array_elem_llvm_type(array);
                Some(self.builder.build_load(elem_ty, ptr, "idxload").unwrap())
            }
            Expr::FieldAccess { object, field, .. } => {
                self.codegen_field_access(object, field, scopes)
            }
            Expr::ArrayLiteral { elements, .. } => {
                Some(self.codegen_array_literal(elements, self.resolved_type(expr.id()).clone(), scopes))
            }
            Expr::ArrayCreation { elem_ty, size, .. } => {
                Some(self.codegen_array_creation(elem_ty, size, scopes))
            }
        }
    }

    fn codegen_binary(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        // && and || short-circuit: the right operand must only be evaluated
        // (and its side effects only happen) when actually needed, so this
        // needs real control flow, not just an LLVM instruction.
        if matches!(op, BinOp::And | BinOp::Or) {
            return Some(self.codegen_short_circuit(op, left, right, scopes).into());
        }

        let l = self.codegen_expr(left, scopes).unwrap();
        let r = self.codegen_expr(right, scopes).unwrap();

        let result: BasicValueEnum = if l.is_int_value() {
            let (l, r) = (l.into_int_value(), r.into_int_value());
            match op {
                BinOp::Add => self.builder.build_int_add(l, r, "addtmp").unwrap().into(),
                BinOp::Sub => self.builder.build_int_sub(l, r, "subtmp").unwrap().into(),
                BinOp::Mul => self.builder.build_int_mul(l, r, "multmp").unwrap().into(),
                BinOp::Div => self
                    .builder
                    .build_int_signed_div(l, r, "divtmp")
                    .unwrap()
                    .into(),
                BinOp::Mod => self
                    .builder
                    .build_int_signed_rem(l, r, "modtmp")
                    .unwrap()
                    .into(),
                BinOp::Lt => self
                    .builder
                    .build_int_compare(IntPredicate::SLT, l, r, "lttmp")
                    .unwrap()
                    .into(),
                BinOp::LtEq => self
                    .builder
                    .build_int_compare(IntPredicate::SLE, l, r, "letmp")
                    .unwrap()
                    .into(),
                BinOp::Gt => self
                    .builder
                    .build_int_compare(IntPredicate::SGT, l, r, "gttmp")
                    .unwrap()
                    .into(),
                BinOp::GtEq => self
                    .builder
                    .build_int_compare(IntPredicate::SGE, l, r, "getmp")
                    .unwrap()
                    .into(),
                BinOp::Eq => self
                    .builder
                    .build_int_compare(IntPredicate::EQ, l, r, "eqtmp")
                    .unwrap()
                    .into(),
                BinOp::NotEq => self
                    .builder
                    .build_int_compare(IntPredicate::NE, l, r, "netmp")
                    .unwrap()
                    .into(),
                BinOp::And | BinOp::Or => unreachable!("handled above"),
            }
        } else {
            let (l, r) = (l.into_float_value(), r.into_float_value());
            match op {
                BinOp::Add => self.builder.build_float_add(l, r, "addtmp").unwrap().into(),
                BinOp::Sub => self.builder.build_float_sub(l, r, "subtmp").unwrap().into(),
                BinOp::Mul => self.builder.build_float_mul(l, r, "multmp").unwrap().into(),
                BinOp::Div => self.builder.build_float_div(l, r, "divtmp").unwrap().into(),
                BinOp::Mod => self.builder.build_float_rem(l, r, "modtmp").unwrap().into(),
                BinOp::Lt => self
                    .builder
                    .build_float_compare(inkwell::FloatPredicate::OLT, l, r, "lttmp")
                    .unwrap()
                    .into(),
                BinOp::LtEq => self
                    .builder
                    .build_float_compare(inkwell::FloatPredicate::OLE, l, r, "letmp")
                    .unwrap()
                    .into(),
                BinOp::Gt => self
                    .builder
                    .build_float_compare(inkwell::FloatPredicate::OGT, l, r, "gttmp")
                    .unwrap()
                    .into(),
                BinOp::GtEq => self
                    .builder
                    .build_float_compare(inkwell::FloatPredicate::OGE, l, r, "getmp")
                    .unwrap()
                    .into(),
                BinOp::Eq => self
                    .builder
                    .build_float_compare(inkwell::FloatPredicate::OEQ, l, r, "eqtmp")
                    .unwrap()
                    .into(),
                BinOp::NotEq => self
                    .builder
                    .build_float_compare(inkwell::FloatPredicate::ONE, l, r, "netmp")
                    .unwrap()
                    .into(),
                BinOp::And | BinOp::Or => unreachable!("handled above"),
            }
        };
        Some(result)
    }

    fn codegen_short_circuit(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> IntValue<'ctx> {
        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();
        let rhs_bb = self.context.append_basic_block(function, "scrhs");
        let merge_bb = self.context.append_basic_block(function, "scmerge");

        let l = self
            .codegen_expr(left, scopes)
            .unwrap()
            .into_int_value();
        let start_bb = self.builder.get_insert_block().unwrap();
        match op {
            BinOp::And => {
                self.builder
                    .build_conditional_branch(l, rhs_bb, merge_bb)
                    .unwrap();
            }
            BinOp::Or => {
                self.builder
                    .build_conditional_branch(l, merge_bb, rhs_bb)
                    .unwrap();
            }
            _ => unreachable!(),
        }

        self.builder.position_at_end(rhs_bb);
        let r = self
            .codegen_expr(right, scopes)
            .unwrap()
            .into_int_value();
        let rhs_end_bb = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb).unwrap();

        self.builder.position_at_end(merge_bb);
        let phi = self.builder.build_phi(self.context.bool_type(), "sctmp").unwrap();
        phi.add_incoming(&[(&l, start_bb), (&r, rhs_end_bb)]);
        phi.as_basic_value().into_int_value()
    }

    fn codegen_unary(
        &mut self,
        op: UnOp,
        operand: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        let v = self.codegen_expr(operand, scopes).unwrap();
        let result: BasicValueEnum = match op {
            UnOp::Neg => {
                if v.is_int_value() {
                    self.builder
                        .build_int_neg(v.into_int_value(), "negtmp")
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_neg(v.into_float_value(), "negtmp")
                        .unwrap()
                        .into()
                }
            }
            UnOp::Not => self
                .builder
                .build_not(v.into_int_value(), "nottmp")
                .unwrap()
                .into(),
        };
        Some(result)
    }

    fn codegen_assign(
        &mut self,
        target: &Expr,
        value: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        let val = self
            .codegen_expr(value, scopes)
            .expect("sema guarantees a non-void assignment value");
        match target {
            Expr::Ident { name, .. } => {
                let ptr = self.resolve_var(scopes, name);
                self.builder.build_store(ptr, val).unwrap();
            }
            Expr::Index { array, index, .. } => {
                let ptr = self.codegen_index_ptr(array, index, scopes);
                self.builder.build_store(ptr, val).unwrap();
            }
            _ => unreachable!("sema guarantees only Ident/Index are valid assignment targets"),
        }
        Some(val)
    }

    fn codegen_call(
        &mut self,
        callee: &str,
        args: &[Expr],
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        if callee == "print" {
            self.codegen_print(&args[0], scopes);
            return None;
        }
        let function = self.functions[callee];
        let arg_values: Vec<_> = args
            .iter()
            .map(|a| {
                self.codegen_expr(a, scopes)
                    .expect("sema guarantees non-void call arguments")
                    .into()
            })
            .collect();
        let call_site = self.builder.build_call(function, &arg_values, "calltmp").unwrap();
        call_site.try_as_basic_value().basic()
    }

    fn codegen_print(&mut self, arg: &Expr, scopes: &mut Scopes<'ctx>) {
        let val = self
            .codegen_expr(arg, scopes)
            .expect("sema guarantees print's argument is non-void");
        match val {
            BasicValueEnum::IntValue(iv) => {
                if iv.get_type().get_bit_width() == 1 {
                    let widened = self
                        .builder
                        .build_int_z_extend(iv, self.context.i8_type(), "booltoi8")
                        .unwrap();
                    self.builder
                        .build_call(self.runtime.print_bool, &[widened.into()], "")
                        .unwrap();
                } else {
                    self.builder
                        .build_call(self.runtime.print_int, &[iv.into()], "")
                        .unwrap();
                }
            }
            BasicValueEnum::FloatValue(fv) => {
                self.builder
                    .build_call(self.runtime.print_float, &[fv.into()], "")
                    .unwrap();
            }
            BasicValueEnum::PointerValue(pv) => {
                // Strings are the only pointer-typed print argument any P0
                // benchmark actually reaches; a bare array isn't printed
                // directly by any of them (only individual elements are).
                self.builder
                    .build_call(self.runtime.print_str, &[pv.into()], "")
                    .unwrap();
            }
            _ => unreachable!("sema guarantees a printable primitive type"),
        }
    }

    /// The element type an `Index`/assign-to-index expression's *array*
    /// operand carries, per sema's side table — needed because opaque
    /// pointers don't encode it in the LLVM value itself.
    fn array_elem_llvm_type(&self, array_expr: &Expr) -> BasicTypeEnum<'ctx> {
        match self.resolved_type(array_expr.id()) {
            Type::Array(elem) => self.llvm_basic_type(elem),
            _ => unreachable!("sema guarantees an array type here"),
        }
    }

    /// Computes the element pointer for `array[index]`, for both reads and
    /// index-assignment.
    fn codegen_index_ptr(
        &mut self,
        array: &Expr,
        index: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> PointerValue<'ctx> {
        let arr_val = self.codegen_expr(array, scopes).unwrap().into_struct_value();
        let data_ptr = self
            .builder
            .build_extract_value(arr_val, 1, "dataptr")
            .unwrap()
            .into_pointer_value();
        let idx = self.codegen_expr(index, scopes).unwrap().into_int_value();
        let elem_ty = self.array_elem_llvm_type(array);
        unsafe {
            self.builder
                .build_gep(elem_ty, data_ptr, &[idx], "elemptr")
                .unwrap()
        }
    }

    fn codegen_field_access(
        &mut self,
        object: &Expr,
        field: &str,
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        debug_assert_eq!(field, "length", "sema guarantees only array.length");
        let arr_val = self.codegen_expr(object, scopes).unwrap().into_struct_value();
        Some(
            self.builder
                .build_extract_value(arr_val, 0, "arrlen")
                .unwrap(),
        )
    }

    fn codegen_array_literal(
        &mut self,
        elements: &[Expr],
        arr_ty: Type,
        scopes: &mut Scopes<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let elem_ty = match arr_ty {
            Type::Array(e) => *e,
            _ => unreachable!("sema guarantees an array type for an array literal"),
        };
        let elem_llvm_ty = self.llvm_basic_type(&elem_ty);
        let n = elements.len() as u64;

        let elem_values: Vec<_> = elements
            .iter()
            .map(|e| {
                self.codegen_expr(e, scopes)
                    .expect("sema guarantees non-void array elements")
            })
            .collect();

        let elem_size = self.size_of_basic_type(elem_llvm_ty);
        let byte_len = self
            .context
            .i64_type()
            .const_int(n * elem_size, false);
        let data_ptr = self
            .builder
            .build_call(self.runtime.malloc, &[byte_len.into()], "arrdata")
            .unwrap()
            .try_as_basic_value()
            .basic()
            .unwrap()
            .into_pointer_value();

        for (i, val) in elem_values.into_iter().enumerate() {
            let idx = self.context.i64_type().const_int(i as u64, false);
            let elem_ptr = unsafe {
                self.builder
                    .build_gep(elem_llvm_ty, data_ptr, &[idx], "initelem")
                    .unwrap()
            };
            self.builder.build_store(elem_ptr, val).unwrap();
        }

        let n_val = self.context.i64_type().const_int(n, false);
        self.build_array_struct(n_val, data_ptr)
    }

    fn codegen_array_creation(
        &mut self,
        elem_ty: &Type,
        size: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let elem_llvm_ty = self.llvm_basic_type(elem_ty);
        let elem_size = self.context.i64_type().const_int(self.size_of_basic_type(elem_llvm_ty), false);
        let n = self.codegen_expr(size, scopes).unwrap().into_int_value();

        // calloc zero-initializes, so `new int[n]` never needs a manual
        // zero-fill loop over a runtime-determined length.
        let data_ptr = self
            .builder
            .build_call(self.runtime.calloc, &[n.into(), elem_size.into()], "arrdata")
            .unwrap()
            .try_as_basic_value()
            .basic()
            .unwrap()
            .into_pointer_value();

        self.build_array_struct(n, data_ptr)
    }

    fn build_array_struct(
        &self,
        length_val: IntValue<'ctx>,
        data_ptr: PointerValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let struct_ty = self.array_struct_type();
        let undef = struct_ty.get_undef();
        let with_len = self
            .builder
            .build_insert_value(undef, length_val, 0, "arr.len")
            .unwrap();
        let with_data = self
            .builder
            .build_insert_value(with_len, data_ptr, 1, "arr.data")
            .unwrap();
        with_data.as_basic_value_enum()
    }

    fn size_of_basic_type(&self, ty: BasicTypeEnum<'ctx>) -> u64 {
        match ty {
            BasicTypeEnum::IntType(i) => (i.get_bit_width() as u64).div_ceil(8),
            BasicTypeEnum::FloatType(_) => 8,
            BasicTypeEnum::PointerType(_) => 8,
            _ => unreachable!("no other basic type appears as an array element in P0"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;
    use crate::sema;

    fn build(source: &str) -> (Program, SemaResult) {
        let tokens = lex(source).expect("should lex");
        let program = parse(tokens).expect("should parse");
        let sema_result = sema::analyze(&program).expect("should type-check");
        (program, sema_result)
    }

    fn assert_verifies(source: &str) {
        let (program, sema_result) = build(source);
        let context = Context::create();
        let mut codegen = Codegen::new(&context, "test", &sema_result);
        codegen.compile(&program);
        if let Err(e) = codegen.verify() {
            panic!("module failed to verify: {e}\n\nIR:\n{}", codegen.print_ir());
        }
    }

    #[test]
    fn compiles_empty_main() {
        assert_verifies("void main() { }");
    }

    /// JIT-executes a compiled function directly (bypassing `main`/`print`,
    /// which call into the not-yet-linked runtime shim) and returns its
    /// result. `Module::verify()` only checks IR is well-formed — this
    /// checks the generated logic actually computes the right answer, which
    /// is a meaningfully stronger bar than Phase 5's literal exit gate.
    /// Restricted to scalar-in/scalar-out functions: an array *parameter*
    /// is passed as a by-value LLVM struct whose C ABI lowering isn't safe
    /// to hand-match from a raw Rust function pointer; arrays used only as
    /// *internal* locals (no such parameter) are fine and still exercise
    /// malloc/calloc + GEP + load/store.
    fn jit_i64_1(source: &str, func_name: &str, arg: i64) -> i64 {
        let (program, sema_result) = build(source);
        let context = Context::create();
        let mut codegen = Codegen::new(&context, "jit_test", &sema_result);
        codegen.compile(&program);
        codegen.verify().expect("module should verify before JIT");
        let ee = codegen
            .module()
            .create_jit_execution_engine(inkwell::OptimizationLevel::None)
            .expect("should create a JIT execution engine");
        unsafe {
            let f: inkwell::execution_engine::JitFunction<unsafe extern "C" fn(i64) -> i64> =
                ee.get_function(func_name).expect("function should be found");
            f.call(arg)
        }
    }

    fn jit_i64_2(source: &str, func_name: &str, a: i64, b: i64) -> i64 {
        let (program, sema_result) = build(source);
        let context = Context::create();
        let mut codegen = Codegen::new(&context, "jit_test", &sema_result);
        codegen.compile(&program);
        codegen.verify().expect("module should verify before JIT");
        let ee = codegen
            .module()
            .create_jit_execution_engine(inkwell::OptimizationLevel::None)
            .expect("should create a JIT execution engine");
        unsafe {
            let f: inkwell::execution_engine::JitFunction<unsafe extern "C" fn(i64, i64) -> i64> =
                ee.get_function(func_name).expect("function should be found");
            f.call(a, b)
        }
    }

    fn jit_i64_0(source: &str, func_name: &str) -> i64 {
        let (program, sema_result) = build(source);
        let context = Context::create();
        let mut codegen = Codegen::new(&context, "jit_test", &sema_result);
        codegen.compile(&program);
        codegen.verify().expect("module should verify before JIT");
        let ee = codegen
            .module()
            .create_jit_execution_engine(inkwell::OptimizationLevel::None)
            .expect("should create a JIT execution engine");
        unsafe {
            let f: inkwell::execution_engine::JitFunction<unsafe extern "C" fn() -> i64> =
                ee.get_function(func_name).expect("function should be found");
            f.call()
        }
    }

    #[test]
    fn jit_factorial_is_correct() {
        let source = r#"
            int factorial(int n) {
                if (n <= 1) return 1;
                return n * factorial(n - 1);
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_1(source, "factorial", 5), 120);
    }

    #[test]
    fn jit_fibonacci_naive_is_correct() {
        let source = r#"
            int fibNaive(int n) {
                if (n <= 1) return n;
                return fibNaive(n - 1) + fibNaive(n - 2);
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_1(source, "fibNaive", 10), 55);
    }

    #[test]
    fn jit_gcd_is_correct() {
        let source = r#"
            int gcd(int a, int b) {
                if (b == 0) return a;
                return gcd(b, a % b);
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_2(source, "gcd", 48, 18), 6);
    }

    #[test]
    fn jit_fast_exponentiation_is_correct() {
        let source = r#"
            int power(int base, int exp) {
                if (exp == 0) return 1;
                int half = power(base, exp / 2);
                int halfSquared = half * half;
                if (exp % 2 == 0) return halfSquared;
                return halfSquared * base;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_2(source, "power", 2, 10), 1024);
    }

    #[test]
    fn jit_climbing_stairs_is_correct() {
        // Uses `new int[n + 1]` and indexed reads/writes as an internal
        // local (no array parameter) — exercises calloc + GEP + load/store
        // logic under real execution, not just IR shape.
        let source = r#"
            int climbStairs(int n) {
                if (n <= 2) return n;
                int[] dp = new int[n + 1];
                dp[1] = 1;
                dp[2] = 2;
                for (int i = 3; i <= n; i = i + 1) {
                    dp[i] = dp[i - 1] + dp[i - 2];
                }
                return dp[n];
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_1(source, "climbStairs", 5), 8);
    }

    #[test]
    fn jit_iterative_loop_and_array_literal_is_correct() {
        // Array literal (not creation) as an internal local, plus a while
        // loop and index reads — max-subarray's Kadane's algorithm.
        let source = r#"
            int maxSubArray() {
                int[] nums = [-2, 1, -3, 4, -1, 2, 1, -5, 4];
                int best = nums[0];
                int current = nums[0];
                int i = 1;
                while (i < nums.length) {
                    int candidate = current + nums[i];
                    if (nums[i] > candidate) {
                        current = nums[i];
                    } else {
                        current = candidate;
                    }
                    if (current > best) {
                        best = current;
                    }
                    i = i + 1;
                }
                return best;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_0(source, "maxSubArray"), 6);
    }

    #[test]
    fn compiles_variables_and_arithmetic() {
        assert_verifies(
            r#"
            void main() {
                int x = 5;
                int y = x + 3 * 2 - 1;
                float f = 3.14;
                bool b = true;
                x = x + 1;
            }
            "#,
        );
    }

    #[test]
    fn compiles_all_comparison_and_logical_operators() {
        assert_verifies(
            r#"
            void main() {
                int a = 1;
                int b = 2;
                bool r1 = a < b;
                bool r2 = a <= b;
                bool r3 = a > b;
                bool r4 = a >= b;
                bool r5 = a == b;
                bool r6 = a != b;
                bool r7 = r1 && r2;
                bool r8 = r1 || r2;
                bool r9 = !r1;
                int neg = -a;
            }
            "#,
        );
    }

    #[test]
    fn compiles_if_else_if_else() {
        assert_verifies(
            r#"
            void main() {
                int x = 0;
                if (x > 0) {
                    print(1);
                } else if (x == 0) {
                    print(0);
                } else {
                    print(-1);
                }
            }
            "#,
        );
    }

    #[test]
    fn compiles_if_with_return_in_both_branches() {
        // Both branches terminate — the merge block has no predecessors and
        // must still get a terminator (`unreachable`) for the module to verify.
        assert_verifies(
            r#"
            int classify(int x) {
                if (x > 0) {
                    return 1;
                } else {
                    return -1;
                }
            }
            void main() { print(classify(5)); }
            "#,
        );
    }

    #[test]
    fn compiles_while_loop() {
        assert_verifies(
            r#"
            void main() {
                int x = 3;
                while (x > 0) {
                    print(x);
                    x = x - 1;
                }
            }
            "#,
        );
    }

    #[test]
    fn compiles_for_loop() {
        assert_verifies(
            r#"
            void main() {
                int[] nums = [1, 2, 3];
                for (int i = 0; i < nums.length; i = i + 1) {
                    print(nums[i]);
                }
            }
            "#,
        );
    }

    #[test]
    fn compiles_functions_and_recursion() {
        assert_verifies(
            r#"
            int fib(int n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }
            void main() { print(fib(10)); }
            "#,
        );
    }

    #[test]
    fn compiles_array_literal_index_assign_and_length() {
        assert_verifies(
            r#"
            void main() {
                int[] arr = [1, 2, 3];
                arr[0] = arr[1] + arr[2];
                print(arr[0]);
                print(arr.length);
            }
            "#,
        );
    }

    #[test]
    fn compiles_array_creation_with_runtime_size() {
        assert_verifies(
            r#"
            void main() {
                int n = 5;
                int[] scratch = new int[n];
                scratch[0] = 1;
            }
            "#,
        );
    }

    #[test]
    fn compiles_arrays_passed_by_value_to_functions() {
        assert_verifies(
            r#"
            void increment(int[] arr) {
                arr[0] = arr[0] + 1;
            }
            void main() {
                int[] nums = [5];
                increment(nums);
                print(nums[0]);
            }
            "#,
        );
    }

    #[test]
    fn compiles_print_of_every_primitive_kind() {
        assert_verifies(
            r#"
            void main() {
                print(42);
                print(3.14);
                print(true);
                print("hello");
            }
            "#,
        );
    }

    #[test]
    fn compiles_short_circuit_and_or() {
        assert_verifies(
            r#"
            bool sideEffect() { return true; }
            void main() {
                if (false && sideEffect()) { }
                if (true || sideEffect()) { }
            }
            "#,
        );
    }

    #[test]
    fn compiles_nested_for_loops() {
        assert_verifies(
            r#"
            void main() {
                int[] arr = [5, 2, 4, 1, 3];
                for (int i = 0; i < arr.length - 1; i = i + 1) {
                    for (int j = 0; j < arr.length - 1 - i; j = j + 1) {
                        if (arr[j] > arr[j + 1]) {
                            int tmp = arr[j];
                            arr[j] = arr[j + 1];
                            arr[j + 1] = tmp;
                        }
                    }
                }
            }
            "#,
        );
    }

    /// Every P0 benchmark program (docs/ANX-Implementation-Plan-v1.md Phase 7)
    /// must emit verifiable LLVM IR — Phase 5's exit gate. Running the
    /// resulting binaries is Phase 6/7's job, not this one's.
    macro_rules! benchmark_verifies {
        ($test_name:ident, $file:literal) => {
            #[test]
            fn $test_name() {
                let source = include_str!(concat!("../../tests/benchmarks/", $file));
                assert_verifies(source);
            }
        };
    }

    benchmark_verifies!(benchmark_01_binary_search, "01_binary_search.nx");
    benchmark_verifies!(
        benchmark_02_binary_search_first_last,
        "02_binary_search_first_last.nx"
    );
    benchmark_verifies!(benchmark_03_two_sum_sorted, "03_two_sum_sorted.nx");
    benchmark_verifies!(benchmark_04_reverse_array, "04_reverse_array.nx");
    benchmark_verifies!(benchmark_05_remove_duplicates, "05_remove_duplicates.nx");
    benchmark_verifies!(benchmark_06_bubble_sort, "06_bubble_sort.nx");
    benchmark_verifies!(benchmark_07_insertion_sort, "07_insertion_sort.nx");
    benchmark_verifies!(benchmark_08_selection_sort, "08_selection_sort.nx");
    benchmark_verifies!(benchmark_09_merge_sort, "09_merge_sort.nx");
    benchmark_verifies!(benchmark_10_quicksort, "10_quicksort.nx");
    benchmark_verifies!(benchmark_11_factorial, "11_factorial.nx");
    benchmark_verifies!(benchmark_12_fibonacci_naive, "12_fibonacci_naive.nx");
    benchmark_verifies!(benchmark_13_fibonacci_memo, "13_fibonacci_memo.nx");
    benchmark_verifies!(
        benchmark_14_fast_exponentiation,
        "14_fast_exponentiation.nx"
    );
    benchmark_verifies!(benchmark_15_gcd, "15_gcd.nx");
    benchmark_verifies!(benchmark_16_climbing_stairs, "16_climbing_stairs.nx");
    benchmark_verifies!(benchmark_17_coin_change, "17_coin_change.nx");
    benchmark_verifies!(benchmark_18_knapsack, "18_knapsack.nx");
    benchmark_verifies!(
        benchmark_19_longest_increasing_subsequence,
        "19_longest_increasing_subsequence.nx"
    );
    benchmark_verifies!(benchmark_20_max_subarray, "20_max_subarray.nx");
}

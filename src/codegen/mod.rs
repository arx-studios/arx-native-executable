pub mod runtime;

use crate::ast::*;
use crate::sema::SemaResult;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};
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
        self.emit_c_main_wrapper();
    }

    /// ANX's `main` is `void`, but a native binary's entry point must be a
    /// C-ABI `int main()` whose return value defines the process exit code.
    /// The ANX function is emitted as `anx_main` (see `declare_function`);
    /// this wrapper calls it and returns 0.
    fn emit_c_main_wrapper(&mut self) {
        let i32_ty = self.context.i32_type();
        let main_fn = self
            .module
            .add_function("main", i32_ty.fn_type(&[], false), None);
        let entry = self.context.append_basic_block(main_fn, "entry");
        self.builder.position_at_end(entry);
        self.builder
            .build_call(self.functions["main"], &[], "")
            .unwrap();
        self.builder
            .build_return(Some(&i32_ty.const_int(0, false)))
            .unwrap();
    }

    pub fn verify(&self) -> Result<(), String> {
        self.module.verify().map_err(|e| e.to_string())
    }

    /// Debugging helper — dumps textual LLVM IR, used by test failure
    /// messages. Only referenced from `#[cfg(test)]` code, so a non-test
    /// build sees it as unused; kept outside that cfg gate since it's a
    /// generally useful method on the type, not test-only logic.
    #[allow(dead_code)]
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
            Type::Str | Type::Array(_) => self.len_data_struct_type().into(),
            Type::Void => unreachable!("void is not a value-carrying type"),
        }
    }

    /// The `{ i64 length, ptr data }` layout shared by arrays *and* strings
    /// (P2 — mirrors the array struct exactly, per
    /// docs/P2/ANX-P2-Strings-Plan-v1.md). Opaque pointers mean `data`'s
    /// LLVM type never encodes the array element type, so this struct shape
    /// is the same for `int[]`, `float[]`, `string`, etc. The *logical*
    /// element type only matters when generating a GEP into an array's
    /// `data`, which is why those call sites consult `self.sema.types`
    /// instead.
    fn len_data_struct_type(&self) -> inkwell::types::StructType<'ctx> {
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
        // ANX `main` gets the LLVM symbol `anx_main` so the C-ABI `main`
        // wrapper (see `emit_c_main_wrapper`) can own the real entry point.
        // The `functions` map still keys by the ANX-source name, so calls
        // resolve unchanged.
        let symbol = if f.name == "main" { "anx_main" } else { &f.name };
        let function = self.module.add_function(symbol, fn_type, None);
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

    /// Emits a runtime check: continue in a fresh block when `ok` holds,
    /// otherwise call the given runtime panic function (which never
    /// returns — it prints to stderr and exits 2, matching the
    /// interpreter's RuntimeError behavior per docs/ANX-Usage-Flow-v1.md).
    fn emit_runtime_guard(
        &mut self,
        ok: IntValue<'ctx>,
        panic_fn: FunctionValue<'ctx>,
        panic_args: &[BasicMetadataValueEnum<'ctx>],
        name: &str,
    ) {
        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();
        let ok_bb = self.context.append_basic_block(function, &format!("{name}.ok"));
        let panic_bb = self
            .context
            .append_basic_block(function, &format!("{name}.panic"));
        self.builder
            .build_conditional_branch(ok, ok_bb, panic_bb)
            .unwrap();
        self.builder.position_at_end(panic_bb);
        self.builder.build_call(panic_fn, panic_args, "").unwrap();
        self.builder.build_unreachable().unwrap();
        self.builder.position_at_end(ok_bb);
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
            Type::Str | Type::Array(_) => {
                let struct_ty = self.len_data_struct_type();
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
            Expr::StringLiteral { value, .. } => {
                // A null-terminated global byte buffer wrapped in the same
                // `{ i64 length, ptr data }` struct arrays use — the
                // terminator lets `anx_print_str` keep treating `data` as a
                // plain C string with no separate lowering path.
                let data_ptr = self
                    .builder
                    .build_global_string_ptr(value, "strlit")
                    .unwrap()
                    .as_pointer_value();
                let len_val = self.context.i64_type().const_int(value.len() as u64, false);
                Some(self.build_len_data_struct(len_val, data_ptr))
            }
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
            Expr::CompoundAssign { op, target, value, .. } => {
                self.codegen_compound_assign(*op, target, value, scopes)
            }
            Expr::IncDec { op, target, is_prefix, .. } => {
                self.codegen_inc_dec(*op, target, *is_prefix, scopes)
            }
            Expr::Ternary { cond, then_branch, else_branch, .. } => {
                self.codegen_ternary(expr.id(), cond, then_branch, else_branch, scopes)
            }
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

        let result: BasicValueEnum = match l {
            BasicValueEnum::IntValue(li) => {
                self.apply_int_binop(op, li, r.into_int_value()).into()
            }
            BasicValueEnum::FloatValue(lf) => {
                self.apply_float_binop(op, lf, r.into_float_value())
            }
            // Only `Str` (never `Array`) reaches a binary op per sema's
            // `combine_binary_types` — `+`/`==`/`!=` are the only ops it
            // allows on a struct-valued operand.
            BasicValueEnum::StructValue(ls) => self.apply_str_binop(op, ls, r.into_struct_value()),
            _ => unreachable!("sema guarantees int/float/string operands for binary ops"),
        };
        Some(result)
    }

    /// The string-operand op logic (P2) — same role as `apply_int_binop`/
    /// `apply_float_binop` above, for the one struct-valued case sema allows
    /// through a binary op (`Str`).
    fn apply_str_binop(
        &mut self,
        op: BinOp,
        l: inkwell::values::StructValue<'ctx>,
        r: inkwell::values::StructValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        match op {
            BinOp::Add => self.codegen_str_concat(l, r),
            BinOp::Eq => self.codegen_str_equals(l, r).into(),
            BinOp::NotEq => {
                let eq = self.codegen_str_equals(l, r);
                self.builder.build_not(eq, "strnetmp").unwrap().into()
            }
            _ => unreachable!("sema guarantees only +, ==, != for string operands"),
        }
    }

    fn codegen_str_concat(
        &mut self,
        l: inkwell::values::StructValue<'ctx>,
        r: inkwell::values::StructValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        self.builder
            .build_call(self.runtime.str_concat, &[l.into(), r.into()], "strcat")
            .unwrap()
            .try_as_basic_value()
            .basic()
            .unwrap()
    }

    /// Returns i1 (ANX's native `bool` type) — the C shim's i8 return is
    /// truncated here, at the one call site, matching the i8-boundary
    /// convention `anx_print_bool` already established for booleans crossing
    /// this FFI edge.
    fn codegen_str_equals(
        &mut self,
        l: inkwell::values::StructValue<'ctx>,
        r: inkwell::values::StructValue<'ctx>,
    ) -> IntValue<'ctx> {
        let call = self
            .builder
            .build_call(self.runtime.str_equals, &[l.into(), r.into()], "streq")
            .unwrap();
        let i8_result = call.try_as_basic_value().basic().unwrap().into_int_value();
        self.builder
            .build_int_truncate(i8_result, self.context.bool_type(), "streqbool")
            .unwrap()
    }

    /// The int-operand op logic, shared by `codegen_binary` (both operands
    /// freshly codegen'd) and `codegen_compound_assign` (target's loaded
    /// current value + the RHS) — mirrors the interpreter's
    /// `apply_binary_op` split for the same reason (see its doc comment).
    fn apply_int_binop(&mut self, op: BinOp, l: IntValue<'ctx>, r: IntValue<'ctx>) -> IntValue<'ctx> {
        match op {
            BinOp::Add => self.builder.build_int_add(l, r, "addtmp").unwrap(),
            BinOp::Sub => self.builder.build_int_sub(l, r, "subtmp").unwrap(),
            BinOp::Mul => self.builder.build_int_mul(l, r, "multmp").unwrap(),
            BinOp::Div => {
                self.emit_div_zero_guard(r);
                self.builder.build_int_signed_div(l, r, "divtmp").unwrap()
            }
            BinOp::Mod => {
                self.emit_div_zero_guard(r);
                self.builder.build_int_signed_rem(l, r, "modtmp").unwrap()
            }
            BinOp::BitAnd => self.builder.build_and(l, r, "andtmp").unwrap(),
            BinOp::BitOr => self.builder.build_or(l, r, "ortmp").unwrap(),
            BinOp::BitXor => self.builder.build_xor(l, r, "xortmp").unwrap(),
            BinOp::Shl => self.builder.build_left_shift(l, r, "shltmp").unwrap(),
            // `true` = arithmetic (sign-extending) shift — the Operators
            // Plan's decision, since ANX's only integer type is signed.
            BinOp::Shr => self.builder.build_right_shift(l, r, true, "shrtmp").unwrap(),
            // `false` = logical (zero-filling) rather than arithmetic —
            // the LLVM builder call already supported this mode, just
            // wasn't exposed as a second ANX operator until now.
            BinOp::UShr => self.builder.build_right_shift(l, r, false, "ushrtmp").unwrap(),
            BinOp::Lt => self
                .builder
                .build_int_compare(IntPredicate::SLT, l, r, "lttmp")
                .unwrap(),
            BinOp::LtEq => self
                .builder
                .build_int_compare(IntPredicate::SLE, l, r, "letmp")
                .unwrap(),
            BinOp::Gt => self
                .builder
                .build_int_compare(IntPredicate::SGT, l, r, "gttmp")
                .unwrap(),
            BinOp::GtEq => self
                .builder
                .build_int_compare(IntPredicate::SGE, l, r, "getmp")
                .unwrap(),
            BinOp::Eq => self
                .builder
                .build_int_compare(IntPredicate::EQ, l, r, "eqtmp")
                .unwrap(),
            BinOp::NotEq => self
                .builder
                .build_int_compare(IntPredicate::NE, l, r, "netmp")
                .unwrap(),
            BinOp::And | BinOp::Or => unreachable!("handled by codegen_short_circuit"),
        }
    }

    fn apply_float_binop(
        &mut self,
        op: BinOp,
        l: inkwell::values::FloatValue<'ctx>,
        r: inkwell::values::FloatValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
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
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr | BinOp::UShr => {
                unreachable!("sema guarantees bitwise/shift operands are int, never float")
            }
            BinOp::And | BinOp::Or => unreachable!("handled by codegen_short_circuit"),
        }
    }

    /// Int `/` and `%` by zero are UB at the LLVM level (and don't even trap
    /// on AArch64 — hardware division by zero silently yields 0 there), so
    /// an explicit check is the only way a compiled binary can match the
    /// interpreter's division-by-zero error.
    fn emit_div_zero_guard(&mut self, divisor: IntValue<'ctx>) {
        let zero = self.context.i64_type().const_int(0, false);
        let nonzero = self
            .builder
            .build_int_compare(IntPredicate::NE, divisor, zero, "nonzero")
            .unwrap();
        self.emit_runtime_guard(nonzero, self.runtime.panic_div_zero, &[], "div0");
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

    /// `cond ? then : else` as an expression — same basic-block-plus-`phi`
    /// shape as `codegen_short_circuit` above, generalized to any result
    /// type instead of always `bool`. Only the taken branch's side effects
    /// happen, same as the interpreter's short-circuiting evaluation.
    /// `Type::Void` can't reach here — sema's `check_ternary` rejects it,
    /// since there's no such thing as a void-typed `phi`.
    fn codegen_ternary(
        &mut self,
        result_id: NodeId,
        cond: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        let function = self
            .builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap();
        let then_bb = self.context.append_basic_block(function, "terntrue");
        let else_bb = self.context.append_basic_block(function, "ternfalse");
        let merge_bb = self.context.append_basic_block(function, "ternmerge");

        let cond_val = self.codegen_expr(cond, scopes).unwrap().into_int_value();
        self.builder
            .build_conditional_branch(cond_val, then_bb, else_bb)
            .unwrap();

        self.builder.position_at_end(then_bb);
        let then_val = self.codegen_expr(then_branch, scopes).unwrap();
        let then_end_bb = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb).unwrap();

        self.builder.position_at_end(else_bb);
        let else_val = self.codegen_expr(else_branch, scopes).unwrap();
        let else_end_bb = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb).unwrap();

        self.builder.position_at_end(merge_bb);
        let result_ty = self.resolved_type(result_id).clone();
        let phi = self
            .builder
            .build_phi(self.llvm_basic_type(&result_ty), "ternresult")
            .unwrap();
        // BasicValueEnum doesn't itself implement the BasicValue trait
        // add_incoming needs — dispatch to the concrete variant per the
        // ternary's resolved type (matches the `.into_*_value()` pattern
        // used everywhere else in this file).
        match result_ty {
            Type::Int | Type::Bool => {
                let t = then_val.into_int_value();
                let e = else_val.into_int_value();
                phi.add_incoming(&[(&t, then_end_bb), (&e, else_end_bb)]);
            }
            Type::Float => {
                let t = then_val.into_float_value();
                let e = else_val.into_float_value();
                phi.add_incoming(&[(&t, then_end_bb), (&e, else_end_bb)]);
            }
            Type::Str | Type::Array(_) => {
                let t = then_val.into_struct_value();
                let e = else_val.into_struct_value();
                phi.add_incoming(&[(&t, then_end_bb), (&e, else_end_bb)]);
            }
            Type::Void => unreachable!("sema rejects void-typed ternary branches"),
        }
        Some(phi.as_basic_value())
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
            // Same LLVM `not` instruction as logical NOT — bitwise complement
            // on an i64 is exactly XOR-with-all-ones at the IR level.
            UnOp::BitNot => self
                .builder
                .build_not(v.into_int_value(), "bitnottmp")
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

    /// Computes the target's pointer exactly once (a `resolve_var` lookup
    /// or a single `codegen_index_ptr` GEP), loads through it, applies `op`,
    /// stores back through the same pointer. Mirrors the interpreter's
    /// `eval_compound_assign` for the same double-evaluation reason.
    fn codegen_compound_assign(
        &mut self,
        op: BinOp,
        target: &Expr,
        value: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        let (ptr, elem_llvm_ty) = match target {
            Expr::Ident { name, .. } => {
                let ptr = self.resolve_var(scopes, name);
                let ty = self.llvm_basic_type(self.resolved_type(target.id()));
                (ptr, ty)
            }
            Expr::Index { array, index, .. } => {
                let ptr = self.codegen_index_ptr(array, index, scopes);
                let ty = self.array_elem_llvm_type(array);
                (ptr, ty)
            }
            _ => unreachable!("sema guarantees only Ident/Index are valid assignment targets"),
        };
        let current = self.builder.build_load(elem_llvm_ty, ptr, "curval").unwrap();
        let rhs = self
            .codegen_expr(value, scopes)
            .expect("sema guarantees a non-void compound-assignment value");
        let new_val: BasicValueEnum = match current {
            BasicValueEnum::IntValue(civ) => self
                .apply_int_binop(op, civ, rhs.into_int_value())
                .into(),
            BasicValueEnum::FloatValue(cfv) => {
                self.apply_float_binop(op, cfv, rhs.into_float_value())
            }
            // Only `s += ...` (string concat) reaches a struct-valued
            // compound assignment — sema's `combine_binary_types` allows no
            // other op on `Str`, and never allows `Array` here at all.
            BasicValueEnum::StructValue(csv) => {
                self.apply_str_binop(op, csv, rhs.into_struct_value())
            }
            _ => unreachable!("sema guarantees int/float/string operands for compound assignment"),
        };
        self.builder.build_store(ptr, new_val).unwrap();
        Some(new_val)
    }

    /// Same "resolve the target's pointer exactly once" shape as
    /// `codegen_compound_assign` above. Prefix returns the new value;
    /// postfix returns the value that was loaded *before* the change —
    /// the one real difference from compound assignment.
    fn codegen_inc_dec(
        &mut self,
        op: IncDecOp,
        target: &Expr,
        is_prefix: bool,
        scopes: &mut Scopes<'ctx>,
    ) -> Option<BasicValueEnum<'ctx>> {
        let (ptr, elem_llvm_ty) = match target {
            Expr::Ident { name, .. } => {
                let ptr = self.resolve_var(scopes, name);
                let ty = self.llvm_basic_type(self.resolved_type(target.id()));
                (ptr, ty)
            }
            Expr::Index { array, index, .. } => {
                let ptr = self.codegen_index_ptr(array, index, scopes);
                let ty = self.array_elem_llvm_type(array);
                (ptr, ty)
            }
            _ => unreachable!("sema guarantees only Ident/Index are valid assignment targets"),
        };
        let current = self.builder.build_load(elem_llvm_ty, ptr, "curval").unwrap();
        let new_val: BasicValueEnum = if current.is_int_value() {
            let c = current.into_int_value();
            let one = c.get_type().const_int(1, false);
            match op {
                IncDecOp::Inc => self.builder.build_int_add(c, one, "inctmp").unwrap().into(),
                IncDecOp::Dec => self.builder.build_int_sub(c, one, "dectmp").unwrap().into(),
            }
        } else {
            let c = current.into_float_value();
            let one = c.get_type().const_float(1.0);
            match op {
                IncDecOp::Inc => self.builder.build_float_add(c, one, "inctmp").unwrap().into(),
                IncDecOp::Dec => self.builder.build_float_sub(c, one, "dectmp").unwrap().into(),
            }
        };
        self.builder.build_store(ptr, new_val).unwrap();
        Some(if is_prefix { new_val } else { current })
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
        // These 3 builtins are pre-registered in sema (see docs/P2/
        // ANX-P2-Strings-Plan-v1.md §2), not user `Decl::Func`s, so they'd
        // never be found in `self.functions` below — intercept by name
        // exactly like `print` above.
        match callee {
            "length" => return Some(self.codegen_field_access(&args[0], "length", scopes).unwrap()),
            "charAt" => return Some(self.codegen_char_at(&args[0], &args[1], scopes)),
            "substring" => {
                return Some(self.codegen_substring(&args[0], &args[1], &args[2], scopes))
            }
            _ => {}
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
            // Strings and arrays are both struct-valued (P2 changed strings
            // from a bare pointer to the same `{i64, ptr}` shape) — the LLVM
            // value alone can't tell them apart, so dispatch on sema's
            // resolved type instead.
            BasicValueEnum::StructValue(sv) => match self.resolved_type(arg.id()) {
                Type::Str => {
                    let data_ptr = self
                        .builder
                        .build_extract_value(sv, 1, "strdata")
                        .unwrap()
                        .into_pointer_value();
                    self.builder
                        .build_call(self.runtime.print_str, &[data_ptr.into()], "")
                        .unwrap();
                }
                Type::Array(_) => {
                    let len = self
                        .builder
                        .build_extract_value(sv, 0, "arrlen")
                        .unwrap()
                        .into_int_value();
                    let data_ptr = self
                        .builder
                        .build_extract_value(sv, 1, "arrdata")
                        .unwrap()
                        .into_pointer_value();
                    self.builder
                        .build_call(self.runtime.print_array, &[len.into(), data_ptr.into()], "")
                        .unwrap();
                }
                _ => unreachable!("sema guarantees only Str/Array are struct-valued"),
            },
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
        let length = self
            .builder
            .build_extract_value(arr_val, 0, "arrlen")
            .unwrap()
            .into_int_value();
        let data_ptr = self
            .builder
            .build_extract_value(arr_val, 1, "dataptr")
            .unwrap()
            .into_pointer_value();
        let idx = self.codegen_expr(index, scopes).unwrap().into_int_value();

        // One unsigned compare covers both bounds: a negative index wraps
        // to a huge u64, so `(u64)idx < (u64)length` iff `0 <= idx < length`.
        let in_bounds = self
            .builder
            .build_int_compare(IntPredicate::ULT, idx, length, "inbounds")
            .unwrap();
        self.emit_runtime_guard(
            in_bounds,
            self.runtime.panic_oob,
            &[idx.into(), length.into()],
            "oob",
        );

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
        debug_assert_eq!(field, "length", "sema guarantees only array.length/string.length");
        // Also reused directly by codegen_call for the `length(s)` builtin
        // (see docs/P2/ANX-P2-Strings-Plan-v1.md §2) — same field-0 extract,
        // no runtime call needed either way.
        let struct_val = self.codegen_expr(object, scopes).unwrap().into_struct_value();
        Some(
            self.builder
                .build_extract_value(struct_val, 0, "lentmp")
                .unwrap(),
        )
    }

    /// `charAt(s, i)` — bounds check is inline here (mirrors
    /// `codegen_index_ptr`'s array check almost line-for-line, per
    /// docs/P2/ANX-P2-Strings-Plan-v1.md Step 3); the actual 1-byte-string
    /// allocation happens in the `anx_str_char_at` C shim once the index is
    /// known valid.
    fn codegen_char_at(
        &mut self,
        s_expr: &Expr,
        idx_expr: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let s_val = self.codegen_expr(s_expr, scopes).unwrap().into_struct_value();
        let len = self
            .builder
            .build_extract_value(s_val, 0, "strlen")
            .unwrap()
            .into_int_value();
        let idx = self.codegen_expr(idx_expr, scopes).unwrap().into_int_value();

        // Same unsigned-compare trick as array indexing: a negative index
        // wraps to a huge u64, so this one check covers `0 <= idx < len`.
        let in_bounds = self
            .builder
            .build_int_compare(IntPredicate::ULT, idx, len, "charinbounds")
            .unwrap();
        self.emit_runtime_guard(
            in_bounds,
            self.runtime.panic_str_oob,
            &[idx.into(), len.into()],
            "charoob",
        );

        self.builder
            .build_call(self.runtime.str_char_at, &[s_val.into(), idx.into()], "charat")
            .unwrap()
            .try_as_basic_value()
            .basic()
            .unwrap()
    }

    /// `substring(s, start, end)` — two sequential inline guards (start in
    /// `[0, len]`, then end in `[start, len]`), same "guard, then let the
    /// shim do the real work" shape as `codegen_char_at` above.
    fn codegen_substring(
        &mut self,
        s_expr: &Expr,
        start_expr: &Expr,
        end_expr: &Expr,
        scopes: &mut Scopes<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let s_val = self.codegen_expr(s_expr, scopes).unwrap().into_struct_value();
        let len = self
            .builder
            .build_extract_value(s_val, 0, "strlen")
            .unwrap()
            .into_int_value();
        let start = self.codegen_expr(start_expr, scopes).unwrap().into_int_value();
        let end = self.codegen_expr(end_expr, scopes).unwrap().into_int_value();

        let start_in_range = self
            .builder
            .build_int_compare(IntPredicate::ULE, start, len, "startinrange")
            .unwrap();
        self.emit_runtime_guard(
            start_in_range,
            self.runtime.panic_str_oob,
            &[start.into(), len.into()],
            "substart",
        );

        let end_le_len = self
            .builder
            .build_int_compare(IntPredicate::ULE, end, len, "endinrange")
            .unwrap();
        let start_le_end = self
            .builder
            .build_int_compare(IntPredicate::SLE, start, end, "startleend")
            .unwrap();
        let end_valid = self.builder.build_and(end_le_len, start_le_end, "endok").unwrap();
        self.emit_runtime_guard(
            end_valid,
            self.runtime.panic_str_oob,
            &[end.into(), len.into()],
            "subend",
        );

        self.builder
            .build_call(
                self.runtime.str_substring,
                &[s_val.into(), start.into(), end.into()],
                "substr",
            )
            .unwrap()
            .try_as_basic_value()
            .basic()
            .unwrap()
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
        self.build_len_data_struct(n_val, data_ptr)
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

        let zero = self.context.i64_type().const_int(0, false);
        let non_negative = self
            .builder
            .build_int_compare(IntPredicate::SGE, n, zero, "nonneg")
            .unwrap();
        self.emit_runtime_guard(
            non_negative,
            self.runtime.panic_neg_size,
            &[n.into()],
            "negsize",
        );

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

        self.build_len_data_struct(n, data_ptr)
    }

    fn build_len_data_struct(
        &self,
        length_val: IntValue<'ctx>,
        data_ptr: PointerValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let struct_ty = self.len_data_struct_type();
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

    // The generated code references the runtime shim's panic functions
    // (bounds/div-zero/negative-size guards), which only exist as C symbols
    // after `anx build` links runtime.c. For in-process JIT execution, map
    // them to Rust stubs instead — the happy-path tests below must never
    // actually reach them, so they panic the test if called.
    extern "C" fn jit_stub_panic_oob(index: i64, length: i64) {
        panic!("JIT hit bounds panic: index {index}, length {length}");
    }
    extern "C" fn jit_stub_panic_div_zero() {
        panic!("JIT hit division-by-zero panic");
    }
    extern "C" fn jit_stub_panic_neg_size(size: i64) {
        panic!("JIT hit negative-array-size panic: {size}");
    }
    extern "C" fn jit_stub_panic_str_oob(index: i64, length: i64) {
        panic!("JIT hit string bounds panic: index {index}, length {length}");
    }

    /// Bit-for-bit mirrors codegen's `{ i64 length, ptr data }` LLVM struct
    /// (`repr(C)`, same two 8-byte fields, no padding) — needed so these
    /// stand-in Rust functions can be called with the exact by-value struct
    /// ABI the JIT-compiled IR expects.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct JitAnxStr {
        length: i64,
        data: *const u8,
    }

    unsafe extern "C" {
        fn malloc(size: usize) -> *mut u8;
    }

    // Real (not panic-stub) implementations of the string runtime shim,
    // mirroring runtime.c's anx_str_* functions exactly — unlike the panic
    // stubs above, the happy-path string JIT tests actually need these to
    // work, not just exist. `anx build` links the real C versions instead;
    // this in-process JIT path can't reach runtime.c at all, so a same-logic
    // Rust stand-in is the only way to JIT-execute these builtins directly.
    extern "C" fn jit_stub_str_concat(a: JitAnxStr, b: JitAnxStr) -> JitAnxStr {
        unsafe {
            let new_len = (a.length + b.length) as usize;
            let buf = malloc(new_len + 1);
            std::ptr::copy_nonoverlapping(a.data, buf, a.length as usize);
            std::ptr::copy_nonoverlapping(b.data, buf.add(a.length as usize), b.length as usize);
            *buf.add(new_len) = 0;
            JitAnxStr { length: new_len as i64, data: buf }
        }
    }
    extern "C" fn jit_stub_str_char_at(s: JitAnxStr, i: i64) -> JitAnxStr {
        unsafe {
            let buf = malloc(2);
            *buf = *s.data.add(i as usize);
            *buf.add(1) = 0;
            JitAnxStr { length: 1, data: buf }
        }
    }
    extern "C" fn jit_stub_str_substring(s: JitAnxStr, start: i64, end: i64) -> JitAnxStr {
        unsafe {
            let new_len = (end - start) as usize;
            let buf = malloc(new_len + 1);
            std::ptr::copy_nonoverlapping(s.data.add(start as usize), buf, new_len);
            *buf.add(new_len) = 0;
            JitAnxStr { length: new_len as i64, data: buf }
        }
    }
    extern "C" fn jit_stub_str_equals(a: JitAnxStr, b: JitAnxStr) -> i8 {
        if a.length != b.length {
            return 0;
        }
        unsafe {
            let sa = std::slice::from_raw_parts(a.data, a.length as usize);
            let sb = std::slice::from_raw_parts(b.data, b.length as usize);
            (sa == sb) as i8
        }
    }

    fn map_runtime_stubs(codegen: &Codegen, ee: &inkwell::execution_engine::ExecutionEngine) {
        let mappings: [(&str, usize); 8] = [
            ("anx_panic_oob", jit_stub_panic_oob as *const () as usize),
            ("anx_panic_div_zero", jit_stub_panic_div_zero as *const () as usize),
            ("anx_panic_neg_size", jit_stub_panic_neg_size as *const () as usize),
            ("anx_panic_str_oob", jit_stub_panic_str_oob as *const () as usize),
            ("anx_str_concat", jit_stub_str_concat as *const () as usize),
            ("anx_str_char_at", jit_stub_str_char_at as *const () as usize),
            ("anx_str_substring", jit_stub_str_substring as *const () as usize),
            ("anx_str_equals", jit_stub_str_equals as *const () as usize),
        ];
        for (name, addr) in mappings {
            if let Some(f) = codegen.module().get_function(name) {
                ee.add_global_mapping(&f, addr);
            }
        }
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
        map_runtime_stubs(&codegen, &ee);
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
        map_runtime_stubs(&codegen, &ee);
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
        map_runtime_stubs(&codegen, &ee);
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

    // ---- P1 Phase 9 (Operators) ----

    #[test]
    fn compiles_ternary_expression() {
        assert_verifies(
            r#"
            void main() {
                int a = 5;
                int b = 3;
                int m = a > b ? a : b;
                print(m);
            }
            "#,
        );
    }

    #[test]
    fn jit_ternary_is_correct() {
        let source = r#"
            int maxOf(int a, int b) {
                return a > b ? a : b;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_2(source, "maxOf", 5, 3), 5);
        assert_eq!(jit_i64_2(source, "maxOf", 3, 5), 5);
    }

    #[test]
    fn compiles_bitwise_and_shift_operators() {
        assert_verifies(
            r#"
            void main() {
                int a = 5 & 3;
                int b = 5 | 2;
                int c = 5 ^ 1;
                int d = ~5;
                int e = 1 << 4;
                int f = 16 >> 2;
                print(a);
                print(b);
                print(c);
                print(d);
                print(e);
                print(f);
            }
            "#,
        );
    }

    #[test]
    fn jit_bitwise_and_shift_are_correct() {
        let source = r#"
            int combine(int a, int b) {
                return ((a & b) | (a ^ b)) << 1;
            }
            void main() { }
        "#;
        // (5 & 3) | (5 ^ 3) = 1 | 6 = 7; 7 << 1 = 14
        assert_eq!(jit_i64_2(source, "combine", 5, 3), 14);
    }

    #[test]
    fn jit_bitwise_not_is_correct() {
        let source = r#"
            int flip(int a) { return ~a; }
            void main() { }
        "#;
        assert_eq!(jit_i64_1(source, "flip", 5), -6);
        assert_eq!(jit_i64_1(source, "flip", 0), -1);
    }

    #[test]
    fn compiles_compound_assignment_on_variable_and_array_index() {
        assert_verifies(
            r#"
            void main() {
                int x = 1;
                x += 2;
                x -= 1;
                x *= 3;
                x /= 2;
                x %= 2;
                x &= 3;
                x |= 4;
                x ^= 1;
                x <<= 2;
                x >>= 1;
                int[] arr = [1, 2, 3];
                arr[0] += 10;
                print(x);
                print(arr[0]);
            }
            "#,
        );
    }

    #[test]
    fn jit_compound_assignment_is_correct() {
        let source = r#"
            int accumulate(int n) {
                int total = 0;
                int i = 0;
                while (i < n) {
                    total += i;
                    i += 1;
                }
                return total;
            }
            void main() { }
        "#;
        // sum of 0..4 = 0+1+2+3 = 6
        assert_eq!(jit_i64_1(source, "accumulate", 4), 6);
    }

    #[test]
    fn jit_compound_assign_on_array_index_evaluates_index_once() {
        // Same double-evaluation hazard as the interpreter test, checked at
        // the compiled-code level: nextIndex() must be called exactly once
        // even though arr[nextIndex()] += 5 reads-and-writes the slot. Uses
        // an array as the call counter (not a global — codegen currently has
        // no support for top-level `Decl::Var` at all, a separate pre-
        // existing gap unrelated to this operators work; see the P1
        // Progress tracker).
        let source = r#"
            int nextIndex(int[] counter) {
                counter[0] += 1;
                return 0;
            }
            int run() {
                int[] arr = [10];
                int[] counter = [0];
                arr[nextIndex(counter)] += 5;
                return arr[0] * 100 + counter[0];
            }
            void main() { }
        "#;
        // arr[0] becomes 15, counter[0] becomes 1 -> 15*100 + 1 = 1501
        assert_eq!(jit_i64_0(source, "run"), 1501);
    }

    #[test]
    fn compiles_unsigned_right_shift() {
        assert_verifies(
            r#"
            void main() {
                int x = -1 >>> 1;
                print(x);
            }
            "#,
        );
    }

    #[test]
    fn jit_unsigned_right_shift_is_correct() {
        let source = r#"
            int ushr(int a, int b) { return a >>> b; }
            void main() { }
        "#;
        assert_eq!(jit_i64_2(source, "ushr", -1, 1), i64::MAX);
        assert_eq!(jit_i64_2(source, "ushr", -1, 0), -1);
    }

    #[test]
    fn compiles_prefix_and_postfix_increment_decrement() {
        assert_verifies(
            r#"
            void main() {
                int x = 5;
                int a = ++x;
                int b = x++;
                int c = --x;
                int d = x--;
                print(a);
                print(b);
                print(c);
                print(d);
                int[] arr = [1, 2, 3];
                arr[0]++;
                ++arr[1];
            }
            "#,
        );
    }

    #[test]
    fn jit_prefix_increment_returns_new_value() {
        let source = r#"
            int preInc(int x) { return ++x; }
            void main() { }
        "#;
        assert_eq!(jit_i64_1(source, "preInc", 5), 6);
    }

    #[test]
    fn jit_postfix_increment_returns_old_value() {
        let source = r#"
            int postInc(int x) { return x++; }
            void main() { }
        "#;
        assert_eq!(jit_i64_1(source, "postInc", 5), 5);
    }

    #[test]
    fn jit_postfix_decrement_returns_old_value_and_mutates() {
        // Confirms the mutation actually happened, not just the return value.
        let source = r#"
            int postDecTwice(int x) {
                int first = x--;
                int second = x--;
                return first * 100 + second;
            }
            void main() { }
        "#;
        // first = 5 (pre-mutation value), x becomes 4; second = 4, x becomes 3
        assert_eq!(jit_i64_1(source, "postDecTwice", 5), 504);
    }

    #[test]
    fn jit_increment_on_array_index_evaluates_index_once() {
        let source = r#"
            int nextIndex(int[] counter) {
                counter[0] += 1;
                return 0;
            }
            int run() {
                int[] arr = [10];
                int[] counter = [0];
                arr[nextIndex(counter)]++;
                return arr[0] * 100 + counter[0];
            }
            void main() { }
        "#;
        // arr[0] becomes 11, counter[0] becomes 1 -> 11*100 + 1 = 1101
        assert_eq!(jit_i64_0(source, "run"), 1101);
    }

    #[test]
    fn jit_increment_works_in_for_loop() {
        let source = r#"
            int sumTo(int n) {
                int total = 0;
                for (int i = 0; i < n; i++) {
                    total += i;
                }
                return total;
            }
            void main() { }
        "#;
        // 0+1+2+3+4 = 10
        assert_eq!(jit_i64_1(source, "sumTo", 5), 10);
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

    // ---- P2 (Strings) ----

    #[test]
    fn compiles_string_builtins_field_access_and_operators() {
        assert_verifies(
            r#"
            void main() {
                string s = "hello";
                int n = length(s);
                int fieldLen = s.length;
                string c = charAt(s, 0);
                string sub = substring(s, 1, 3);
                string cat = s + " world";
                bool eq = s == "hello";
                bool neq = s != "world";
                print(n);
                print(fieldLen);
                print(c);
                print(sub);
                print(cat);
                print(eq);
                print(neq);
            }
            "#,
        );
    }

    #[test]
    fn compiles_string_ternary_and_compound_assign() {
        assert_verifies(
            r#"
            void main() {
                string a = "foo";
                string b = "bar";
                string picked = true ? a : b;
                a += b;
                print(picked);
                print(a);
            }
            "#,
        );
    }

    /// JIT-executes int-returning wrapper functions that exercise the string
    /// runtime shim internally (concat, substring, charAt, equals) — the
    /// same "verify() isn't enough, check real computed values" bar Phase 5
    /// set for arrays. Wrapped as int-returning per the existing
    /// scalar-in/scalar-out JIT restriction (see `jit_i64_0` doc comment):
    /// string params/returns can't be safely hand-matched to a raw Rust
    /// function pointer's ABI, but an internal-only `string` local is fine.
    #[test]
    fn jit_string_concat_length_and_equality_are_correct() {
        let source = r#"
            int concatCheck() {
                string a = "foo";
                string b = "bar";
                string c = a + b;
                if (length(c) != 6) return 0;
                if (c != "foobar") return 0;
                return 1;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_0(source, "concatCheck"), 1);
    }

    #[test]
    fn jit_string_char_at_is_correct() {
        let source = r#"
            int charAtCheck() {
                string s = "hello";
                if (charAt(s, 0) != "h") return 0;
                if (charAt(s, 4) != "o") return 0;
                return 1;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_0(source, "charAtCheck"), 1);
    }

    #[test]
    fn jit_string_substring_is_correct() {
        let source = r#"
            int substringCheck() {
                string s = "hello world";
                if (substring(s, 6, 11) != "world") return 0;
                if (substring(s, 0, 5) != "hello") return 0;
                if (length(substring(s, 2, 2)) != 0) return 0;
                return 1;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_0(source, "substringCheck"), 1);
    }

    #[test]
    fn jit_string_length_field_access_is_correct() {
        let source = r#"
            int fieldLengthCheck() {
                string s = "hello";
                return s.length;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_0(source, "fieldLengthCheck"), 5);
    }

    #[test]
    fn jit_string_inequality_is_correct() {
        let source = r#"
            int notEqualsCheck() {
                string a = "abc";
                string b = "abd";
                if (a != b) return 1;
                return 0;
            }
            void main() { }
        "#;
        assert_eq!(jit_i64_0(source, "notEqualsCheck"), 1);
    }
}

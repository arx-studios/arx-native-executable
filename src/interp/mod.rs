pub mod value;

use crate::ast::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use thiserror::Error;
use value::{as_bool, as_int, default_value, format_value, numeric_cmp, numeric_op, Value};

#[derive(Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error("array index {index} out of bounds for length {length}")]
    IndexOutOfBounds { index: i64, length: usize },
    #[error("division by zero")]
    DivisionByZero,
    #[error("array size must be non-negative, found {size}")]
    NegativeArraySize { size: i64 },
    #[error("string index {index} out of bounds for length {length}")]
    StringIndexOutOfBounds { index: i64, length: usize },
}

pub fn interpret(program: &Program) -> Result<(), RuntimeError> {
    Interpreter::new(program).run()
}

struct Environment {
    vars: RefCell<HashMap<String, Value>>,
    parent: Option<Rc<Environment>>,
}

impl Environment {
    fn new() -> Rc<Self> {
        Rc::new(Environment {
            vars: RefCell::new(HashMap::new()),
            parent: None,
        })
    }

    fn child(parent: &Rc<Environment>) -> Rc<Self> {
        Rc::new(Environment {
            vars: RefCell::new(HashMap::new()),
            parent: Some(Rc::clone(parent)),
        })
    }

    fn define(&self, name: &str, value: Value) {
        self.vars.borrow_mut().insert(name.to_string(), value);
    }

    fn get(&self, name: &str) -> Value {
        if let Some(v) = self.vars.borrow().get(name) {
            return v.clone();
        }
        match &self.parent {
            Some(p) => p.get(name),
            None => panic!("sema guarantees '{name}' is declared before use"),
        }
    }

    fn set(&self, name: &str, value: Value) {
        if self.vars.borrow().contains_key(name) {
            self.vars.borrow_mut().insert(name.to_string(), value);
            return;
        }
        match &self.parent {
            Some(p) => p.set(name, value),
            None => panic!("sema guarantees '{name}' is declared before assignment"),
        }
    }
}

/// Signals an in-flight `return` propagating up through nested statements.
enum Flow {
    Normal,
    Return(Value),
}

struct Interpreter<'a> {
    program: &'a Program,
    functions: HashMap<&'a str, &'a FuncDecl>,
    globals: Rc<Environment>,
    output: Box<dyn Fn(&str) + 'a>,
}

impl<'a> Interpreter<'a> {
    fn new(program: &'a Program) -> Self {
        Self::with_output(program, |s| println!("{s}"))
    }

    /// Same as `new`, but routes `print()` through a caller-supplied sink
    /// instead of real stdout — used by tests to assert on program output
    /// without spawning a subprocess (that's Phase 6/7's job).
    fn with_output(program: &'a Program, output: impl Fn(&str) + 'a) -> Self {
        let mut functions = HashMap::new();
        for decl in &program.decls {
            if let Decl::Func(f) = decl {
                functions.insert(f.name.as_str(), f);
            }
        }
        Interpreter {
            program,
            functions,
            globals: Environment::new(),
            output: Box::new(output),
        }
    }

    fn run(&self) -> Result<(), RuntimeError> {
        for decl in &self.program.decls {
            if let Decl::Var(vd) = decl {
                let val = match &vd.init {
                    Some(e) => self.eval_expr(e, &self.globals)?,
                    None => default_value(&vd.ty),
                };
                self.globals.define(&vd.name, val);
            }
        }
        self.call_function("main", &[])?;
        Ok(())
    }

    fn call_function(&self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        let f = *self
            .functions
            .get(name)
            .expect("sema guarantees the called function exists");
        // Every call gets a fresh frame parented directly to globals — ANX
        // functions aren't closures, matching sema's per-function scoping.
        let env = Environment::child(&self.globals);
        for (param, arg) in f.params.iter().zip(args.iter()) {
            env.define(&param.name, arg.clone());
        }
        match self.exec_block(&f.body.stmts, &env)? {
            Flow::Return(v) => Ok(v),
            Flow::Normal => Ok(Value::Void),
        }
    }

    fn exec_block(&self, stmts: &'a [Stmt], env: &Rc<Environment>) -> Result<Flow, RuntimeError> {
        for stmt in stmts {
            match self.exec_stmt(stmt, env)? {
                Flow::Normal => {}
                ret @ Flow::Return(_) => return Ok(ret),
            }
        }
        Ok(Flow::Normal)
    }

    /// Wraps a control-flow body (if/while/for branch) in its own scope
    /// unless it's already a `{ }` block, which scopes itself — mirrors
    /// sema's `check_scoped_stmt` so behavior can't diverge from what
    /// type-checked.
    fn exec_scoped(&self, stmt: &'a Stmt, env: &Rc<Environment>) -> Result<Flow, RuntimeError> {
        if matches!(stmt, Stmt::Block(_)) {
            self.exec_stmt(stmt, env)
        } else {
            let child = Environment::child(env);
            self.exec_stmt(stmt, &child)
        }
    }

    fn exec_stmt(&self, stmt: &'a Stmt, env: &Rc<Environment>) -> Result<Flow, RuntimeError> {
        match stmt {
            Stmt::Expr(e) => {
                self.eval_expr(e, env)?;
                Ok(Flow::Normal)
            }
            Stmt::VarDecl(vd) => {
                let val = match &vd.init {
                    Some(e) => self.eval_expr(e, env)?,
                    None => default_value(&vd.ty),
                };
                env.define(&vd.name, val);
                Ok(Flow::Normal)
            }
            Stmt::Block(b) => {
                let child = Environment::child(env);
                self.exec_block(&b.stmts, &child)
            }
            Stmt::If(if_stmt) => {
                if as_bool(&self.eval_expr(&if_stmt.cond, env)?) {
                    self.exec_scoped(&if_stmt.then_branch, env)
                } else if let Some(else_branch) = &if_stmt.else_branch {
                    self.exec_scoped(else_branch, env)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Stmt::While(w) => {
                while as_bool(&self.eval_expr(&w.cond, env)?) {
                    match self.exec_scoped(&w.body, env)? {
                        Flow::Normal => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::For(f) => {
                let loop_env = Environment::child(env);
                self.exec_stmt(&f.init, &loop_env)?;
                while as_bool(&self.eval_expr(&f.cond, &loop_env)?) {
                    match self.exec_scoped(&f.body, &loop_env)? {
                        Flow::Normal => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                    self.eval_expr(&f.update, &loop_env)?;
                }
                Ok(Flow::Normal)
            }
            Stmt::Return(r) => {
                let val = match &r.value {
                    Some(e) => self.eval_expr(e, env)?,
                    None => Value::Void,
                };
                Ok(Flow::Return(val))
            }
        }
    }

    fn eval_expr(&self, expr: &'a Expr, env: &Rc<Environment>) -> Result<Value, RuntimeError> {
        match expr {
            Expr::IntLiteral { value, .. } => Ok(Value::Int(*value)),
            Expr::FloatLiteral { value, .. } => Ok(Value::Float(*value)),
            Expr::BoolLiteral { value, .. } => Ok(Value::Bool(*value)),
            Expr::StringLiteral { value, .. } => Ok(Value::Str(value.clone())),
            Expr::Null { .. } => Ok(Value::Void),
            Expr::Ident { name, .. } => Ok(env.get(name)),
            Expr::Binary { op, left, right, .. } => self.eval_binary(*op, left, right, env),
            Expr::Unary { op, operand, .. } => self.eval_unary(*op, operand, env),
            Expr::Assign { target, value, .. } => self.eval_assign(target, value, env),
            Expr::CompoundAssign { op, target, value, .. } => {
                self.eval_compound_assign(*op, target, value, env)
            }
            Expr::IncDec { op, target, is_prefix, .. } => {
                self.eval_inc_dec(*op, target, *is_prefix, env)
            }
            Expr::Ternary { cond, then_branch, else_branch, .. } => {
                if as_bool(&self.eval_expr(cond, env)?) {
                    self.eval_expr(then_branch, env)
                } else {
                    self.eval_expr(else_branch, env)
                }
            }
            Expr::Call { callee, args, .. } => self.eval_call(callee, args, env),
            Expr::Index { array, index, .. } => self.eval_index(array, index, env),
            Expr::FieldAccess { object, field, .. } => self.eval_field_access(object, field, env),
            Expr::ArrayLiteral { elements, .. } => {
                let mut values = Vec::with_capacity(elements.len());
                for el in elements {
                    values.push(self.eval_expr(el, env)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(values))))
            }
            Expr::ArrayCreation { elem_ty, size, .. } => {
                let n = as_int(&self.eval_expr(size, env)?);
                if n < 0 {
                    return Err(RuntimeError::NegativeArraySize { size: n });
                }
                let values = vec![default_value(elem_ty); n as usize];
                Ok(Value::Array(Rc::new(RefCell::new(values))))
            }
        }
    }

    fn eval_binary(
        &self,
        op: BinOp,
        left: &'a Expr,
        right: &'a Expr,
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        // && and || short-circuit, so the right operand must be evaluated
        // lazily rather than alongside the left as the other operators do.
        match op {
            BinOp::And => {
                let l = self.eval_expr(left, env)?;
                if !as_bool(&l) {
                    return Ok(Value::Bool(false));
                }
                return Ok(Value::Bool(as_bool(&self.eval_expr(right, env)?)));
            }
            BinOp::Or => {
                let l = self.eval_expr(left, env)?;
                if as_bool(&l) {
                    return Ok(Value::Bool(true));
                }
                return Ok(Value::Bool(as_bool(&self.eval_expr(right, env)?)));
            }
            _ => {}
        }

        let l = self.eval_expr(left, env)?;
        let r = self.eval_expr(right, env)?;
        self.apply_binary_op(op, l, r)
    }

    /// The op-specific value computation, shared by `eval_binary` (once
    /// both operands are freshly evaluated) and `eval_compound_assign`
    /// (target's current value + the RHS). Never called with `And`/`Or` —
    /// those short-circuit in `eval_binary` before reaching here, and the
    /// parser never produces a `CompoundAssign` with those ops (no `&&=`/
    /// `||=` in the grammar).
    fn apply_binary_op(&self, op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
        match op {
            BinOp::Add => match (l, r) {
                (Value::Str(a), Value::Str(b)) => Ok(Value::Str(a + &b)),
                (l, r) => Ok(numeric_op(l, r, |a, b| a + b, |a, b| a + b)),
            },
            BinOp::Sub => Ok(numeric_op(l, r, |a, b| a - b, |a, b| a - b)),
            BinOp::Mul => Ok(numeric_op(l, r, |a, b| a * b, |a, b| a * b)),
            BinOp::Div => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        Err(RuntimeError::DivisionByZero)
                    } else {
                        Ok(Value::Int(a / b))
                    }
                }
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
                _ => unreachable!("sema guarantees matching numeric operand types"),
            },
            BinOp::Mod => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        Err(RuntimeError::DivisionByZero)
                    } else {
                        Ok(Value::Int(a % b))
                    }
                }
                _ => unreachable!("sema guarantees int operands for %"),
            },
            BinOp::BitAnd => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a & b)),
                _ => unreachable!("sema guarantees int operands for &"),
            },
            BinOp::BitOr => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a | b)),
                _ => unreachable!("sema guarantees int operands for |"),
            },
            BinOp::BitXor => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a ^ b)),
                _ => unreachable!("sema guarantees int operands for ^"),
            },
            // wrapping_sh{l,r} (not `<<`/`>>`) deliberately avoid a Rust-level
            // panic on out-of-range shift amounts — matches the plan's "not
            // range-checked, same as LLVM" stance without crashing the
            // interpreter process to get there.
            BinOp::Shl => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_shl(b as u32))),
                _ => unreachable!("sema guarantees int operands for <<"),
            },
            BinOp::Shr => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_shr(b as u32))),
                _ => unreachable!("sema guarantees int operands for >>"),
            },
            // Logical (zero-filling) right shift: reinterpret as unsigned
            // before shifting, so the vacated high bits are 0 instead of a
            // sign-extended 1 — the only way `>>>` differs from `>>`.
            BinOp::UShr => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    Ok(Value::Int((a as u64).wrapping_shr(b as u32) as i64))
                }
                _ => unreachable!("sema guarantees int operands for >>>"),
            },
            BinOp::Lt => Ok(Value::Bool(numeric_cmp(&l, &r) == std::cmp::Ordering::Less)),
            BinOp::LtEq => Ok(Value::Bool(
                numeric_cmp(&l, &r) != std::cmp::Ordering::Greater,
            )),
            BinOp::Gt => Ok(Value::Bool(
                numeric_cmp(&l, &r) == std::cmp::Ordering::Greater,
            )),
            BinOp::GtEq => Ok(Value::Bool(numeric_cmp(&l, &r) != std::cmp::Ordering::Less)),
            BinOp::Eq => Ok(Value::Bool(l == r)),
            BinOp::NotEq => Ok(Value::Bool(l != r)),
            BinOp::And | BinOp::Or => unreachable!("handled by eval_binary before this point"),
        }
    }

    fn eval_unary(
        &self,
        op: UnOp,
        operand: &'a Expr,
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(operand, env)?;
        match (op, v) {
            (UnOp::Neg, Value::Int(a)) => Ok(Value::Int(-a)),
            (UnOp::Neg, Value::Float(a)) => Ok(Value::Float(-a)),
            (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
            (UnOp::BitNot, Value::Int(a)) => Ok(Value::Int(!a)),
            _ => unreachable!("sema guarantees a matching operand type for unary ops"),
        }
    }

    fn eval_assign(
        &self,
        target: &'a Expr,
        value: &'a Expr,
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        let val = self.eval_expr(value, env)?;
        match target {
            Expr::Ident { name, .. } => {
                env.set(name, val.clone());
            }
            Expr::Index { array, index, .. } => {
                let arr_val = self.eval_expr(array, env)?;
                let idx = as_int(&self.eval_expr(index, env)?);
                match arr_val {
                    Value::Array(cells) => {
                        let mut cells = cells.borrow_mut();
                        if idx < 0 || idx as usize >= cells.len() {
                            return Err(RuntimeError::IndexOutOfBounds {
                                index: idx,
                                length: cells.len(),
                            });
                        }
                        cells[idx as usize] = val.clone();
                    }
                    _ => unreachable!("sema guarantees an array type for index assignment"),
                }
            }
            _ => unreachable!("sema guarantees only Ident/Index are valid assignment targets"),
        }
        Ok(val)
    }

    /// Reads the target's current value, evaluates the RHS, combines via
    /// `op`, and writes back — evaluating the target's array+index (if any)
    /// exactly once, not twice as a naive `target = target op value`
    /// desugaring would (see docs/P1/ANX-P1-Operators-Plan-v1.md §1).
    fn eval_compound_assign(
        &self,
        op: BinOp,
        target: &'a Expr,
        value: &'a Expr,
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        match target {
            Expr::Ident { name, .. } => {
                let current = env.get(name);
                let rhs = self.eval_expr(value, env)?;
                let new_val = self.apply_binary_op(op, current, rhs)?;
                env.set(name, new_val.clone());
                Ok(new_val)
            }
            Expr::Index { array, index, .. } => {
                let arr_val = self.eval_expr(array, env)?;
                let idx = as_int(&self.eval_expr(index, env)?);
                match arr_val {
                    Value::Array(cells) => {
                        let current = {
                            let cells_ref = cells.borrow();
                            if idx < 0 || idx as usize >= cells_ref.len() {
                                return Err(RuntimeError::IndexOutOfBounds {
                                    index: idx,
                                    length: cells_ref.len(),
                                });
                            }
                            cells_ref[idx as usize].clone()
                        };
                        let rhs = self.eval_expr(value, env)?;
                        let new_val = self.apply_binary_op(op, current, rhs)?;
                        cells.borrow_mut()[idx as usize] = new_val.clone();
                        Ok(new_val)
                    }
                    _ => unreachable!("sema guarantees an array type for index compound-assignment"),
                }
            }
            _ => unreachable!("sema guarantees only Ident/Index are valid assignment targets"),
        }
    }

    fn apply_inc_dec(op: IncDecOp, v: Value) -> Value {
        match (op, v) {
            (IncDecOp::Inc, Value::Int(a)) => Value::Int(a + 1),
            (IncDecOp::Dec, Value::Int(a)) => Value::Int(a - 1),
            (IncDecOp::Inc, Value::Float(a)) => Value::Float(a + 1.0),
            (IncDecOp::Dec, Value::Float(a)) => Value::Float(a - 1.0),
            _ => unreachable!("sema guarantees an int or float operand for ++/--"),
        }
    }

    /// Same "resolve the target's slot exactly once" concern as
    /// `eval_compound_assign` — `arr[f()]++` must call `f()` once. Prefix
    /// returns the *new* value; postfix returns the value *before* the
    /// change, which is the one real difference from compound assignment.
    fn eval_inc_dec(
        &self,
        op: IncDecOp,
        target: &'a Expr,
        is_prefix: bool,
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        match target {
            Expr::Ident { name, .. } => {
                let current = env.get(name);
                let new_val = Self::apply_inc_dec(op, current.clone());
                env.set(name, new_val.clone());
                Ok(if is_prefix { new_val } else { current })
            }
            Expr::Index { array, index, .. } => {
                let arr_val = self.eval_expr(array, env)?;
                let idx = as_int(&self.eval_expr(index, env)?);
                match arr_val {
                    Value::Array(cells) => {
                        let current = {
                            let cells_ref = cells.borrow();
                            if idx < 0 || idx as usize >= cells_ref.len() {
                                return Err(RuntimeError::IndexOutOfBounds {
                                    index: idx,
                                    length: cells_ref.len(),
                                });
                            }
                            cells_ref[idx as usize].clone()
                        };
                        let new_val = Self::apply_inc_dec(op, current.clone());
                        cells.borrow_mut()[idx as usize] = new_val.clone();
                        Ok(if is_prefix { new_val } else { current })
                    }
                    _ => unreachable!("sema guarantees an array type for index increment/decrement"),
                }
            }
            _ => unreachable!("sema guarantees only Ident/Index are valid assignment targets"),
        }
    }

    fn eval_call(
        &self,
        callee: &str,
        args: &'a [Expr],
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        if callee == "print" {
            let val = self.eval_expr(&args[0], env)?;
            (self.output)(&format_value(&val));
            return Ok(Value::Void);
        }
        let mut arg_values = Vec::with_capacity(args.len());
        for a in args {
            arg_values.push(self.eval_expr(a, env)?);
        }
        match callee {
            "length" => return self.eval_string_length(&arg_values),
            "charAt" => return self.eval_char_at(&arg_values),
            "substring" => return self.eval_substring(&arg_values),
            _ => {}
        }
        self.call_function(callee, &arg_values)
    }

    // `length()` counts bytes, not Unicode scalars — matches what's cheap in
    // the LLVM runtime shim (strlen-equivalent) and what Java's `.length()`
    // effectively does too (see docs/P2/ANX-P2-Strings-Plan-v1.md §5).
    fn eval_string_length(&self, args: &[Value]) -> Result<Value, RuntimeError> {
        match &args[0] {
            Value::Str(s) => Ok(Value::Int(s.len() as i64)),
            _ => unreachable!("sema guarantees a string operand for length()"),
        }
    }

    fn eval_char_at(&self, args: &[Value]) -> Result<Value, RuntimeError> {
        match (&args[0], &args[1]) {
            (Value::Str(s), Value::Int(i)) => {
                let bytes = s.as_bytes();
                if *i < 0 || *i as usize >= bytes.len() {
                    return Err(RuntimeError::StringIndexOutOfBounds {
                        index: *i,
                        length: bytes.len(),
                    });
                }
                Ok(Value::Str((bytes[*i as usize] as char).to_string()))
            }
            _ => unreachable!("sema guarantees (string, int) operands for charAt()"),
        }
    }

    fn eval_substring(&self, args: &[Value]) -> Result<Value, RuntimeError> {
        match (&args[0], &args[1], &args[2]) {
            (Value::Str(s), Value::Int(start), Value::Int(end)) => {
                let bytes = s.as_bytes();
                let len = bytes.len();
                if *start < 0 || *start as usize > len {
                    return Err(RuntimeError::StringIndexOutOfBounds {
                        index: *start,
                        length: len,
                    });
                }
                if *end < *start || *end as usize > len {
                    return Err(RuntimeError::StringIndexOutOfBounds {
                        index: *end,
                        length: len,
                    });
                }
                let slice = &bytes[*start as usize..*end as usize];
                Ok(Value::Str(String::from_utf8_lossy(slice).into_owned()))
            }
            _ => unreachable!("sema guarantees (string, int, int) operands for substring()"),
        }
    }

    fn eval_index(
        &self,
        array: &'a Expr,
        index: &'a Expr,
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        let arr_val = self.eval_expr(array, env)?;
        let idx = as_int(&self.eval_expr(index, env)?);
        match arr_val {
            Value::Array(cells) => {
                let cells = cells.borrow();
                if idx < 0 || idx as usize >= cells.len() {
                    return Err(RuntimeError::IndexOutOfBounds {
                        index: idx,
                        length: cells.len(),
                    });
                }
                Ok(cells[idx as usize].clone())
            }
            _ => unreachable!("sema guarantees an array type for indexing"),
        }
    }

    fn eval_field_access(
        &self,
        object: &'a Expr,
        field: &str,
        env: &Rc<Environment>,
    ) -> Result<Value, RuntimeError> {
        let obj_val = self.eval_expr(object, env)?;
        match (obj_val, field) {
            (Value::Array(cells), "length") => Ok(Value::Int(cells.borrow().len() as i64)),
            (Value::Str(s), "length") => Ok(Value::Int(s.len() as i64)),
            _ => unreachable!("sema guarantees only array.length/string.length field access"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn run_captured(source: &str) -> Vec<String> {
        let tokens = lex(source).expect("should lex");
        let program = parse(tokens).expect("should parse");
        crate::sema::analyze(&program).expect("should type-check");
        let captured = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&captured);
        let interp = Interpreter::with_output(&program, move |s| sink.borrow_mut().push(s.to_string()));
        interp.run().expect("should run without a runtime error");
        drop(interp); // releases the closure's Rc clone so try_unwrap below succeeds
        Rc::try_unwrap(captured).unwrap().into_inner()
    }

    fn run_expect_runtime_error(source: &str) -> RuntimeError {
        let tokens = lex(source).expect("should lex");
        let program = parse(tokens).expect("should parse");
        crate::sema::analyze(&program).expect("should type-check");
        let interp = Interpreter::with_output(&program, |_| {});
        interp.run().expect_err("expected a runtime error")
    }

    #[test]
    fn evaluates_arithmetic() {
        let out = run_captured("void main() { print(2 + 3 * 4); }");
        assert_eq!(out, vec!["14"]);
    }

    #[test]
    fn evaluates_comparisons_and_equality() {
        let out = run_captured(
            r#"
            void main() {
                print(3 < 5);
                print(3 == 3);
                print(3 != 5);
            }
            "#,
        );
        assert_eq!(out, vec!["true", "true", "true"]);
    }

    #[test]
    fn short_circuits_and() {
        let out = run_captured(
            r#"
            bool sideEffect() { print("called"); return true; }
            void main() {
                if (false && sideEffect()) { }
                print("done");
            }
            "#,
        );
        assert_eq!(out, vec!["done"]);
    }

    #[test]
    fn short_circuits_or() {
        let out = run_captured(
            r#"
            bool sideEffect() { print("called"); return true; }
            void main() {
                if (true || sideEffect()) { }
                print("done");
            }
            "#,
        );
        assert_eq!(out, vec!["done"]);
    }

    #[test]
    fn evaluates_if_else_if_else() {
        let out = run_captured(
            r#"
            void main() {
                int x = 0;
                if (x > 0) { print("positive"); }
                else if (x == 0) { print("zero"); }
                else { print("negative"); }
            }
            "#,
        );
        assert_eq!(out, vec!["zero"]);
    }

    #[test]
    fn evaluates_while_loop() {
        let out = run_captured(
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
        assert_eq!(out, vec!["3", "2", "1"]);
    }

    #[test]
    fn evaluates_for_loop() {
        let out = run_captured(
            r#"
            void main() {
                int[] nums = [10, 20, 30];
                for (int i = 0; i < nums.length; i = i + 1) {
                    print(nums[i]);
                }
            }
            "#,
        );
        assert_eq!(out, vec!["10", "20", "30"]);
    }

    #[test]
    fn evaluates_recursion() {
        let out = run_captured(
            r#"
            int fib(int n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }
            void main() { print(fib(10)); }
            "#,
        );
        assert_eq!(out, vec!["55"]);
    }

    #[test]
    fn arrays_are_reference_semantics_across_calls() {
        let out = run_captured(
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
        assert_eq!(out, vec!["6"]);
    }

    #[test]
    fn array_creation_default_initializes_to_zero() {
        let out = run_captured(
            r#"
            void main() {
                int[] arr = new int[3];
                print(arr[0]);
                print(arr[1]);
                print(arr[2]);
            }
            "#,
        );
        assert_eq!(out, vec!["0", "0", "0"]);
    }

    #[test]
    fn errors_on_array_index_out_of_bounds() {
        let err = run_expect_runtime_error(
            r#"
            void main() {
                int[] arr = [1, 2, 3];
                print(arr[5]);
            }
            "#,
        );
        assert!(matches!(err, RuntimeError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn errors_on_division_by_zero() {
        let err = run_expect_runtime_error(
            r#"
            void main() {
                int x = 5;
                int y = 0;
                print(x / y);
            }
            "#,
        );
        assert_eq!(err, RuntimeError::DivisionByZero);
    }

    #[test]
    fn errors_on_modulo_by_zero() {
        let err = run_expect_runtime_error(
            r#"
            void main() {
                int x = 5;
                int y = 0;
                print(x % y);
            }
            "#,
        );
        assert_eq!(err, RuntimeError::DivisionByZero);
    }

    #[test]
    fn errors_on_negative_array_creation_size() {
        let err = run_expect_runtime_error(
            r#"
            void main() {
                int n = -1;
                int[] arr = new int[n];
            }
            "#,
        );
        assert!(matches!(err, RuntimeError::NegativeArraySize { .. }));
    }

    // ---- P1 Phase 9 (Operators) ----

    #[test]
    fn evaluates_ternary() {
        let out = run_captured(
            r#"
            void main() {
                int a = 5;
                int b = 3;
                print(a > b ? a : b);
                print(a > b ? b : a);
            }
            "#,
        );
        assert_eq!(out, vec!["5", "3"]);
    }

    #[test]
    fn ternary_only_evaluates_the_taken_branch() {
        let out = run_captured(
            r#"
            int loud(int v) { print("called"); return v; }
            void main() {
                int x = true ? 1 : loud(99);
                print(x);
            }
            "#,
        );
        assert_eq!(out, vec!["1"]);
    }

    #[test]
    fn evaluates_bitwise_and_shift_operators() {
        let out = run_captured(
            r#"
            void main() {
                print(5 & 3);
                print(5 | 2);
                print(5 ^ 1);
                print(~5);
                print(1 << 4);
                print(16 >> 2);
            }
            "#,
        );
        assert_eq!(out, vec!["1", "7", "4", "-6", "16", "4"]);
    }

    #[test]
    fn evaluates_compound_assignment_on_variable() {
        let out = run_captured(
            r#"
            void main() {
                int x = 10;
                x += 5;
                print(x);
                x -= 3;
                print(x);
                x *= 2;
                print(x);
                x /= 4;
                print(x);
                x %= 4;
                print(x);
                x &= 3;
                print(x);
                x |= 8;
                print(x);
                x ^= 1;
                print(x);
                x <<= 2;
                print(x);
                x >>= 1;
                print(x);
            }
            "#,
        );
        assert_eq!(
            out,
            vec!["15", "12", "24", "6", "2", "2", "10", "11", "44", "22"]
        );
    }

    #[test]
    fn compound_assignment_evaluates_array_index_expression_exactly_once() {
        // The exact hazard the Operators Plan calls out: naive desugaring
        // to `arr[f()] = arr[f()] + 1` would call f() twice.
        let out = run_captured(
            r#"
            int calls = 0;
            int nextIndex() {
                calls = calls + 1;
                return 0;
            }
            void main() {
                int[] arr = [10];
                arr[nextIndex()] += 5;
                print(arr[0]);
                print(calls);
            }
            "#,
        );
        assert_eq!(out, vec!["15", "1"]);
    }

    #[test]
    fn compound_assignment_returns_the_new_value() {
        let out = run_captured(
            r#"
            void main() {
                int x = 1;
                print(x += 4);
            }
            "#,
        );
        assert_eq!(out, vec!["5"]);
    }

    #[test]
    fn evaluates_unsigned_right_shift() {
        // -1 as i64 is all-1-bits; >>> 1 zero-fills the top bit instead of
        // sign-extending, giving i64::MAX. `>>` on the same input stays -1.
        let out = run_captured(
            r#"
            void main() {
                int x = -1;
                print(x >>> 1);
                print(x >> 1);
            }
            "#,
        );
        assert_eq!(out, vec![(i64::MAX).to_string(), "-1".to_string()]);
    }

    #[test]
    fn evaluates_prefix_increment_and_decrement() {
        let out = run_captured(
            r#"
            void main() {
                int x = 5;
                print(++x);
                print(x);
                print(--x);
                print(x);
            }
            "#,
        );
        assert_eq!(out, vec!["6", "6", "5", "5"]);
    }

    #[test]
    fn evaluates_postfix_increment_and_decrement() {
        let out = run_captured(
            r#"
            void main() {
                int x = 5;
                print(x++);
                print(x);
                print(x--);
                print(x);
            }
            "#,
        );
        assert_eq!(out, vec!["5", "6", "6", "5"]);
    }

    #[test]
    fn increment_on_array_index_evaluates_index_once() {
        // Same double-evaluation hazard as compound assignment.
        let out = run_captured(
            r#"
            int calls = 0;
            int nextIndex(int[] counter) {
                counter[0] = counter[0] + 1;
                return 0;
            }
            void main() {
                int[] arr = [10];
                int[] counter = [0];
                arr[nextIndex(counter)]++;
                print(arr[0]);
                print(counter[0]);
            }
            "#,
        );
        assert_eq!(out, vec!["11", "1"]);
    }

    #[test]
    fn increment_works_in_for_loop_update() {
        let out = run_captured(
            r#"
            void main() {
                for (int i = 0; i < 3; i++) {
                    print(i);
                }
            }
            "#,
        );
        assert_eq!(out, vec!["0", "1", "2"]);
    }

    /// Every P0 benchmark program (docs/ANX-Implementation-Plan-v1.md Phase 7)
    /// must produce the correct output when interpreted — this is Phase 4's
    /// exit gate. Expected values were computed by hand-tracing each
    /// algorithm (no compiled path exists yet to cross-check against).
    macro_rules! benchmark_produces_output {
        ($test_name:ident, $file:literal, [$($expected:literal),+ $(,)?]) => {
            #[test]
            fn $test_name() {
                let source = include_str!(concat!("../../tests/benchmarks/", $file));
                let output = run_captured(source);
                let expected: Vec<String> = vec![$($expected.to_string()),+];
                assert_eq!(output, expected, "{} produced unexpected output", $file);
            }
        };
    }

    benchmark_produces_output!(benchmark_01_binary_search, "01_binary_search.nx", ["3", "-1"]);
    benchmark_produces_output!(
        benchmark_02_binary_search_first_last,
        "02_binary_search_first_last.nx",
        ["1", "3"]
    );
    benchmark_produces_output!(
        benchmark_03_two_sum_sorted,
        "03_two_sum_sorted.nx",
        ["0", "1"]
    );
    benchmark_produces_output!(benchmark_04_reverse_array, "04_reverse_array.nx", ["5", "1"]);
    benchmark_produces_output!(
        benchmark_05_remove_duplicates,
        "05_remove_duplicates.nx",
        ["3", "1", "2", "3"]
    );
    benchmark_produces_output!(
        benchmark_06_bubble_sort,
        "06_bubble_sort.nx",
        ["1", "2", "3", "4", "5"]
    );
    benchmark_produces_output!(
        benchmark_07_insertion_sort,
        "07_insertion_sort.nx",
        ["1", "2", "3", "4", "5"]
    );
    benchmark_produces_output!(
        benchmark_08_selection_sort,
        "08_selection_sort.nx",
        ["1", "2", "3", "4", "5"]
    );
    benchmark_produces_output!(benchmark_09_merge_sort, "09_merge_sort.nx", ["1", "4", "9"]);
    benchmark_produces_output!(
        benchmark_10_quicksort,
        "10_quicksort.nx",
        ["1", "2", "3", "4", "5"]
    );
    benchmark_produces_output!(benchmark_11_factorial, "11_factorial.nx", ["120"]);
    benchmark_produces_output!(benchmark_12_fibonacci_naive, "12_fibonacci_naive.nx", ["55"]);
    benchmark_produces_output!(benchmark_13_fibonacci_memo, "13_fibonacci_memo.nx", ["55"]);
    benchmark_produces_output!(
        benchmark_14_fast_exponentiation,
        "14_fast_exponentiation.nx",
        ["1024"]
    );
    benchmark_produces_output!(benchmark_15_gcd, "15_gcd.nx", ["6"]);
    benchmark_produces_output!(benchmark_16_climbing_stairs, "16_climbing_stairs.nx", ["8"]);
    benchmark_produces_output!(benchmark_17_coin_change, "17_coin_change.nx", ["3"]);
    benchmark_produces_output!(benchmark_18_knapsack, "18_knapsack.nx", ["9"]);
    benchmark_produces_output!(
        benchmark_19_longest_increasing_subsequence,
        "19_longest_increasing_subsequence.nx",
        ["4"]
    );
    benchmark_produces_output!(benchmark_20_max_subarray, "20_max_subarray.nx", ["6"]);

    // ---- P2 (Strings) ----

    #[test]
    fn evaluates_string_length_charat_substring() {
        let out = run_captured(
            r#"
            void main() {
                string s = "hello";
                print(length(s));
                print(s.length);
                print(charAt(s, 1));
                print(substring(s, 1, 4));
            }
            "#,
        );
        assert_eq!(out, vec!["5", "5", "e", "ell"]);
    }

    #[test]
    fn evaluates_string_concat_and_equality() {
        let out = run_captured(
            r#"
            void main() {
                string a = "foo";
                string b = "bar";
                print(a + b);
                print(a == "foo");
                print(a != b);
                print(a + b == "foobar");
            }
            "#,
        );
        assert_eq!(out, vec!["foobar", "true", "true", "true"]);
    }

    #[test]
    fn errors_on_char_at_out_of_bounds() {
        let err = run_expect_runtime_error(
            r#"
            void main() {
                string s = "hi";
                print(charAt(s, 5));
            }
            "#,
        );
        assert!(matches!(err, RuntimeError::StringIndexOutOfBounds { .. }));
    }

    #[test]
    fn errors_on_substring_out_of_bounds() {
        let err = run_expect_runtime_error(
            r#"
            void main() {
                string s = "hi";
                print(substring(s, 0, 5));
            }
            "#,
        );
        assert!(matches!(err, RuntimeError::StringIndexOutOfBounds { .. }));
    }

    #[test]
    fn solves_is_palindrome_via_charat_loop() {
        let out = run_captured(
            r#"
            bool isPalindrome(string s) {
                int i = 0;
                int j = length(s) - 1;
                while (i < j) {
                    if (charAt(s, i) != charAt(s, j)) return false;
                    i = i + 1;
                    j = j - 1;
                }
                return true;
            }
            void main() {
                print(isPalindrome("racecar"));
                print(isPalindrome("hello"));
            }
            "#,
        );
        assert_eq!(out, vec!["true", "false"]);
    }

    #[test]
    fn solves_build_reversed_string_via_concat_loop() {
        let out = run_captured(
            r#"
            string reverse(string s) {
                string result = "";
                int i = length(s) - 1;
                while (i >= 0) {
                    result = result + charAt(s, i);
                    i = i - 1;
                }
                return result;
            }
            void main() {
                print(reverse("hello"));
            }
            "#,
        );
        assert_eq!(out, vec!["olleh"]);
    }

    #[test]
    fn solves_valid_anagram_via_letter_counting() {
        let out = run_captured(
            r#"
            int countChar(string s, string c) {
                int count = 0;
                int i = 0;
                while (i < length(s)) {
                    if (charAt(s, i) == c) count = count + 1;
                    i = i + 1;
                }
                return count;
            }
            bool isAnagram(string a, string b) {
                if (length(a) != length(b)) return false;
                string alphabet = "abcdefghijklmnopqrstuvwxyz";
                int k = 0;
                while (k < length(alphabet)) {
                    string c = charAt(alphabet, k);
                    if (countChar(a, c) != countChar(b, c)) return false;
                    k = k + 1;
                }
                return true;
            }
            void main() {
                print(isAnagram("cat", "act"));
                print(isAnagram("cat", "dog"));
            }
            "#,
        );
        assert_eq!(out, vec!["true", "false"]);
    }
}

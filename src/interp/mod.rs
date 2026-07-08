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
        match op {
            BinOp::Add => Ok(numeric_op(l, r, |a, b| a + b, |a, b| a + b)),
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
            BinOp::And | BinOp::Or => unreachable!("handled above"),
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
        self.call_function(callee, &arg_values)
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
            _ => unreachable!("sema guarantees only array.length field access"),
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
}

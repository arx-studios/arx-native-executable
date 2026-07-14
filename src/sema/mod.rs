pub mod symtab;
pub mod types;

use crate::ast::*;
use std::collections::HashMap;
use symtab::{FuncSig, SymbolId, SymbolTable};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum SemaError {
    #[error("{line}: undeclared identifier '{name}'")]
    UndeclaredIdentifier { name: String, line: usize },
    #[error("{line}: undeclared function '{name}'")]
    UndeclaredFunction { name: String, line: usize },
    #[error("{line}: '{name}' is already declared in this scope")]
    DuplicateDeclaration { name: String, line: usize },
    #[error("{line}: type mismatch: expected {expected}, found {found}")]
    TypeMismatch {
        expected: String,
        found: String,
        line: usize,
    },
    #[error("{line}: condition must be bool, found {found}")]
    NonBoolCondition { found: String, line: usize },
    #[error("{line}: cannot index into non-array type {found}")]
    NotIndexable { found: String, line: usize },
    #[error("{line}: '{name}' expects {expected} argument(s), found {found}")]
    ArityMismatch {
        name: String,
        expected: usize,
        found: usize,
        line: usize,
    },
    #[error("{line}: invalid assignment target")]
    InvalidAssignTarget { line: usize },
    #[error("{line}: not all code paths return a value")]
    MissingReturn { line: usize },
    #[error("{line}: type {name} has no field '{field}'")]
    UnknownField {
        name: String,
        field: String,
        line: usize,
    },
    #[error(
        "no top-level `void main()` found (either missing, or defined with the wrong signature)"
    )]
    MissingMain,
}

/// Side tables annotating the AST in place, per the shared-sema design in the
/// Implementation Plan: both the interpreter and codegen consume the same
/// tree plus these tables, rather than a second typed-AST.
#[derive(Debug, Default)]
pub struct SemaResult {
    pub types: HashMap<NodeId, Type>,
    pub var_refs: HashMap<NodeId, SymbolId>,
}

pub fn analyze(program: &Program) -> Result<SemaResult, Vec<SemaError>> {
    Checker::new().run(program)
}

// `FieldAccess` is deliberately excluded: `.length` is P0's only field, and
// it's a computed property (array length), not stored state — there's
// nothing for `arr.length = 5;` to actually assign to.
fn is_lvalue(e: &Expr) -> bool {
    matches!(e, Expr::Ident { .. } | Expr::Index { .. })
}

struct Checker {
    symtab: SymbolTable,
    errors: Vec<SemaError>,
    result: SemaResult,
}

impl Checker {
    fn new() -> Self {
        Checker {
            symtab: SymbolTable::new(),
            errors: Vec::new(),
            result: SemaResult::default(),
        }
    }

    /// Pre-registered before user-function hoisting so `length`/`charAt`/
    /// `substring` resolve through the ordinary `check_call` path (arity,
    /// arg-type checking) with no special-casing at call sites — see
    /// docs/P2/ANX-P2-Strings-Plan-v1.md §2.
    fn declare_builtin_funcs(&mut self) {
        self.symtab.declare_func(
            "length".to_string(),
            FuncSig {
                params: vec![Type::Str],
                return_ty: Type::Int,
            },
        );
        self.symtab.declare_func(
            "charAt".to_string(),
            FuncSig {
                params: vec![Type::Str, Type::Int],
                return_ty: Type::Str,
            },
        );
        self.symtab.declare_func(
            "substring".to_string(),
            FuncSig {
                params: vec![Type::Str, Type::Int, Type::Int],
                return_ty: Type::Str,
            },
        );
    }

    fn run(mut self, program: &Program) -> Result<SemaResult, Vec<SemaError>> {
        self.declare_builtin_funcs();

        // Pass 1: hoist function signatures so forward calls and recursion resolve.
        for decl in &program.decls {
            if let Decl::Func(f) = decl {
                if self.symtab.resolve_func(&f.name).is_some() {
                    self.errors.push(SemaError::DuplicateDeclaration {
                        name: f.name.clone(),
                        line: f.line,
                    });
                    continue;
                }
                self.symtab.declare_func(
                    f.name.clone(),
                    FuncSig {
                        params: f.params.iter().map(|p| p.ty.clone()).collect(),
                        return_ty: f.return_ty.clone(),
                    },
                );
            }
        }

        let has_valid_main = self
            .symtab
            .resolve_func("main")
            .is_some_and(|sig| sig.params.is_empty() && sig.return_ty == Type::Void);
        if !has_valid_main {
            self.errors.push(SemaError::MissingMain);
        }

        // Pass 2: walk bodies in source order (globals declared as encountered;
        // only functions are hoisted).
        for decl in &program.decls {
            match decl {
                Decl::Var(vd) => self.declare_local_var(vd),
                Decl::Func(f) => self.check_func(f),
            }
        }

        if self.errors.is_empty() {
            Ok(self.result)
        } else {
            Err(self.errors)
        }
    }

    fn check_func(&mut self, f: &FuncDecl) {
        self.symtab.push_scope();
        for p in &f.params {
            self.symtab.declare_var(&p.name, p.ty.clone());
        }
        let returns = self.check_block_stmts(&f.body.stmts, &f.return_ty);
        if f.return_ty != Type::Void && !returns {
            self.errors.push(SemaError::MissingReturn { line: f.line });
        }
        self.symtab.pop_scope();
    }

    fn declare_local_var(&mut self, vd: &VarDecl) {
        if self.symtab.is_declared_in_current_scope(&vd.name) {
            self.errors.push(SemaError::DuplicateDeclaration {
                name: vd.name.clone(),
                line: vd.line,
            });
        }
        if let Some(init) = &vd.init {
            self.check_expr_against(init, &vd.ty);
        }
        self.symtab.declare_var(&vd.name, vd.ty.clone());
    }

    /// Returns whether this statement unconditionally returns — used to flag
    /// non-void functions with a code path that falls off the end.
    fn check_stmt(&mut self, stmt: &Stmt, return_ty: &Type) -> bool {
        match stmt {
            Stmt::Expr(e) => {
                self.check_expr(e);
                false
            }
            Stmt::VarDecl(vd) => {
                self.declare_local_var(vd);
                false
            }
            Stmt::Block(b) => {
                self.symtab.push_scope();
                let returns = self.check_block_stmts(&b.stmts, return_ty);
                self.symtab.pop_scope();
                returns
            }
            Stmt::If(if_stmt) => {
                self.check_condition(&if_stmt.cond);
                let then_returns = self.check_scoped_stmt(&if_stmt.then_branch, return_ty);
                let else_returns = match &if_stmt.else_branch {
                    Some(e) => self.check_scoped_stmt(e, return_ty),
                    None => false,
                };
                then_returns && else_returns
            }
            Stmt::While(w) => {
                self.check_condition(&w.cond);
                self.check_scoped_stmt(&w.body, return_ty);
                false
            }
            Stmt::For(f) => {
                self.symtab.push_scope();
                self.check_stmt(&f.init, return_ty);
                self.check_condition(&f.cond);
                self.check_expr(&f.update);
                self.check_scoped_stmt(&f.body, return_ty);
                self.symtab.pop_scope();
                false
            }
            Stmt::Return(r) => {
                self.check_return(r, return_ty);
                true
            }
        }
    }

    /// Wraps a control-flow body (if/while/for branch) in its own scope
    /// unless it's already a `{ }` block, which scopes itself.
    fn check_scoped_stmt(&mut self, stmt: &Stmt, return_ty: &Type) -> bool {
        if matches!(stmt, Stmt::Block(_)) {
            self.check_stmt(stmt, return_ty)
        } else {
            self.symtab.push_scope();
            let returns = self.check_stmt(stmt, return_ty);
            self.symtab.pop_scope();
            returns
        }
    }

    fn check_block_stmts(&mut self, stmts: &[Stmt], return_ty: &Type) -> bool {
        let mut returns = false;
        for s in stmts {
            if self.check_stmt(s, return_ty) {
                returns = true;
            }
        }
        returns
    }

    fn check_condition(&mut self, cond: &Expr) {
        if let Some(ty) = self.check_expr(cond) {
            if ty != Type::Bool {
                self.errors.push(SemaError::NonBoolCondition {
                    found: format!("{ty:?}"),
                    line: cond.line(),
                });
            }
        }
    }

    fn check_return(&mut self, r: &ReturnStmt, return_ty: &Type) {
        match (&r.value, return_ty) {
            (None, Type::Void) => {}
            (None, other) => self.errors.push(SemaError::TypeMismatch {
                expected: format!("{other:?}"),
                found: "nothing".to_string(),
                line: r.line,
            }),
            (Some(v), Type::Void) => {
                self.check_expr(v);
                self.errors.push(SemaError::TypeMismatch {
                    expected: "Void".to_string(),
                    found: "a value".to_string(),
                    line: r.line,
                });
            }
            (Some(v), expected) => {
                self.check_expr_against(v, expected);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn check(source: &str) -> Result<SemaResult, Vec<SemaError>> {
        let tokens = lex(source).expect("source should lex cleanly");
        let program = parse(tokens).expect("source should parse cleanly");
        analyze(&program)
    }

    fn expect_ok(source: &str) {
        if let Err(errors) = check(source) {
            panic!("expected no sema errors, got: {errors:?}");
        }
    }

    fn expect_errors(source: &str) -> Vec<SemaError> {
        check(source).expect_err("expected sema errors")
    }

    #[test]
    fn valid_var_decl_and_main() {
        expect_ok("void main() { int x = 5; }");
    }

    #[test]
    fn valid_recursive_function() {
        expect_ok(
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
    fn valid_array_literal_index_and_length() {
        expect_ok(
            r#"
            void main() {
                int[] nums = [1, 2, 3];
                print(nums[0]);
                print(nums.length);
            }
            "#,
        );
    }

    #[test]
    fn valid_array_creation_with_runtime_size() {
        expect_ok(
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
    fn valid_for_and_while_loops() {
        expect_ok(
            r#"
            void main() {
                int[] nums = [1, 2, 3];
                for (int i = 0; i < nums.length; i = i + 1) {
                    print(nums[i]);
                }
                int x = 3;
                while (x > 0) {
                    x = x - 1;
                }
            }
            "#,
        );
    }

    #[test]
    fn errors_on_undeclared_identifier() {
        let errors = expect_errors("void main() { print(y); }");
        assert!(matches!(errors[0], SemaError::UndeclaredIdentifier { .. }));
    }

    #[test]
    fn errors_on_type_mismatch_in_var_decl() {
        let errors = expect_errors("void main() { int x = true; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_call_arity_mismatch() {
        let errors = expect_errors(
            r#"
            int add(int a, int b) { return a + b; }
            void main() { print(add(1)); }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::ArityMismatch { .. })));
    }

    #[test]
    fn errors_on_call_argument_type_mismatch() {
        let errors = expect_errors(
            r#"
            int add(int a, int b) { return a + b; }
            void main() { print(add(1, true)); }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_non_bool_if_condition() {
        let errors = expect_errors("void main() { if (5) { } }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::NonBoolCondition { .. })));
    }

    #[test]
    fn errors_on_non_int_array_index() {
        let errors = expect_errors(
            r#"
            void main() {
                int[] nums = [1, 2, 3];
                bool b = true;
                print(nums[b]);
            }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_indexing_non_array() {
        let errors = expect_errors(
            r#"
            void main() {
                int x = 5;
                print(x[0]);
            }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::NotIndexable { .. })));
    }

    #[test]
    fn errors_on_missing_main() {
        let errors = expect_errors("int add(int a, int b) { return a + b; }");
        assert!(errors.iter().any(|e| matches!(e, SemaError::MissingMain)));
    }

    #[test]
    fn errors_on_duplicate_declaration_in_same_scope() {
        let errors = expect_errors("void main() { int x = 1; int x = 2; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::DuplicateDeclaration { .. })));
    }

    #[test]
    fn errors_on_missing_return_in_non_void_function() {
        let errors = expect_errors(
            r#"
            int add(int a, int b) { }
            void main() { }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::MissingReturn { .. })));
    }

    #[test]
    fn errors_on_invalid_assignment_target() {
        let errors = expect_errors("void main() { int x = 1; 5 = x; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::InvalidAssignTarget { .. })));
    }

    #[test]
    fn errors_on_assigning_to_array_length() {
        // .length is computed, not stored — there's nothing to assign to.
        let errors = expect_errors(
            r#"
            void main() {
                int[] nums = [1, 2, 3];
                nums.length = 5;
            }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::InvalidAssignTarget { .. })));
    }

    #[test]
    fn errors_on_unknown_field_access() {
        let errors = expect_errors(
            r#"
            void main() {
                int x = 5;
                print(x.length);
            }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::UnknownField { .. })));
    }

    #[test]
    fn errors_collect_more_than_one_at_once() {
        // Sema must not stop at the first error, per the plan's "collect all
        // errors per pass" requirement — unlike the lexer/parser.
        let errors = expect_errors(
            r#"
            void main() {
                print(undeclared_one);
                print(undeclared_two);
            }
            "#,
        );
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn variable_scoping_is_block_local() {
        let errors = expect_errors(
            r#"
            void main() {
                if (true) {
                    int x = 1;
                }
                print(x);
            }
            "#,
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::UndeclaredIdentifier { .. })));
    }

    // ---- P1 Phase 9 (Operators): ternary, compound assignment, bitwise, shift ----

    #[test]
    fn valid_ternary_expression() {
        expect_ok("void main() { int x = true ? 1 : 2; }");
    }

    #[test]
    fn errors_on_non_bool_ternary_condition() {
        let errors = expect_errors("void main() { int x = 1 ? 1 : 2; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::NonBoolCondition { .. })));
    }

    #[test]
    fn errors_on_ternary_branch_type_mismatch() {
        let errors = expect_errors(r#"void main() { int x = true ? 1 : "a"; }"#);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_void_typed_ternary_branch() {
        // print() returns Void — a phi node can't be void-typed, so this
        // must be rejected here rather than discovered by codegen.
        let errors = expect_errors("void main() { true ? print(1) : print(2); }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn valid_compound_assignment_on_variable_and_array_index() {
        expect_ok(
            r#"
            void main() {
                int x = 1;
                x += 2;
                x -= 1;
                x *= 3;
                x /= 2;
                x %= 2;
                int[] arr = [1, 2, 3];
                arr[0] += 10;
            }
            "#,
        );
    }

    #[test]
    fn valid_bitwise_compound_assignment() {
        expect_ok(
            r#"
            void main() {
                int x = 5;
                x &= 3;
                x |= 8;
                x ^= 1;
                x <<= 2;
                x >>= 1;
            }
            "#,
        );
    }

    #[test]
    fn errors_on_compound_assign_type_mismatch() {
        let errors = expect_errors(r#"void main() { int x = 1; x += "a"; }"#);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_compound_assign_to_invalid_target() {
        let errors = expect_errors("void main() { 5 += 1; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::InvalidAssignTarget { .. })));
    }

    #[test]
    fn valid_bitwise_and_shift_binary_ops() {
        expect_ok(
            r#"
            void main() {
                int a = 5 & 3;
                int b = 5 | 3;
                int c = 5 ^ 3;
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
    fn errors_on_bitwise_op_with_non_int_operand() {
        let errors = expect_errors("void main() { bool b = true; int x = b & 1; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_bitwise_not_on_non_int() {
        let errors = expect_errors("void main() { bool b = true; int x = ~b; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn valid_unsigned_right_shift() {
        expect_ok("void main() { int x = -1 >>> 1; print(x); }");
    }

    #[test]
    fn valid_unsigned_right_shift_compound_assign() {
        expect_ok("void main() { int x = -1; x >>>= 1; print(x); }");
    }

    #[test]
    fn valid_prefix_and_postfix_inc_dec_on_variable() {
        expect_ok(
            r#"
            void main() {
                int x = 5;
                print(++x);
                print(x++);
                print(--x);
                print(x--);
            }
            "#,
        );
    }

    #[test]
    fn valid_inc_dec_on_array_index_and_float() {
        expect_ok(
            r#"
            void main() {
                int[] arr = [1, 2, 3];
                arr[0]++;
                ++arr[1];
                float f = 1.5;
                f++;
            }
            "#,
        );
    }

    #[test]
    fn errors_on_inc_dec_on_invalid_target() {
        let errors = expect_errors("void main() { 5++; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::InvalidAssignTarget { .. })));
    }

    #[test]
    fn errors_on_inc_dec_on_non_numeric_type() {
        let errors = expect_errors("void main() { bool b = true; b++; }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    /// Every P0 benchmark program (see docs/ANX-Implementation-Plan-v1.md
    /// Phase 7) must type-check cleanly now, even though nothing can execute
    /// them yet (Phase 3's exit gate) — this is the earliest point at which
    /// the P0 grammar/sema surface can be validated against real DSA solutions.
    macro_rules! benchmark_type_checks {
        ($test_name:ident, $file:literal) => {
            #[test]
            fn $test_name() {
                let source = include_str!(concat!("../../tests/benchmarks/", $file));
                if let Err(errors) = check(source) {
                    panic!("{} failed to type-check: {errors:?}", $file);
                }
            }
        };
    }

    benchmark_type_checks!(benchmark_01_binary_search, "01_binary_search.nx");
    benchmark_type_checks!(
        benchmark_02_binary_search_first_last,
        "02_binary_search_first_last.nx"
    );
    benchmark_type_checks!(benchmark_03_two_sum_sorted, "03_two_sum_sorted.nx");
    benchmark_type_checks!(benchmark_04_reverse_array, "04_reverse_array.nx");
    benchmark_type_checks!(benchmark_05_remove_duplicates, "05_remove_duplicates.nx");
    benchmark_type_checks!(benchmark_06_bubble_sort, "06_bubble_sort.nx");
    benchmark_type_checks!(benchmark_07_insertion_sort, "07_insertion_sort.nx");
    benchmark_type_checks!(benchmark_08_selection_sort, "08_selection_sort.nx");
    benchmark_type_checks!(benchmark_09_merge_sort, "09_merge_sort.nx");
    benchmark_type_checks!(benchmark_10_quicksort, "10_quicksort.nx");
    benchmark_type_checks!(benchmark_11_factorial, "11_factorial.nx");
    benchmark_type_checks!(benchmark_12_fibonacci_naive, "12_fibonacci_naive.nx");
    benchmark_type_checks!(benchmark_13_fibonacci_memo, "13_fibonacci_memo.nx");
    benchmark_type_checks!(
        benchmark_14_fast_exponentiation,
        "14_fast_exponentiation.nx"
    );
    benchmark_type_checks!(benchmark_15_gcd, "15_gcd.nx");
    benchmark_type_checks!(benchmark_16_climbing_stairs, "16_climbing_stairs.nx");
    benchmark_type_checks!(benchmark_17_coin_change, "17_coin_change.nx");
    benchmark_type_checks!(benchmark_18_knapsack, "18_knapsack.nx");
    benchmark_type_checks!(
        benchmark_19_longest_increasing_subsequence,
        "19_longest_increasing_subsequence.nx"
    );
    benchmark_type_checks!(benchmark_20_max_subarray, "20_max_subarray.nx");

    // ---- P2 (Strings) ----

    #[test]
    fn valid_string_builtins_and_operators() {
        expect_ok(
            r#"
            void main() {
                string s = "hello";
                int n = length(s);
                string c = charAt(s, 0);
                string sub = substring(s, 1, 3);
                string cat = s + " world";
                bool eq = s == "hello";
                bool neq = s != "world";
                print(n);
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
    fn valid_string_length_field_access() {
        expect_ok(
            r#"
            void main() {
                string s = "hello";
                print(s.length);
            }
            "#,
        );
    }

    #[test]
    fn errors_on_length_arity_mismatch() {
        let errors = expect_errors(r#"void main() { print(length("a", "b")); }"#);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::ArityMismatch { .. })));
    }

    #[test]
    fn errors_on_length_arg_type_mismatch() {
        let errors = expect_errors("void main() { print(length(5)); }");
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_char_at_arg_type_mismatch() {
        let errors = expect_errors(r#"void main() { print(charAt("hi", "x")); }"#);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_substring_arity_mismatch() {
        let errors = expect_errors(r#"void main() { print(substring("hi", 0)); }"#);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::ArityMismatch { .. })));
    }

    #[test]
    fn errors_on_string_plus_int() {
        let errors = expect_errors(r#"void main() { string s = "a" + 1; }"#);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }

    #[test]
    fn errors_on_string_int_equality_mismatch() {
        let errors = expect_errors(r#"void main() { bool b = "a" == 1; }"#);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SemaError::TypeMismatch { .. })));
    }
}

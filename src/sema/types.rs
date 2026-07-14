use super::{is_lvalue, Checker, SemaError};
use crate::ast::*;

impl Checker {
    pub(super) fn check_expr(&mut self, expr: &Expr) -> Option<Type> {
        let ty = match expr {
            Expr::IntLiteral { .. } => Some(Type::Int),
            Expr::FloatLiteral { .. } => Some(Type::Float),
            Expr::BoolLiteral { .. } => Some(Type::Bool),
            Expr::StringLiteral { .. } => Some(Type::Str),
            Expr::Null { .. } => Some(Type::Void),
            Expr::Ident { name, line, .. } => match self.symtab.resolve_var(name) {
                Some(sym) => {
                    self.result.var_refs.insert(expr.id(), sym.id);
                    Some(sym.ty.clone())
                }
                None => {
                    self.errors.push(SemaError::UndeclaredIdentifier {
                        name: name.clone(),
                        line: *line,
                    });
                    None
                }
            },
            Expr::Binary {
                op, left, right, line, ..
            } => self.check_binary(*op, left, right, *line),
            Expr::Unary { op, operand, line, .. } => self.check_unary(*op, operand, *line),
            Expr::Assign { target, value, line, .. } => self.check_assign(target, value, *line),
            Expr::CompoundAssign { op, target, value, line, .. } => {
                self.check_compound_assign(*op, target, value, *line)
            }
            Expr::IncDec { target, line, .. } => self.check_inc_dec(target, *line),
            Expr::Ternary { cond, then_branch, else_branch, line, .. } => {
                self.check_ternary(cond, then_branch, else_branch, *line)
            }
            Expr::Call { callee, args, line, .. } => self.check_call(callee, args, *line),
            Expr::Index { array, index, line, .. } => self.check_index(array, index, *line),
            Expr::FieldAccess { object, field, line, .. } => {
                self.check_field_access(object, field, *line)
            }
            Expr::ArrayLiteral { elements, line, .. } => {
                self.check_array_literal_inferred(elements, *line)
            }
            Expr::ArrayCreation { elem_ty, size, line, .. } => {
                self.check_array_creation(elem_ty, size, *line)
            }
        };
        if let Some(t) = &ty {
            self.result.types.insert(expr.id(), t.clone());
        }
        ty
    }

    /// Type-checks `expr` against a known `expected` type from context (a var
    /// decl's declared type, a call argument's parameter type, an assignment's
    /// target type). Needed as its own entry point — separate from bottom-up
    /// `check_expr` — so array literals (and eventually an empty `[]`) can be
    /// checked element-by-element against the expected element type instead of
    /// only inferring from their first element.
    pub(super) fn check_expr_against(&mut self, expr: &Expr, expected: &Type) -> Option<Type> {
        if let Expr::ArrayLiteral { elements, line, .. } = expr {
            let elem_ty = match expected {
                Type::Array(e) => (**e).clone(),
                _ => {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: format!("{expected:?}"),
                        found: "array literal".to_string(),
                        line: *line,
                    });
                    return None;
                }
            };
            let mut ok = true;
            for el in elements {
                if self.check_expr_against(el, &elem_ty).is_none() {
                    ok = false;
                }
            }
            let ty = Type::Array(Box::new(elem_ty));
            if ok {
                self.result.types.insert(expr.id(), ty.clone());
                Some(ty)
            } else {
                None
            }
        } else {
            let actual = self.check_expr(expr)?;
            if &actual == expected {
                Some(actual)
            } else {
                self.errors.push(SemaError::TypeMismatch {
                    expected: format!("{expected:?}"),
                    found: format!("{actual:?}"),
                    line: expr.line(),
                });
                None
            }
        }
    }

    fn check_binary(&mut self, op: BinOp, left: &Expr, right: &Expr, line: usize) -> Option<Type> {
        let lt = self.check_expr(left);
        let rt = self.check_expr(right);
        let (lt, rt) = (lt?, rt?);
        self.combine_binary_types(op, lt, rt, line)
    }

    /// The op-specific type rule shared by `check_binary` (both operands
    /// freshly checked) and `check_compound_assign` (target/value already
    /// known — see docs/P1/ANX-P1-Operators-Plan-v1.md §1 on why compound
    /// assignment isn't just parser-level `target = target op value` sugar).
    fn combine_binary_types(&mut self, op: BinOp, lt: Type, rt: Type, line: usize) -> Option<Type> {
        use BinOp::*;
        match op {
            Add | Sub | Mul | Div | Mod => {
                if lt == Type::Int && rt == Type::Int {
                    Some(Type::Int)
                } else if lt == Type::Float && rt == Type::Float {
                    Some(Type::Float)
                } else if op == Add && lt == Type::Str && rt == Type::Str {
                    Some(Type::Str)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: format!("{lt:?}"),
                        found: format!("{rt:?}"),
                        line,
                    });
                    None
                }
            }
            BitAnd | BitOr | BitXor | Shl | Shr | UShr => {
                if lt == Type::Int && rt == Type::Int {
                    Some(Type::Int)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Int".to_string(),
                        found: format!("{lt:?}/{rt:?}"),
                        line,
                    });
                    None
                }
            }
            Lt | LtEq | Gt | GtEq => {
                if (lt == Type::Int && rt == Type::Int) || (lt == Type::Float && rt == Type::Float)
                {
                    Some(Type::Bool)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: format!("{lt:?}"),
                        found: format!("{rt:?}"),
                        line,
                    });
                    None
                }
            }
            Eq | NotEq => {
                if lt == rt {
                    Some(Type::Bool)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: format!("{lt:?}"),
                        found: format!("{rt:?}"),
                        line,
                    });
                    None
                }
            }
            And | Or => {
                if lt == Type::Bool && rt == Type::Bool {
                    Some(Type::Bool)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Bool".to_string(),
                        found: format!("{lt:?}/{rt:?}"),
                        line,
                    });
                    None
                }
            }
        }
    }

    fn check_compound_assign(
        &mut self,
        op: BinOp,
        target: &Expr,
        value: &Expr,
        line: usize,
    ) -> Option<Type> {
        if !is_lvalue(target) {
            self.errors.push(SemaError::InvalidAssignTarget { line });
            self.check_expr(target);
            self.check_expr(value);
            return None;
        }
        let target_ty = self.check_expr(target)?;
        let value_ty = self.check_expr(value)?;
        let combined = self.combine_binary_types(op, target_ty.clone(), value_ty, line)?;
        if combined == target_ty {
            Some(combined)
        } else {
            self.errors.push(SemaError::TypeMismatch {
                expected: format!("{target_ty:?}"),
                found: format!("{combined:?}"),
                line,
            });
            None
        }
    }

    /// Shared by both prefix and postfix `++`/`--` — the static type is the
    /// same either way (only the *value* returned at runtime differs,
    /// which is an interp/codegen concern, not a sema one).
    fn check_inc_dec(&mut self, target: &Expr, line: usize) -> Option<Type> {
        if !is_lvalue(target) {
            self.errors.push(SemaError::InvalidAssignTarget { line });
            self.check_expr(target);
            return None;
        }
        let target_ty = self.check_expr(target)?;
        if target_ty == Type::Int || target_ty == Type::Float {
            Some(target_ty)
        } else {
            self.errors.push(SemaError::TypeMismatch {
                expected: "Int or Float".to_string(),
                found: format!("{target_ty:?}"),
                line,
            });
            None
        }
    }

    fn check_ternary(
        &mut self,
        cond: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
        line: usize,
    ) -> Option<Type> {
        if let Some(cond_ty) = self.check_expr(cond) {
            if cond_ty != Type::Bool {
                self.errors.push(SemaError::NonBoolCondition {
                    found: format!("{cond_ty:?}"),
                    line,
                });
            }
        }
        let then_ty = self.check_expr(then_branch);
        let else_ty = self.check_expr(else_branch);
        match (then_ty, else_ty) {
            // Void can't flow into an LLVM `phi` node (no such thing as a
            // void-typed value) — and there's no real use case for a
            // ternary of two void calls anyway (an if/else statement
            // already does that job). Reject it here rather than let
            // codegen discover it needs to special-case void.
            (Some(Type::Void), Some(_)) | (Some(_), Some(Type::Void)) => {
                self.errors.push(SemaError::TypeMismatch {
                    expected: "a non-void type in both branches".to_string(),
                    found: "Void".to_string(),
                    line,
                });
                None
            }
            (Some(t), Some(e)) if t == e => Some(t),
            (Some(t), Some(e)) => {
                self.errors.push(SemaError::TypeMismatch {
                    expected: format!("{t:?}"),
                    found: format!("{e:?}"),
                    line,
                });
                None
            }
            _ => None,
        }
    }

    fn check_unary(&mut self, op: UnOp, operand: &Expr, line: usize) -> Option<Type> {
        let ty = self.check_expr(operand)?;
        match op {
            UnOp::Neg => {
                if ty == Type::Int || ty == Type::Float {
                    Some(ty)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Int or Float".to_string(),
                        found: format!("{ty:?}"),
                        line,
                    });
                    None
                }
            }
            UnOp::Not => {
                if ty == Type::Bool {
                    Some(Type::Bool)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Bool".to_string(),
                        found: format!("{ty:?}"),
                        line,
                    });
                    None
                }
            }
            UnOp::BitNot => {
                if ty == Type::Int {
                    Some(Type::Int)
                } else {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Int".to_string(),
                        found: format!("{ty:?}"),
                        line,
                    });
                    None
                }
            }
        }
    }

    fn check_assign(&mut self, target: &Expr, value: &Expr, line: usize) -> Option<Type> {
        if !is_lvalue(target) {
            self.errors.push(SemaError::InvalidAssignTarget { line });
            self.check_expr(target);
            self.check_expr(value);
            return None;
        }
        let target_ty = self.check_expr(target)?;
        self.check_expr_against(value, &target_ty)
    }

    fn check_call(&mut self, callee: &str, args: &[Expr], line: usize) -> Option<Type> {
        if callee == "print" {
            if args.len() != 1 {
                self.errors.push(SemaError::ArityMismatch {
                    name: "print".to_string(),
                    expected: 1,
                    found: args.len(),
                    line,
                });
                for a in args {
                    self.check_expr(a);
                }
                return None;
            }
            self.check_expr(&args[0]);
            return Some(Type::Void);
        }

        let sig = match self.symtab.resolve_func(callee) {
            Some(s) => s.clone(),
            None => {
                self.errors.push(SemaError::UndeclaredFunction {
                    name: callee.to_string(),
                    line,
                });
                for a in args {
                    self.check_expr(a);
                }
                return None;
            }
        };
        if args.len() != sig.params.len() {
            self.errors.push(SemaError::ArityMismatch {
                name: callee.to_string(),
                expected: sig.params.len(),
                found: args.len(),
                line,
            });
            for a in args {
                self.check_expr(a);
            }
            return None;
        }
        let mut ok = true;
        for (arg, expected) in args.iter().zip(sig.params.iter()) {
            if self.check_expr_against(arg, expected).is_none() {
                ok = false;
            }
        }
        if ok {
            Some(sig.return_ty.clone())
        } else {
            None
        }
    }

    fn check_index(&mut self, array: &Expr, index: &Expr, line: usize) -> Option<Type> {
        let arr_ty = self.check_expr(array);
        let idx_ty = self.check_expr(index);
        let mut ok = true;

        let elem_ty = match &arr_ty {
            Some(Type::Array(e)) => Some((**e).clone()),
            Some(other) => {
                self.errors.push(SemaError::NotIndexable {
                    found: format!("{other:?}"),
                    line,
                });
                ok = false;
                None
            }
            None => {
                ok = false;
                None
            }
        };

        match &idx_ty {
            Some(Type::Int) => {}
            Some(other) => {
                self.errors.push(SemaError::TypeMismatch {
                    expected: "Int".to_string(),
                    found: format!("{other:?}"),
                    line,
                });
                ok = false;
            }
            None => ok = false,
        }

        if ok {
            elem_ty
        } else {
            None
        }
    }

    fn check_field_access(&mut self, object: &Expr, field: &str, line: usize) -> Option<Type> {
        let obj_ty = self.check_expr(object)?;
        match (&obj_ty, field) {
            (Type::Array(_), "length") | (Type::Str, "length") => Some(Type::Int),
            _ => {
                self.errors.push(SemaError::UnknownField {
                    name: format!("{obj_ty:?}"),
                    field: field.to_string(),
                    line,
                });
                None
            }
        }
    }

    fn check_array_literal_inferred(&mut self, elements: &[Expr], line: usize) -> Option<Type> {
        if elements.is_empty() {
            self.errors.push(SemaError::TypeMismatch {
                expected: "a known element type".to_string(),
                found: "empty array literal".to_string(),
                line,
            });
            return None;
        }
        let first_ty = self.check_expr(&elements[0])?;
        let mut ok = true;
        for el in &elements[1..] {
            if self.check_expr_against(el, &first_ty).is_none() {
                ok = false;
            }
        }
        if ok {
            Some(Type::Array(Box::new(first_ty)))
        } else {
            None
        }
    }

    fn check_array_creation(&mut self, elem_ty: &Type, size: &Expr, line: usize) -> Option<Type> {
        match self.check_expr(size) {
            Some(Type::Int) => Some(Type::Array(Box::new(elem_ty.clone()))),
            Some(other) => {
                self.errors.push(SemaError::TypeMismatch {
                    expected: "Int".to_string(),
                    found: format!("{other:?}"),
                    line,
                });
                None
            }
            None => None,
        }
    }
}

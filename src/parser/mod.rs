use crate::ast::*;
use crate::lexer::token::{Token, TokenKind};
use std::mem;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("{line}: expected {expected}, found {found}")]
    UnexpectedToken {
        expected: String,
        found: String,
        line: usize,
    },
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
    let mut parser = Parser::new(tokens);
    let mut decls = Vec::new();
    while !parser.is_at_end() {
        decls.push(parser.parse_decl()?);
    }
    Ok(Program { decls })
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    next_id: NodeId,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            next_id: 0,
        }
    }

    fn fresh_id(&mut self) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if !self.is_at_end() {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        mem::discriminant(self.peek_kind()) == mem::discriminant(kind)
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.check(&kind) {
            Ok(self.advance())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: kind.to_string(),
                found: self.peek_kind().to_string(),
                line: self.peek_line(),
            })
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: other.to_string(),
                line: self.peek_line(),
            }),
        }
    }

    // ---- types ----

    /// A single primitive type keyword, with no trailing `[]` — used both as
    /// the base of `parse_type` and standalone in `new type[expr]`, where the
    /// bracket belongs to the array-creation syntax, not a type suffix.
    fn parse_base_type(&mut self) -> Result<Type, ParseError> {
        let ty = match self.peek_kind() {
            TokenKind::Int => Type::Int,
            TokenKind::Float => Type::Float,
            TokenKind::Bool => Type::Bool,
            TokenKind::Str => Type::Str,
            TokenKind::Void => Type::Void,
            other => {
                return Err(ParseError::UnexpectedToken {
                    expected: "type".to_string(),
                    found: other.to_string(),
                    line: self.peek_line(),
                });
            }
        };
        self.advance();
        Ok(ty)
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let base = self.parse_base_type()?;
        if self.check(&TokenKind::LBracket) {
            self.advance();
            self.expect(TokenKind::RBracket)?;
            Ok(Type::Array(Box::new(base)))
        } else {
            Ok(base)
        }
    }

    // ---- top-level declarations ----

    fn parse_decl(&mut self) -> Result<Decl, ParseError> {
        let line = self.peek_line();
        let ty = self.parse_type()?;
        let name = self.expect_identifier()?;
        if self.check(&TokenKind::LParen) {
            Ok(Decl::Func(self.parse_func_decl_rest(ty, name, line)?))
        } else {
            Ok(Decl::Var(self.parse_var_decl_rest(ty, name, line)?))
        }
    }

    fn parse_func_decl_rest(
        &mut self,
        return_ty: Type,
        name: String,
        line: usize,
    ) -> Result<FuncDecl, ParseError> {
        self.expect(TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(TokenKind::RParen)?;
        let body = self.parse_block()?;
        Ok(FuncDecl {
            id: self.fresh_id(),
            name,
            params,
            return_ty,
            body,
            line,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let ty = self.parse_type()?;
                let name = self.expect_identifier()?;
                params.push(Param { ty, name });
                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        Ok(params)
    }

    fn parse_var_decl_rest(
        &mut self,
        ty: Type,
        name: String,
        line: usize,
    ) -> Result<VarDecl, ParseError> {
        let init = if self.check(&TokenKind::Eq) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Semicolon)?;
        Ok(VarDecl {
            id: self.fresh_id(),
            ty,
            name,
            init,
            line,
        })
    }

    fn parse_var_decl_stmt(&mut self) -> Result<Stmt, ParseError> {
        let line = self.peek_line();
        let ty = self.parse_type()?;
        let name = self.expect_identifier()?;
        Ok(Stmt::VarDecl(self.parse_var_decl_rest(ty, name, line)?))
    }

    // ---- statements ----

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Block { stmts })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek_kind() {
            TokenKind::LBrace => Ok(Stmt::Block(self.parse_block()?)),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Int | TokenKind::Float | TokenKind::Bool | TokenKind::Str | TokenKind::Void => {
                self.parse_var_decl_stmt()
            }
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let line = self.expect(TokenKind::If)?.line;
        self.expect(TokenKind::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = Box::new(self.parse_stmt()?);
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            Some(Box::new(self.parse_stmt()?))
        } else {
            None
        };
        Ok(Stmt::If(IfStmt {
            id: self.fresh_id(),
            cond,
            then_branch,
            else_branch,
            line,
        }))
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let line = self.expect(TokenKind::While)?.line;
        self.expect(TokenKind::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let body = Box::new(self.parse_stmt()?);
        Ok(Stmt::While(WhileStmt {
            id: self.fresh_id(),
            cond,
            body,
            line,
        }))
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let line = self.expect(TokenKind::For)?.line;
        self.expect(TokenKind::LParen)?;
        let init = Box::new(self.parse_var_decl_stmt()?);
        let cond = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        let update = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let body = Box::new(self.parse_stmt()?);
        Ok(Stmt::For(ForStmt {
            id: self.fresh_id(),
            init,
            cond,
            update,
            body,
            line,
        }))
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let line = self.expect(TokenKind::Return)?.line;
        let value = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Return(ReturnStmt {
            id: self.fresh_id(),
            value,
            line,
        }))
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Expr(expr))
    }

    // ---- expressions (precedence-climbing) ----
    // assignment (right-assoc) < ternary (right-assoc) < || < && < | < ^ < &
    // < ==,!= < <,<=,>,>= < <<,>> < +,- < *,/,% < unary !,-,~ < postfix [] () .
    // See docs/P1/ANX-P1-Operators-Plan-v1.md §2 for the full grammar.

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_assignment()
    }

    fn compound_assign_op(kind: &TokenKind) -> Option<BinOp> {
        match kind {
            TokenKind::PlusEq => Some(BinOp::Add),
            TokenKind::MinusEq => Some(BinOp::Sub),
            TokenKind::StarEq => Some(BinOp::Mul),
            TokenKind::SlashEq => Some(BinOp::Div),
            TokenKind::PercentEq => Some(BinOp::Mod),
            TokenKind::AmpEq => Some(BinOp::BitAnd),
            TokenKind::PipeEq => Some(BinOp::BitOr),
            TokenKind::CaretEq => Some(BinOp::BitXor),
            TokenKind::ShlEq => Some(BinOp::Shl),
            TokenKind::ShrEq => Some(BinOp::Shr),
            TokenKind::UShrEq => Some(BinOp::UShr),
            _ => None,
        }
    }

    fn parse_assignment(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_ternary()?;
        if self.check(&TokenKind::Eq) {
            let line = self.advance().line;
            let value = self.parse_assignment()?;
            return Ok(Expr::Assign {
                id: self.fresh_id(),
                target: Box::new(expr),
                value: Box::new(value),
                line,
            });
        }
        if let Some(op) = Self::compound_assign_op(self.peek_kind()) {
            let line = self.advance().line;
            let value = self.parse_assignment()?;
            return Ok(Expr::CompoundAssign {
                id: self.fresh_id(),
                op,
                target: Box::new(expr),
                value: Box::new(value),
                line,
            });
        }
        Ok(expr)
    }

    fn parse_ternary(&mut self) -> Result<Expr, ParseError> {
        let cond = self.parse_or()?;
        if self.check(&TokenKind::Question) {
            let line = self.advance().line;
            let then_branch = self.parse_expr()?;
            self.expect(TokenKind::Colon)?;
            let else_branch = self.parse_ternary()?;
            return Ok(Expr::Ternary {
                id: self.fresh_id(),
                cond: Box::new(cond),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
                line,
            });
        }
        Ok(cond)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_and()?;
        while self.check(&TokenKind::OrOr) {
            let line = self.advance().line;
            let right = self.parse_and()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op: BinOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_bit_or()?;
        while self.check(&TokenKind::AndAnd) {
            let line = self.advance().line;
            let right = self.parse_bit_or()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op: BinOp::And,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_bit_or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_bit_xor()?;
        while self.check(&TokenKind::Pipe) {
            let line = self.advance().line;
            let right = self.parse_bit_xor()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op: BinOp::BitOr,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_bit_xor(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_bit_and()?;
        while self.check(&TokenKind::Caret) {
            let line = self.advance().line;
            let right = self.parse_bit_and()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op: BinOp::BitXor,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_bit_and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_equality()?;
        while self.check(&TokenKind::Amp) {
            let line = self.advance().line;
            let right = self.parse_equality()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op: BinOp::BitAnd,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_comparison()?;
        loop {
            let op = if self.check(&TokenKind::EqEq) {
                BinOp::Eq
            } else if self.check(&TokenKind::NotEq) {
                BinOp::NotEq
            } else {
                break;
            };
            let line = self.advance().line;
            let right = self.parse_comparison()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_shift()?;
        loop {
            let op = if self.check(&TokenKind::Lt) {
                BinOp::Lt
            } else if self.check(&TokenKind::LtEq) {
                BinOp::LtEq
            } else if self.check(&TokenKind::Gt) {
                BinOp::Gt
            } else if self.check(&TokenKind::GtEq) {
                BinOp::GtEq
            } else {
                break;
            };
            let line = self.advance().line;
            let right = self.parse_shift()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_term()?;
        loop {
            let op = if self.check(&TokenKind::Shl) {
                BinOp::Shl
            } else if self.check(&TokenKind::Shr) {
                BinOp::Shr
            } else if self.check(&TokenKind::UShr) {
                BinOp::UShr
            } else {
                break;
            };
            let line = self.advance().line;
            let right = self.parse_term()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_factor()?;
        loop {
            let op = if self.check(&TokenKind::Plus) {
                BinOp::Add
            } else if self.check(&TokenKind::Minus) {
                BinOp::Sub
            } else {
                break;
            };
            let line = self.advance().line;
            let right = self.parse_factor()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = if self.check(&TokenKind::Star) {
                BinOp::Mul
            } else if self.check(&TokenKind::Slash) {
                BinOp::Div
            } else if self.check(&TokenKind::Percent) {
                BinOp::Mod
            } else {
                break;
            };
            let line = self.advance().line;
            let right = self.parse_unary()?;
            expr = Expr::Binary {
                id: self.fresh_id(),
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.check(&TokenKind::PlusPlus) || self.check(&TokenKind::MinusMinus) {
            let tok = self.advance();
            let op = if tok.kind == TokenKind::PlusPlus {
                IncDecOp::Inc
            } else {
                IncDecOp::Dec
            };
            // The operand of prefix ++/-- binds at unary precedence (so
            // `++arr[i]` targets the index expression, not just `arr`),
            // matching Java.
            let target = self.parse_unary()?;
            return Ok(Expr::IncDec {
                id: self.fresh_id(),
                op,
                target: Box::new(target),
                is_prefix: true,
                line: tok.line,
            });
        }
        if self.check(&TokenKind::Bang) || self.check(&TokenKind::Minus) || self.check(&TokenKind::Tilde) {
            let tok = self.advance();
            let op = match tok.kind {
                TokenKind::Bang => UnOp::Not,
                TokenKind::Tilde => UnOp::BitNot,
                _ => UnOp::Neg,
            };
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                id: self.fresh_id(),
                op,
                operand: Box::new(operand),
                line: tok.line,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&TokenKind::LBracket) {
                let line = self.advance().line;
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index {
                    id: self.fresh_id(),
                    array: Box::new(expr),
                    index: Box::new(index),
                    line,
                };
            } else if self.check(&TokenKind::Dot) {
                let line = self.advance().line;
                let field = self.expect_identifier()?;
                expr = Expr::FieldAccess {
                    id: self.fresh_id(),
                    object: Box::new(expr),
                    field,
                    line,
                };
            } else {
                break;
            }
        }
        // Postfix ++/-- binds once, after the full index/field-access
        // chain (e.g. `arr[i]++`) — not inside the loop above, since Java
        // disallows chaining (`x++++` is a parse error, not two decrements).
        if self.check(&TokenKind::PlusPlus) || self.check(&TokenKind::MinusMinus) {
            let tok = self.advance();
            let op = if tok.kind == TokenKind::PlusPlus {
                IncDecOp::Inc
            } else {
                IncDecOp::Dec
            };
            expr = Expr::IncDec {
                id: self.fresh_id(),
                op,
                target: Box::new(expr),
                is_prefix: false,
                line: tok.line,
            };
        }
        Ok(expr)
    }

    fn parse_array_creation(&mut self) -> Result<Expr, ParseError> {
        let line = self.expect(TokenKind::New)?.line;
        let elem_ty = self.parse_base_type()?;
        self.expect(TokenKind::LBracket)?;
        let size = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        Ok(Expr::ArrayCreation {
            id: self.fresh_id(),
            elem_ty,
            size: Box::new(size),
            line,
        })
    }

    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        let line = self.expect(TokenKind::LBracket)?.line;
        let mut elements = Vec::new();
        if !self.check(&TokenKind::RBracket) {
            loop {
                elements.push(self.parse_expr()?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RBracket)?;
        Ok(Expr::ArrayLiteral {
            id: self.fresh_id(),
            elements,
            line,
        })
    }

    fn parse_call_rest(&mut self, callee: String, line: usize) -> Result<Expr, ParseError> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(Expr::Call {
            id: self.fresh_id(),
            callee,
            args,
            line,
        })
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let line = self.peek_line();
        match self.peek_kind().clone() {
            TokenKind::IntLiteral(value) => {
                self.advance();
                Ok(Expr::IntLiteral {
                    id: self.fresh_id(),
                    value,
                    line,
                })
            }
            TokenKind::FloatLiteral(value) => {
                self.advance();
                Ok(Expr::FloatLiteral {
                    id: self.fresh_id(),
                    value,
                    line,
                })
            }
            TokenKind::StringLiteral(value) => {
                self.advance();
                Ok(Expr::StringLiteral {
                    id: self.fresh_id(),
                    value,
                    line,
                })
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::BoolLiteral {
                    id: self.fresh_id(),
                    value: true,
                    line,
                })
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::BoolLiteral {
                    id: self.fresh_id(),
                    value: false,
                    line,
                })
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Null {
                    id: self.fresh_id(),
                    line,
                })
            }
            TokenKind::New => self.parse_array_creation(),
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::Identifier(name) => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    self.parse_call_rest(name, line)
                } else {
                    Ok(Expr::Ident {
                        id: self.fresh_id(),
                        name,
                        line,
                    })
                }
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "expression".to_string(),
                found: other.to_string(),
                line,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_source(source: &str) -> Result<Program, ParseError> {
        parse(lex(source).expect("source should lex cleanly"))
    }

    fn parse_one_decl(source: &str) -> Decl {
        let mut program = parse_source(source).expect("source should parse cleanly");
        assert_eq!(program.decls.len(), 1, "expected exactly one declaration");
        program.decls.remove(0)
    }

    #[test]
    fn parses_var_decl_with_initializer() {
        let decl = parse_one_decl("int x = 5;");
        match decl {
            Decl::Var(v) => {
                assert_eq!(v.ty, Type::Int);
                assert_eq!(v.name, "x");
                assert_eq!(
                    v.init,
                    Some(Expr::IntLiteral {
                        id: v.init.as_ref().unwrap().id(),
                        value: 5,
                        line: 1
                    })
                );
            }
            other => panic!("expected VarDecl, got {other:?}"),
        }
    }

    #[test]
    fn parses_array_literal_decl() {
        let decl = parse_one_decl("int[] nums = [1, 2, 3, 4, 5];");
        match decl {
            Decl::Var(v) => {
                assert_eq!(v.ty, Type::Array(Box::new(Type::Int)));
                match v.init {
                    Some(Expr::ArrayLiteral { elements, .. }) => {
                        assert_eq!(elements.len(), 5);
                    }
                    other => panic!("expected ArrayLiteral, got {other:?}"),
                }
            }
            other => panic!("expected VarDecl, got {other:?}"),
        }
    }

    #[test]
    fn parses_array_creation_with_runtime_size() {
        let decl = parse_one_decl("int[] scratch = new int[n];");
        match decl {
            Decl::Var(v) => match v.init {
                Some(Expr::ArrayCreation { elem_ty, size, .. }) => {
                    assert_eq!(elem_ty, Type::Int);
                    assert!(matches!(*size, Expr::Ident { .. }));
                }
                other => panic!("expected ArrayCreation, got {other:?}"),
            },
            other => panic!("expected VarDecl, got {other:?}"),
        }
    }

    #[test]
    fn parses_function_with_recursion() {
        let decl = parse_one_decl(
            r#"
            int fib(int n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }
            "#,
        );
        match decl {
            Decl::Func(f) => {
                assert_eq!(f.name, "fib");
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].ty, Type::Int);
                assert_eq!(f.return_ty, Type::Int);
                assert_eq!(f.body.stmts.len(), 2);
            }
            other => panic!("expected FuncDecl, got {other:?}"),
        }
    }

    #[test]
    fn parses_if_else_if_else_chain() {
        let decl = parse_one_decl(
            r#"
            void main() {
                if (x > 0) {
                    print("positive");
                } else if (x == 0) {
                    print("zero");
                } else {
                    print("negative");
                }
            }
            "#,
        );
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        assert_eq!(f.body.stmts.len(), 1);
        let Stmt::If(outer) = &f.body.stmts[0] else {
            panic!("expected If statement")
        };
        // the "else if" is itself an If statement nested in the else branch
        assert!(matches!(
            outer.else_branch.as_deref(),
            Some(Stmt::If(_))
        ));
    }

    #[test]
    fn parses_while_loop() {
        let decl = parse_one_decl("void main() { while (x > 0) { x = x - 1; } }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        assert!(matches!(f.body.stmts[0], Stmt::While(_)));
    }

    #[test]
    fn parses_for_loop() {
        let decl = parse_one_decl(
            "void main() { for (int i = 0; i < nums.length; i = i + 1) { print(nums[i]); } }",
        );
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::For(for_stmt) = &f.body.stmts[0] else {
            panic!("expected For statement")
        };
        assert!(matches!(*for_stmt.init, Stmt::VarDecl(_)));
        assert!(matches!(for_stmt.cond, Expr::Binary { op: BinOp::Lt, .. }));
        assert!(matches!(
            for_stmt.update,
            Expr::Assign { .. }
        ));
    }

    #[test]
    fn parses_array_index_assignment_as_lvalue() {
        // Required for in-place sorting algorithms (bubble/insertion/merge/quicksort) —
        // the informal grammar sketch only shows IDENTIFIER as an assignment target,
        // but arr[j] = tmp; must also be a valid assignment, so the parser accepts any
        // postfix expression on the left of `=` and defers lvalue-validity to sema.
        let decl = parse_one_decl("void main() { arr[j] = tmp; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Assign { target, .. }) = &f.body.stmts[0] else {
            panic!("expected assignment expression statement")
        };
        assert!(matches!(**target, Expr::Index { .. }));
    }

    #[test]
    fn assignment_is_right_associative() {
        let decl = parse_one_decl("void main() { x = y = 5; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Assign { value, .. }) = &f.body.stmts[0] else {
            panic!("expected assignment expression statement")
        };
        assert!(matches!(**value, Expr::Assign { .. }));
    }

    #[test]
    fn parses_field_access() {
        let decl = parse_one_decl("void main() { print(arr.length); }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args, .. }) = &f.body.stmts[0] else {
            panic!("expected call expression statement")
        };
        assert!(matches!(args[0], Expr::FieldAccess { .. }));
    }

    #[test]
    fn index_binds_tighter_than_comparison() {
        // arr[mid] == target must parse as Binary(Index, ==, Ident), not
        // Index(Binary(...)) — confirms postfix precedence is above equality.
        let decl = parse_one_decl("void main() { if (arr[mid] == target) return mid; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::If(if_stmt) = &f.body.stmts[0] else {
            panic!("expected If statement")
        };
        let Expr::Binary { op, left, .. } = &if_stmt.cond else {
            panic!("expected Binary condition")
        };
        assert_eq!(*op, BinOp::Eq);
        assert!(matches!(**left, Expr::Index { .. }));
    }

    #[test]
    fn parses_ternary_expression() {
        let decl = parse_one_decl("void main() { int max = a > b ? a : b; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::VarDecl(vd) = &f.body.stmts[0] else {
            panic!("expected VarDecl")
        };
        let Some(Expr::Ternary { cond, .. }) = &vd.init else {
            panic!("expected Ternary init, got {:?}", vd.init)
        };
        assert!(matches!(**cond, Expr::Binary { op: BinOp::Gt, .. }));
    }

    #[test]
    fn ternary_is_right_associative_and_binds_looser_than_assignment() {
        // a ? b : c ? d : e  must parse as  a ? b : (c ? d : e)
        let decl = parse_one_decl("void main() { int x = a ? b : c ? d : e; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::VarDecl(vd) = &f.body.stmts[0] else {
            panic!("expected VarDecl")
        };
        let Some(Expr::Ternary { else_branch, .. }) = &vd.init else {
            panic!("expected Ternary init")
        };
        assert!(matches!(**else_branch, Expr::Ternary { .. }));
    }

    #[test]
    fn parses_compound_assignment_operators() {
        let cases: &[(&str, BinOp)] = &[
            ("x += 1;", BinOp::Add),
            ("x -= 1;", BinOp::Sub),
            ("x *= 1;", BinOp::Mul),
            ("x /= 1;", BinOp::Div),
            ("x %= 1;", BinOp::Mod),
            ("x &= 1;", BinOp::BitAnd),
            ("x |= 1;", BinOp::BitOr),
            ("x ^= 1;", BinOp::BitXor),
            ("x <<= 1;", BinOp::Shl),
            ("x >>= 1;", BinOp::Shr),
        ];
        for (src, expected_op) in cases {
            let decl = parse_one_decl(&format!("void main() {{ {src} }}"));
            let Decl::Func(f) = decl else {
                panic!("expected FuncDecl")
            };
            let Stmt::Expr(Expr::CompoundAssign { op, target, .. }) = &f.body.stmts[0] else {
                panic!("expected CompoundAssign statement for {src}")
            };
            assert_eq!(op, expected_op, "wrong op for {src}");
            assert!(matches!(**target, Expr::Ident { .. }));
        }
    }

    #[test]
    fn compound_assignment_works_on_array_index_target() {
        // The exact double-evaluation hazard case from the Operators Plan.
        let decl = parse_one_decl("void main() { arr[f()] += 1; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::CompoundAssign { target, .. }) = &f.body.stmts[0] else {
            panic!("expected CompoundAssign statement")
        };
        assert!(matches!(**target, Expr::Index { .. }));
    }

    #[test]
    fn parses_bitwise_and_shift_binary_operators() {
        let cases: &[(&str, BinOp)] = &[
            ("a & b", BinOp::BitAnd),
            ("a | b", BinOp::BitOr),
            ("a ^ b", BinOp::BitXor),
            ("a << b", BinOp::Shl),
            ("a >> b", BinOp::Shr),
        ];
        for (src, expected_op) in cases {
            let decl = parse_one_decl(&format!("void main() {{ print({src}); }}"));
            let Decl::Func(f) = decl else {
                panic!("expected FuncDecl")
            };
            let Stmt::Expr(Expr::Call { args, .. }) = &f.body.stmts[0] else {
                panic!("expected call statement")
            };
            assert!(
                matches!(&args[0], Expr::Binary { op, .. } if op == expected_op),
                "wrong op for {src}: {:?}",
                args[0]
            );
        }
    }

    #[test]
    fn parses_unary_bitwise_not() {
        let decl = parse_one_decl("void main() { print(~a); }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args, .. }) = &f.body.stmts[0] else {
            panic!("expected call statement")
        };
        assert!(matches!(
            &args[0],
            Expr::Unary { op: UnOp::BitNot, .. }
        ));
    }

    #[test]
    fn parses_unsigned_right_shift() {
        let decl = parse_one_decl("void main() { print(a >>> b); }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args, .. }) = &f.body.stmts[0] else {
            panic!("expected call statement")
        };
        assert!(matches!(
            &args[0],
            Expr::Binary { op: BinOp::UShr, .. }
        ));
    }

    #[test]
    fn parses_unsigned_right_shift_compound_assign() {
        let decl = parse_one_decl("void main() { x >>>= 1; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::CompoundAssign { op, .. }) = &f.body.stmts[0] else {
            panic!("expected CompoundAssign statement")
        };
        assert_eq!(*op, BinOp::UShr);
    }

    #[test]
    fn parses_prefix_increment_and_decrement() {
        let decl = parse_one_decl("void main() { print(++x); print(--y); }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args: args1, .. }) = &f.body.stmts[0] else {
            panic!("expected call statement")
        };
        assert!(matches!(
            &args1[0],
            Expr::IncDec { op: IncDecOp::Inc, is_prefix: true, .. }
        ));
        let Stmt::Expr(Expr::Call { args: args2, .. }) = &f.body.stmts[1] else {
            panic!("expected call statement")
        };
        assert!(matches!(
            &args2[0],
            Expr::IncDec { op: IncDecOp::Dec, is_prefix: true, .. }
        ));
    }

    #[test]
    fn parses_postfix_increment_and_decrement() {
        let decl = parse_one_decl("void main() { print(x++); print(y--); }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args: args1, .. }) = &f.body.stmts[0] else {
            panic!("expected call statement")
        };
        assert!(matches!(
            &args1[0],
            Expr::IncDec { op: IncDecOp::Inc, is_prefix: false, .. }
        ));
        let Stmt::Expr(Expr::Call { args: args2, .. }) = &f.body.stmts[1] else {
            panic!("expected call statement")
        };
        assert!(matches!(
            &args2[0],
            Expr::IncDec { op: IncDecOp::Dec, is_prefix: false, .. }
        ));
    }

    #[test]
    fn postfix_increment_applies_to_full_index_chain() {
        // arr[i]++ must target the Index expression, not just `arr`.
        let decl = parse_one_decl("void main() { arr[i]++; }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::IncDec { target, is_prefix, .. }) = &f.body.stmts[0] else {
            panic!("expected IncDec statement")
        };
        assert!(!is_prefix);
        assert!(matches!(**target, Expr::Index { .. }));
    }

    #[test]
    fn postfix_increment_does_not_chain() {
        // x++++ is not two decrements/increments — the second `++` has
        // nothing valid to attach to as a postfix op after the first, so it
        // should fail to parse (not silently accept it).
        assert!(parse_source("void main() { x++++; }").is_err());
    }

    #[test]
    fn bitwise_and_binds_tighter_than_bitwise_or() {
        // a | b & c must parse as a | (b & c)
        let decl = parse_one_decl("void main() { print(a | b & c); }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args, .. }) = &f.body.stmts[0] else {
            panic!("expected call statement")
        };
        let Expr::Binary { op, right, .. } = &args[0] else {
            panic!("expected top-level Binary")
        };
        assert_eq!(*op, BinOp::BitOr);
        assert!(matches!(**right, Expr::Binary { op: BinOp::BitAnd, .. }));
    }

    #[test]
    fn shift_binds_looser_than_additive_but_tighter_than_comparison() {
        // a << b + c must parse as a << (b + c)
        let decl = parse_one_decl("void main() { print(a << b + c); }");
        let Decl::Func(f) = decl else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args, .. }) = &f.body.stmts[0] else {
            panic!("expected call statement")
        };
        let Expr::Binary { op, right, .. } = &args[0] else {
            panic!("expected top-level Binary")
        };
        assert_eq!(*op, BinOp::Shl);
        assert!(matches!(**right, Expr::Binary { op: BinOp::Add, .. }));

        // a < b << c must parse as a < (b << c) — shift binds tighter than comparison
        let decl2 = parse_one_decl("void main() { print(a < b << c); }");
        let Decl::Func(f2) = decl2 else {
            panic!("expected FuncDecl")
        };
        let Stmt::Expr(Expr::Call { args: args2, .. }) = &f2.body.stmts[0] else {
            panic!("expected call statement")
        };
        let Expr::Binary { op: op2, right: right2, .. } = &args2[0] else {
            panic!("expected top-level Binary")
        };
        assert_eq!(*op2, BinOp::Lt);
        assert!(matches!(**right2, Expr::Binary { op: BinOp::Shl, .. }));
    }

    #[test]
    fn errors_on_missing_semicolon() {
        assert!(parse_source("int x = 5").is_err());
    }

    #[test]
    fn errors_report_line_number() {
        let err = parse_source("int x = 5\nint y = 10;").unwrap_err();
        match err {
            ParseError::UnexpectedToken { line, .. } => assert_eq!(line, 2),
        }
    }

    #[test]
    fn parses_variables_and_primitives_sample() {
        parse_source(
            r#"
            int x = 5;
            float pi = 3.14;
            bool found = false;
            string name = "arxcy";
            "#,
        )
        .expect("variables & primitives sample should parse cleanly");
    }

    #[test]
    fn parses_arrays_sample() {
        parse_source(
            r#"
            int[] nums = [1, 2, 3, 4, 5];
            void main() { print(nums[0]); }
            "#,
        )
        .expect("arrays sample should parse cleanly");
    }

    #[test]
    fn parses_functions_and_recursion_sample() {
        parse_source(
            r#"
            int add(int a, int b) {
                return a + b;
            }

            int fib(int n) {
                if (n <= 1) return n;
                return fib(n - 1) + fib(n - 2);
            }
            "#,
        )
        .expect("functions & recursion sample should parse cleanly");
    }

    #[test]
    fn parses_control_flow_sample() {
        parse_source(
            r#"
            void main() {
                if (x > 0) {
                    print("positive");
                } else if (x == 0) {
                    print("zero");
                } else {
                    print("negative");
                }

                for (int i = 0; i < nums.length; i = i + 1) {
                    print(nums[i]);
                }

                while (x > 0) {
                    x = x - 1;
                }
            }
            "#,
        )
        .expect("control flow sample should parse cleanly");
    }

    #[test]
    fn parses_binary_search_worked_example() {
        let program = parse_source(
            r#"
            int binarySearch(int[] arr, int target) {
                int lo = 0;
                int hi = arr.length - 1;
                while (lo <= hi) {
                    int mid = lo + (hi - lo) / 2;
                    if (arr[mid] == target) return mid;
                    else if (arr[mid] < target) lo = mid + 1;
                    else hi = mid - 1;
                }
                return -1;
            }
            "#,
        )
        .expect("binary search worked example should parse cleanly");
        assert_eq!(program.decls.len(), 1);
        let Decl::Func(f) = &program.decls[0] else {
            panic!("expected FuncDecl")
        };
        assert_eq!(f.name, "binarySearch");
        assert_eq!(f.params.len(), 2);
    }
}

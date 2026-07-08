pub type NodeId = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Array(Box<Type>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub decls: Vec<Decl>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Var(VarDecl),
    Func(FuncDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub id: NodeId,
    pub ty: Type,
    pub name: String,
    pub init: Option<Expr>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub ty: Type,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    pub id: NodeId,
    pub name: String,
    pub params: Vec<Param>,
    pub return_ty: Type,
    pub body: Block,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    VarDecl(VarDecl),
    If(IfStmt),
    While(WhileStmt),
    For(ForStmt),
    Return(ReturnStmt),
    Block(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub id: NodeId,
    pub cond: Expr,
    pub then_branch: Box<Stmt>,
    pub else_branch: Option<Box<Stmt>>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhileStmt {
    pub id: NodeId,
    pub cond: Expr,
    pub body: Box<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStmt {
    pub id: NodeId,
    pub init: Box<Stmt>,
    pub cond: Expr,
    pub update: Expr,
    pub body: Box<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStmt {
    pub id: NodeId,
    pub value: Option<Expr>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    IntLiteral {
        id: NodeId,
        value: i64,
        line: usize,
    },
    FloatLiteral {
        id: NodeId,
        value: f64,
        line: usize,
    },
    BoolLiteral {
        id: NodeId,
        value: bool,
        line: usize,
    },
    StringLiteral {
        id: NodeId,
        value: String,
        line: usize,
    },
    Null {
        id: NodeId,
        line: usize,
    },
    Ident {
        id: NodeId,
        name: String,
        line: usize,
    },
    Binary {
        id: NodeId,
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        line: usize,
    },
    Unary {
        id: NodeId,
        op: UnOp,
        operand: Box<Expr>,
        line: usize,
    },
    Assign {
        id: NodeId,
        target: Box<Expr>,
        value: Box<Expr>,
        line: usize,
    },
    Call {
        id: NodeId,
        callee: String,
        args: Vec<Expr>,
        line: usize,
    },
    Index {
        id: NodeId,
        array: Box<Expr>,
        index: Box<Expr>,
        line: usize,
    },
    FieldAccess {
        id: NodeId,
        object: Box<Expr>,
        field: String,
        line: usize,
    },
    ArrayLiteral {
        id: NodeId,
        elements: Vec<Expr>,
        line: usize,
    },
    ArrayCreation {
        id: NodeId,
        elem_ty: Type,
        size: Box<Expr>,
        line: usize,
    },
}

impl Expr {
    pub fn id(&self) -> NodeId {
        match self {
            Expr::IntLiteral { id, .. }
            | Expr::FloatLiteral { id, .. }
            | Expr::BoolLiteral { id, .. }
            | Expr::StringLiteral { id, .. }
            | Expr::Null { id, .. }
            | Expr::Ident { id, .. }
            | Expr::Binary { id, .. }
            | Expr::Unary { id, .. }
            | Expr::Assign { id, .. }
            | Expr::Call { id, .. }
            | Expr::Index { id, .. }
            | Expr::FieldAccess { id, .. }
            | Expr::ArrayLiteral { id, .. }
            | Expr::ArrayCreation { id, .. } => *id,
        }
    }

    pub fn line(&self) -> usize {
        match self {
            Expr::IntLiteral { line, .. }
            | Expr::FloatLiteral { line, .. }
            | Expr::BoolLiteral { line, .. }
            | Expr::StringLiteral { line, .. }
            | Expr::Null { line, .. }
            | Expr::Ident { line, .. }
            | Expr::Binary { line, .. }
            | Expr::Unary { line, .. }
            | Expr::Assign { line, .. }
            | Expr::Call { line, .. }
            | Expr::Index { line, .. }
            | Expr::FieldAccess { line, .. }
            | Expr::ArrayLiteral { line, .. }
            | Expr::ArrayCreation { line, .. } => *line,
        }
    }
}

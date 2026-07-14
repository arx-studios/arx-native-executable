use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    Identifier(String),

    // Keywords — primitive types
    Int,
    Float,
    Bool,
    Str,
    Void,

    // Keywords — control flow / declarations
    If,
    Else,
    While,
    For,
    Return,
    True,
    False,
    Null,
    New,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    Bang,
    Eq,

    // Bitwise / shift (P1 — ANX-P1-Operators-Plan-v1.md)
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    /// `>>>` — unsigned/logical right shift (zero-fills instead of
    /// sign-extending). Added alongside `++`/`--`, after the initial P1
    /// Operators pass.
    UShr,

    // Ternary (P1)
    Question,
    Colon,

    // Compound assignment (P1)
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    UShrEq,

    // Increment/decrement
    PlusPlus,
    MinusMinus,

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Dot,

    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, col: usize) -> Self {
        Token { kind, line, col }
    }
}

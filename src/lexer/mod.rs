pub mod token;

use std::str::Chars;
use thiserror::Error;
use token::{Token, TokenKind};

#[derive(Debug, Error, PartialEq)]
pub enum LexError {
    #[error("{line}:{col}: unexpected character '{ch}'")]
    UnexpectedChar { ch: char, line: usize, col: usize },
    #[error("{line}: unterminated string literal")]
    UnterminatedString { line: usize },
    #[error("{line}: unterminated block comment")]
    UnterminatedBlockComment { line: usize },
    #[error("{line}:{col}: invalid number literal '{lexeme}'")]
    InvalidNumber {
        lexeme: String,
        line: usize,
        col: usize,
    },
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).tokenize()
}

struct Lexer<'a> {
    chars: std::iter::Peekable<Chars<'a>>,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Lexer {
            chars: source.chars().peekable(),
            line: 1,
            col: 1,
        }
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.next();
        if let Some(ch) = c {
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        c
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn peek_is(&mut self, expected: char) -> bool {
        self.peek() == Some(expected)
    }

    /// Peeks the character after the current one without consuming anything.
    fn peek_next(&self) -> Option<char> {
        let mut lookahead = self.chars.clone();
        lookahead.next();
        lookahead.next()
    }

    fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments()?;
            let (line, col) = (self.line, self.col);

            let Some(ch) = self.peek() else {
                tokens.push(Token::new(TokenKind::Eof, line, col));
                break;
            };

            let kind = if ch.is_ascii_digit() {
                self.lex_number(line, col)?
            } else if ch == '"' {
                self.lex_string(line)?
            } else if ch.is_alphabetic() || ch == '_' {
                self.lex_identifier_or_keyword()
            } else {
                self.lex_operator_or_punct(line, col)?
            };

            tokens.push(Token::new(kind, line, col));
        }
        Ok(tokens)
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                }
                Some('/') if self.peek_next() == Some('/') => {
                    self.advance();
                    self.advance();
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some('/') if self.peek_next() == Some('*') => {
                    let start_line = self.line;
                    self.advance();
                    self.advance();
                    let mut closed = false;
                    while let Some(c) = self.advance() {
                        if c == '*' && self.peek_is('/') {
                            self.advance();
                            closed = true;
                            break;
                        }
                    }
                    if !closed {
                        return Err(LexError::UnterminatedBlockComment { line: start_line });
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn lex_number(&mut self, line: usize, col: usize) -> Result<TokenKind, LexError> {
        let mut lexeme = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                lexeme.push(c);
                self.advance();
            } else {
                break;
            }
        }

        let mut is_float = false;
        if self.peek_is('.') && self.peek_next().is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            lexeme.push('.');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    lexeme.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        if is_float {
            lexeme
                .parse::<f64>()
                .map(TokenKind::FloatLiteral)
                .map_err(|_| LexError::InvalidNumber { lexeme, line, col })
        } else {
            lexeme
                .parse::<i64>()
                .map(TokenKind::IntLiteral)
                .map_err(|_| LexError::InvalidNumber { lexeme, line, col })
        }
    }

    fn lex_string(&mut self, line: usize) -> Result<TokenKind, LexError> {
        self.advance(); // opening quote
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('"') => return Ok(TokenKind::StringLiteral(s)),
                Some('\\') => match self.advance() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some(c) => s.push(c),
                    None => return Err(LexError::UnterminatedString { line }),
                },
                Some(c) => s.push(c),
                None => return Err(LexError::UnterminatedString { line }),
            }
        }
    }

    fn lex_identifier_or_keyword(&mut self) -> TokenKind {
        let mut ident = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                ident.push(c);
                self.advance();
            } else {
                break;
            }
        }

        match ident.as_str() {
            "int" => TokenKind::Int,
            "float" => TokenKind::Float,
            "bool" => TokenKind::Bool,
            "string" => TokenKind::Str,
            "void" => TokenKind::Void,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "return" => TokenKind::Return,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "new" => TokenKind::New,
            _ => TokenKind::Identifier(ident),
        }
    }

    fn lex_operator_or_punct(&mut self, line: usize, col: usize) -> Result<TokenKind, LexError> {
        let c = self.advance().unwrap();
        let kind = match c {
            '+' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::PlusEq
                } else if self.peek_is('+') {
                    self.advance();
                    TokenKind::PlusPlus
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::MinusEq
                } else if self.peek_is('-') {
                    self.advance();
                    TokenKind::MinusMinus
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            '=' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::EqEq
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::NotEq
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                if self.peek_is('<') {
                    if self.peek_next() == Some('=') {
                        self.advance();
                        self.advance();
                        TokenKind::ShlEq
                    } else {
                        self.advance();
                        TokenKind::Shl
                    }
                } else if self.peek_is('=') {
                    self.advance();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek_is('>') {
                    if self.peek_next() == Some('>') {
                        self.advance(); // consume 2nd '>'
                        self.advance(); // consume 3rd '>'
                        if self.peek_is('=') {
                            self.advance();
                            TokenKind::UShrEq
                        } else {
                            TokenKind::UShr
                        }
                    } else if self.peek_next() == Some('=') {
                        self.advance();
                        self.advance();
                        TokenKind::ShrEq
                    } else {
                        self.advance();
                        TokenKind::Shr
                    }
                } else if self.peek_is('=') {
                    self.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '&' => {
                if self.peek_is('&') {
                    self.advance();
                    TokenKind::AndAnd
                } else if self.peek_is('=') {
                    self.advance();
                    TokenKind::AmpEq
                } else {
                    TokenKind::Amp
                }
            }
            '|' => {
                if self.peek_is('|') {
                    self.advance();
                    TokenKind::OrOr
                } else if self.peek_is('=') {
                    self.advance();
                    TokenKind::PipeEq
                } else {
                    TokenKind::Pipe
                }
            }
            '^' => {
                if self.peek_is('=') {
                    self.advance();
                    TokenKind::CaretEq
                } else {
                    TokenKind::Caret
                }
            }
            '~' => TokenKind::Tilde,
            '?' => TokenKind::Question,
            ':' => TokenKind::Colon,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '.' => TokenKind::Dot,
            _ => return Err(LexError::UnexpectedChar { ch: c, line, col }),
        };
        Ok(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn lexes_int_literal() {
        assert_eq!(kinds("42"), vec![TokenKind::IntLiteral(42), TokenKind::Eof]);
    }

    #[test]
    fn lexes_float_literal() {
        assert_eq!(
            kinds("3.14"),
            vec![TokenKind::FloatLiteral(3.14), TokenKind::Eof]
        );
    }

    #[test]
    fn lexes_bool_literals() {
        assert_eq!(
            kinds("true false"),
            vec![TokenKind::True, TokenKind::False, TokenKind::Eof]
        );
    }

    #[test]
    fn lexes_string_literal() {
        assert_eq!(
            kinds(r#""arxcy""#),
            vec![
                TokenKind::StringLiteral("arxcy".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn lexes_string_with_escapes() {
        assert_eq!(
            kinds(r#""line\nbreak""#),
            vec![
                TokenKind::StringLiteral("line\nbreak".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn lexes_identifier() {
        assert_eq!(
            kinds("nums_2"),
            vec![
                TokenKind::Identifier("nums_2".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn lexes_all_keywords() {
        assert_eq!(
            kinds("int float bool string void if else while for return true false null new"),
            vec![
                TokenKind::Int,
                TokenKind::Float,
                TokenKind::Bool,
                TokenKind::Str,
                TokenKind::Void,
                TokenKind::If,
                TokenKind::Else,
                TokenKind::While,
                TokenKind::For,
                TokenKind::Return,
                TokenKind::True,
                TokenKind::False,
                TokenKind::Null,
                TokenKind::New,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_operators() {
        assert_eq!(
            kinds("+ - * / % == != < <= > >= && || ! ="),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::EqEq,
                TokenKind::NotEq,
                TokenKind::Lt,
                TokenKind::LtEq,
                TokenKind::Gt,
                TokenKind::GtEq,
                TokenKind::AndAnd,
                TokenKind::OrOr,
                TokenKind::Bang,
                TokenKind::Eq,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_bitwise_and_shift_operators() {
        assert_eq!(
            kinds("& | ^ ~ << >>"),
            vec![
                TokenKind::Amp,
                TokenKind::Pipe,
                TokenKind::Caret,
                TokenKind::Tilde,
                TokenKind::Shl,
                TokenKind::Shr,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_ternary_operator() {
        assert_eq!(
            kinds("? :"),
            vec![TokenKind::Question, TokenKind::Colon, TokenKind::Eof]
        );
    }

    #[test]
    fn lexes_compound_assignment_operators() {
        assert_eq!(
            kinds("+= -= *= /= %= &= |= ^= <<= >>="),
            vec![
                TokenKind::PlusEq,
                TokenKind::MinusEq,
                TokenKind::StarEq,
                TokenKind::SlashEq,
                TokenKind::PercentEq,
                TokenKind::AmpEq,
                TokenKind::PipeEq,
                TokenKind::CaretEq,
                TokenKind::ShlEq,
                TokenKind::ShrEq,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_unsigned_shift_and_increment_decrement() {
        assert_eq!(
            kinds(">>> >>>= ++ --"),
            vec![
                TokenKind::UShr,
                TokenKind::UShrEq,
                TokenKind::PlusPlus,
                TokenKind::MinusMinus,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn distinguishes_ushr_from_shr_and_shr_eq() {
        assert_eq!(kinds("a >> b"), vec![
            TokenKind::Identifier("a".to_string()),
            TokenKind::Shr,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ]);
        assert_eq!(kinds("a >>= b"), vec![
            TokenKind::Identifier("a".to_string()),
            TokenKind::ShrEq,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ]);
        assert_eq!(kinds("a >>> b"), vec![
            TokenKind::Identifier("a".to_string()),
            TokenKind::UShr,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ]);
        assert_eq!(kinds("a >>>= b"), vec![
            TokenKind::Identifier("a".to_string()),
            TokenKind::UShrEq,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn distinguishes_increment_from_plus_and_plus_eq() {
        assert_eq!(kinds("a + b"), vec![
            TokenKind::Identifier("a".to_string()),
            TokenKind::Plus,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ]);
        assert_eq!(kinds("a += b"), vec![
            TokenKind::Identifier("a".to_string()),
            TokenKind::PlusEq,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ]);
        assert_eq!(kinds("a++"), vec![
            TokenKind::Identifier("a".to_string()),
            TokenKind::PlusPlus,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn distinguishes_shift_from_comparison_and_shift_assign() {
        assert_eq!(
            kinds("a < b"),
            vec![
                TokenKind::Identifier("a".to_string()),
                TokenKind::Lt,
                TokenKind::Identifier("b".to_string()),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("a << b"),
            vec![
                TokenKind::Identifier("a".to_string()),
                TokenKind::Shl,
                TokenKind::Identifier("b".to_string()),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("a <<= b"),
            vec![
                TokenKind::Identifier("a".to_string()),
                TokenKind::ShlEq,
                TokenKind::Identifier("b".to_string()),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("a <= b"),
            vec![
                TokenKind::Identifier("a".to_string()),
                TokenKind::LtEq,
                TokenKind::Identifier("b".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn lexes_punctuation() {
        assert_eq!(
            kinds("( ) { } [ ] , ; ."),
            vec![
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::Comma,
                TokenKind::Semicolon,
                TokenKind::Dot,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn skips_line_comments() {
        assert_eq!(
            kinds("1 // this is a comment\n2"),
            vec![
                TokenKind::IntLiteral(1),
                TokenKind::IntLiteral(2),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn skips_block_comments() {
        assert_eq!(
            kinds("1 /* block\ncomment */ 2"),
            vec![
                TokenKind::IntLiteral(1),
                TokenKind::IntLiteral(2),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn division_is_not_mistaken_for_a_comment() {
        assert_eq!(
            kinds("10 / 2"),
            vec![
                TokenKind::IntLiteral(10),
                TokenKind::Slash,
                TokenKind::IntLiteral(2),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn errors_on_unterminated_string() {
        assert_eq!(
            lex("\"unterminated"),
            Err(LexError::UnterminatedString { line: 1 })
        );
    }

    #[test]
    fn errors_on_unterminated_block_comment() {
        assert_eq!(
            lex("/* never closed"),
            Err(LexError::UnterminatedBlockComment { line: 1 })
        );
    }

    #[test]
    fn errors_on_unexpected_character() {
        assert_eq!(
            lex("@"),
            Err(LexError::UnexpectedChar {
                ch: '@',
                line: 1,
                col: 1
            })
        );
    }

    #[test]
    fn tracks_line_and_column() {
        let tokens = lex("int x\n= 5;").unwrap();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].col, 1);
        assert_eq!(tokens[1].line, 1);
        assert_eq!(tokens[1].col, 5);
        // '=' is the first token on line 2
        assert_eq!(tokens[2].line, 2);
        assert_eq!(tokens[2].col, 1);
    }

    #[test]
    fn lexes_binary_search_worked_example() {
        let source = r#"
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
        "#;
        let tokens = lex(source).expect("worked example should lex cleanly");
        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::While));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Return));
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Identifier("binarySearch".to_string()))
        );
    }

    #[test]
    fn lexes_control_flow_snippet() {
        let source = r#"
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
        "#;
        let tokens = lex(source).expect("control flow snippet should lex cleanly");
        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::For));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Else));
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::StringLiteral("negative".to_string()))
        );
    }
}

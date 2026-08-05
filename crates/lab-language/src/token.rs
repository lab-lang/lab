use std::fmt;

use crate::source::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Identifier(String),
    String(String),
    Integer(u64),
    Decimal(String),
    Newline,
    Indent,
    Dedent,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Colon,
    Dot,
    DotDot,
    Equal,
    LeftArrow,
    RightArrow,
    Pipe,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Plus,
    Minus,
    Star,
    Slash,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(identifier) => write!(f, "'{identifier}'"),
            Self::String(_) => f.write_str("a string"),
            Self::Integer(integer) => write!(f, "'{integer}'"),
            Self::Decimal(decimal) => write!(f, "'{decimal}'"),
            Self::Newline => f.write_str("a newline"),
            Self::Indent => f.write_str("an indented block"),
            Self::Dedent => f.write_str("the end of an indented block"),
            Self::LeftParen => f.write_str("'('"),
            Self::RightParen => f.write_str("')'"),
            Self::LeftBracket => f.write_str("'['"),
            Self::RightBracket => f.write_str("']'"),
            Self::LeftBrace => f.write_str("'{'"),
            Self::RightBrace => f.write_str("'}'"),
            Self::Comma => f.write_str("','"),
            Self::Colon => f.write_str("':'"),
            Self::Dot => f.write_str("'.'"),
            Self::DotDot => f.write_str("'..'"),
            Self::Equal => f.write_str("'='"),
            Self::LeftArrow => f.write_str("'<-'"),
            Self::RightArrow => f.write_str("'->'"),
            Self::Pipe => f.write_str("'|'"),
            Self::EqualEqual => f.write_str("'=='"),
            Self::NotEqual => f.write_str("'!='"),
            Self::Less => f.write_str("'<'"),
            Self::LessEqual => f.write_str("'<='"),
            Self::Greater => f.write_str("'>'"),
            Self::GreaterEqual => f.write_str("'>='"),
            Self::Plus => f.write_str("'+'"),
            Self::Minus => f.write_str("'-'"),
            Self::Star => f.write_str("'*'"),
            Self::Slash => f.write_str("'/'"),
        }
    }
}

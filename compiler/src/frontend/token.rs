use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Word(String),
    String(String),
    Number(u64),
    LeftBrace,
    RightBrace,
    Semicolon,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word(word) => write!(f, "'{word}'"),
            Self::String(_) => f.write_str("a string"),
            Self::Number(number) => write!(f, "'{number}'"),
            Self::LeftBrace => f.write_str("'{'"),
            Self::RightBrace => f.write_str("'}'"),
            Self::Semicolon => f.write_str("';'"),
        }
    }
}

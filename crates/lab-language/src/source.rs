use serde::{Deserialize, Serialize};

/// Half-open byte range in one Lab source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn at(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    pub const fn join(self, other: Self) -> Self {
        Self::new(self.start, other.end)
    }

    /// Whether an offset falls in this span, including either edge: a cursor
    /// resting against a name is on it.
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }
}

/// A value paired with the source range that produced it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub const fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }
}

pub type Identifier = Spanned<String>;

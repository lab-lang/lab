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

/// Line boundaries of one source file, for turning byte spans into the
/// line-and-column positions people and editors work in.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset + 1),
        );
        Self { starts }
    }

    /// The zero-based line containing `offset`.
    pub fn line(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        }
    }

    /// The zero-based line and character column of `offset`. Columns count
    /// characters rather than bytes, so a caret lands under the character an
    /// author sees rather than under a byte in the middle of one.
    pub fn position(&self, source: &str, offset: usize) -> (usize, usize) {
        let offset = char_boundary(source, offset);
        let line = self.line(offset);
        let column = source[self.starts[line]..offset].chars().count();
        (line, column)
    }

    /// The offset a line begins at.
    pub fn start(&self, line: usize) -> usize {
        self.starts[line]
    }

    /// The text of a line without its terminator.
    pub fn line_text<'a>(&self, source: &'a str, line: usize) -> &'a str {
        let start = self.starts[line];
        let end = self.starts.get(line + 1).copied().unwrap_or(source.len());
        source[start..end].trim_end_matches(['\n', '\r'])
    }

    pub fn lines(&self) -> usize {
        self.starts.len()
    }
}

/// The largest offset no greater than `offset` that splits characters rather
/// than bytes. Spans come from the lexer and normally land on boundaries, but
/// rendering must not panic on one that does not.
pub fn char_boundary(source: &str, offset: usize) -> usize {
    let mut offset = offset.min(source.len());
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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

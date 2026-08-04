use super::error::{ParseError, syntax, syntax_span};
use super::source::Span;
use super::token::{Token, TokenKind};

/// Lex Lab's layout-sensitive surface syntax.
///
/// Newlines and indentation are significant outside paired delimiters. Blank
/// lines and comment-only lines do not affect indentation.
pub(crate) fn lex(source: &str) -> Result<Vec<Token>, ParseError> {
    Lexer::new(source).lex()
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    line_start: bool,
    delimiter_depth: usize,
    indent_stack: Vec<usize>,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            cursor: 0,
            line_start: true,
            delimiter_depth: 0,
            indent_stack: vec![0],
            tokens: Vec::new(),
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, ParseError> {
        while self.cursor < self.bytes.len() {
            if self.line_start && self.delimiter_depth == 0 && self.lex_indentation()? {
                continue;
            }

            let start = self.cursor;
            match self.bytes[self.cursor] {
                b' ' | b'\t' | b'\r' => self.cursor += 1,
                b'\n' => self.lex_newline(),
                b'#' => self.skip_comment(),
                b'/' if self.bytes.get(self.cursor + 1) == Some(&b'/') => self.skip_comment(),
                b'"' => self.lex_string()?,
                byte if byte.is_ascii_digit() => self.lex_number()?,
                byte if byte == b'_' || byte.is_ascii_alphabetic() => self.lex_identifier(),
                b'(' => self.open(TokenKind::LeftParen),
                b')' => self.close(TokenKind::RightParen)?,
                b'[' => self.open(TokenKind::LeftBracket),
                b']' => self.close(TokenKind::RightBracket)?,
                b'{' => self.open(TokenKind::LeftBrace),
                b'}' => self.close(TokenKind::RightBrace)?,
                b',' => self.single(TokenKind::Comma),
                b':' => self.single(TokenKind::Colon),
                b'.' if self.bytes.get(self.cursor + 1) == Some(&b'.') => {
                    self.double(TokenKind::DotDot)
                }
                b'.' => self.single(TokenKind::Dot),
                b'=' if self.bytes.get(self.cursor + 1) == Some(&b'=') => {
                    self.double(TokenKind::EqualEqual)
                }
                b'=' => self.single(TokenKind::Equal),
                b'<' if self.bytes.get(self.cursor + 1) == Some(&b'-') => {
                    self.double(TokenKind::LeftArrow)
                }
                b'<' if self.bytes.get(self.cursor + 1) == Some(&b'=') => {
                    self.double(TokenKind::LessEqual)
                }
                b'<' => self.single(TokenKind::Less),
                b'-' if self.bytes.get(self.cursor + 1) == Some(&b'>') => {
                    self.double(TokenKind::RightArrow)
                }
                b'-' => self.single(TokenKind::Minus),
                b'!' if self.bytes.get(self.cursor + 1) == Some(&b'=') => {
                    self.double(TokenKind::NotEqual)
                }
                b'>' if self.bytes.get(self.cursor + 1) == Some(&b'=') => {
                    self.double(TokenKind::GreaterEqual)
                }
                b'>' => self.single(TokenKind::Greater),
                b'|' => self.single(TokenKind::Pipe),
                b'+' => self.single(TokenKind::Plus),
                b'*' => self.single(TokenKind::Star),
                b'/' => self.single(TokenKind::Slash),
                byte => {
                    return Err(syntax(
                        start,
                        format!("unexpected character '{}'", byte as char),
                    ));
                }
            }
        }

        if self.delimiter_depth != 0 {
            return Err(syntax(self.cursor, "unclosed delimiter"));
        }
        if !self.tokens.is_empty()
            && !matches!(
                self.tokens.last().map(|token| &token.kind),
                Some(TokenKind::Newline)
            )
        {
            self.push(TokenKind::Newline, self.cursor, self.cursor);
        }
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.push(TokenKind::Dedent, self.cursor, self.cursor);
        }
        Ok(self.tokens)
    }

    /// Returns true when an empty/comment-only line was consumed.
    fn lex_indentation(&mut self) -> Result<bool, ParseError> {
        let start = self.cursor;
        let mut width = 0;
        while self.bytes.get(self.cursor) == Some(&b' ') {
            self.cursor += 1;
            width += 1;
        }
        if self.bytes.get(self.cursor) == Some(&b'\t') {
            return Err(syntax(
                self.cursor,
                "tabs are not allowed for indentation; use spaces",
            ));
        }

        if self.cursor >= self.bytes.len() {
            return Ok(true);
        }
        if self.bytes[self.cursor] == b'\n' {
            self.cursor += 1;
            self.line_start = true;
            return Ok(true);
        }
        if self.bytes[self.cursor] == b'#'
            || (self.bytes[self.cursor] == b'/' && self.bytes.get(self.cursor + 1) == Some(&b'/'))
        {
            self.skip_comment();
            return Ok(true);
        }

        let current = *self
            .indent_stack
            .last()
            .expect("indent stack is never empty");
        if width > current {
            self.indent_stack.push(width);
            self.push(TokenKind::Indent, start, self.cursor);
        } else if width < current {
            while width
                < *self
                    .indent_stack
                    .last()
                    .expect("indent stack is never empty")
            {
                self.indent_stack.pop();
                self.push(TokenKind::Dedent, start, self.cursor);
            }
            if width
                != *self
                    .indent_stack
                    .last()
                    .expect("indent stack is never empty")
            {
                return Err(syntax_span(
                    Span::new(start, self.cursor),
                    "indentation does not match an enclosing block",
                ));
            }
        }
        self.line_start = false;
        Ok(false)
    }

    fn lex_newline(&mut self) {
        let start = self.cursor;
        self.cursor += 1;
        if self.delimiter_depth == 0 {
            if !matches!(
                self.tokens.last().map(|token| &token.kind),
                Some(TokenKind::Newline)
            ) {
                self.push(TokenKind::Newline, start, self.cursor);
            }
            self.line_start = true;
        }
    }

    fn skip_comment(&mut self) {
        while self.cursor < self.bytes.len() && self.bytes[self.cursor] != b'\n' {
            self.cursor += 1;
        }
        if self.cursor < self.bytes.len() {
            self.lex_newline();
        }
    }

    fn lex_string(&mut self) -> Result<(), ParseError> {
        let start = self.cursor;
        self.cursor += 1;
        let mut value = String::new();
        while self.cursor < self.bytes.len() {
            match self.bytes[self.cursor] {
                b'"' => {
                    self.cursor += 1;
                    self.push(TokenKind::String(value), start, self.cursor);
                    return Ok(());
                }
                b'\\' => {
                    self.cursor += 1;
                    let escaped = self
                        .bytes
                        .get(self.cursor)
                        .copied()
                        .ok_or_else(|| syntax(start, "unterminated string literal"))?;
                    let character = match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'n' => '\n',
                        b't' => '\t',
                        _ => {
                            return Err(syntax(
                                self.cursor,
                                format!("unsupported escape '\\{}'", escaped as char),
                            ));
                        }
                    };
                    value.push(character);
                    self.cursor += 1;
                }
                b'\n' => return Err(syntax(start, "unterminated string literal")),
                byte if byte.is_ascii() => {
                    value.push(byte as char);
                    self.cursor += 1;
                }
                _ => {
                    return Err(syntax(
                        self.cursor,
                        "non-ASCII text is not yet supported in string literals",
                    ));
                }
            }
        }
        Err(syntax(start, "unterminated string literal"))
    }

    fn lex_number(&mut self) -> Result<(), ParseError> {
        let start = self.cursor;
        while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
            self.cursor += 1;
        }
        if self.bytes.get(self.cursor) == Some(&b'.')
            && self
                .bytes
                .get(self.cursor + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            self.cursor += 1;
            while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                self.cursor += 1;
            }
            self.push(
                TokenKind::Decimal(self.source[start..self.cursor].to_owned()),
                start,
                self.cursor,
            );
        } else {
            let value = self.source[start..self.cursor]
                .parse()
                .map_err(|_| syntax(start, "integer is too large"))?;
            self.push(TokenKind::Integer(value), start, self.cursor);
        }
        Ok(())
    }

    fn lex_identifier(&mut self) {
        let start = self.cursor;
        self.cursor += 1;
        while self
            .bytes
            .get(self.cursor)
            .is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
        {
            self.cursor += 1;
        }
        self.push(
            TokenKind::Identifier(self.source[start..self.cursor].to_owned()),
            start,
            self.cursor,
        );
    }

    fn open(&mut self, kind: TokenKind) {
        self.delimiter_depth += 1;
        self.single(kind);
    }

    fn close(&mut self, kind: TokenKind) -> Result<(), ParseError> {
        if self.delimiter_depth == 0 {
            return Err(syntax(self.cursor, format!("unexpected {kind}")));
        }
        self.delimiter_depth -= 1;
        self.single(kind);
        Ok(())
    }

    fn single(&mut self, kind: TokenKind) {
        let start = self.cursor;
        self.cursor += 1;
        self.push(kind, start, self.cursor);
    }

    fn double(&mut self, kind: TokenKind) {
        let start = self.cursor;
        self.cursor += 2;
        self.push(kind, start, self.cursor);
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, end),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_layout_only_for_significant_lines() {
        let kinds = lex("workflow grow:\n  # note\n  x = f(\n    1,\n    2,\n  )\nnext = 3\n")
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == TokenKind::Indent)
                .count(),
            1
        );
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == TokenKind::Dedent)
                .count(),
            1
        );
    }

    #[test]
    fn rejects_tabs_in_indentation() {
        let error = lex("plasmid p:\n\tsequence = dna(\"ACGT\")\n").unwrap_err();
        assert!(error.to_string().contains("tabs are not allowed"));
    }
}

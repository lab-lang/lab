use crate::error::{ParseError, syntax, syntax_span};
use crate::source::Span;
use crate::token::{Token, TokenKind};

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
                b'/' if self.bytes.get(self.cursor + 1) == Some(&b'/') => self.skip_comment(),
                b'/' if self.bytes.get(self.cursor + 1) == Some(&b'*') => {
                    self.lex_documentation()?
                }
                b'#' => {
                    return Err(syntax(
                        start,
                        "'#' does not start a comment; use '//' for a comment and '/** */' for documentation",
                    ));
                }
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
                b'?' => self.single(TokenKind::Question),
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
        // A comment-only line keeps the indentation of the code around it. A
        // documentation comment is a token, so its line is laid out normally.
        if self.bytes[self.cursor] == b'/' && self.bytes.get(self.cursor + 1) == Some(&b'/') {
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

    /// Lex a documentation comment into a token the parser attaches to what it
    /// describes: `/** */` to the declaration below it, `/*! */` to the module
    /// it opens. `/*` opens documentation and nothing else: an ordinary comment
    /// is `//`.
    fn lex_documentation(&mut self) -> Result<(), ParseError> {
        let start = self.cursor;
        let module = match self.bytes.get(self.cursor + 2) {
            Some(b'*') => false,
            Some(b'!') => true,
            _ => {
                return Err(syntax(
                    start,
                    "'/*' opens documentation, written '/** */' for the declaration below it and '/*! */' for the module; use '//' for an ordinary comment",
                ));
            }
        };
        self.cursor += 3;
        let text_start = self.cursor;
        loop {
            if self.cursor >= self.bytes.len() {
                return Err(syntax_span(
                    Span::new(start, self.cursor),
                    "unterminated documentation comment; close it with '*/'",
                ));
            }
            if self.bytes[self.cursor] == b'*' && self.bytes.get(self.cursor + 1) == Some(&b'/') {
                break;
            }
            self.cursor += 1;
        }
        let text = documentation(&self.source[text_start..self.cursor]);
        self.cursor += 2;
        let kind = if module {
            TokenKind::ModuleDoc(text)
        } else {
            TokenKind::DocComment(text)
        };
        self.push(kind, start, self.cursor);
        self.line_start = false;
        Ok(())
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
        let mut fractional = false;
        if self.bytes.get(self.cursor) == Some(&b'.')
            && self
                .bytes
                .get(self.cursor + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            fractional = true;
            self.cursor += 1;
            while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                self.cursor += 1;
            }
        }
        // A transformation efficiency is 1e9 cfu/ug and a copy number is 2e5.
        // Writing those out is a row of zeros to miscount, which is the class
        // of error a measured language exists to refuse.
        if self.lex_exponent() {
            let magnitude = expanded_exponent(&self.source[start..self.cursor]);
            self.push(TokenKind::Decimal(magnitude), start, self.cursor);
            return Ok(());
        }
        if fractional {
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

    /// Consume an `e12` or `e-3` suffix, reporting whether one was there.
    ///
    /// `e` is only an exponent when digits follow it, optionally after a sign.
    /// Otherwise it opens a unit, and `20 eq` has to keep meaning twenty of
    /// whatever `eq` is.
    fn lex_exponent(&mut self) -> bool {
        if !matches!(self.bytes.get(self.cursor), Some(b'e' | b'E')) {
            return false;
        }
        let mut lookahead = self.cursor + 1;
        if matches!(self.bytes.get(lookahead), Some(b'+' | b'-')) {
            lookahead += 1;
        }
        if !self.bytes.get(lookahead).is_some_and(u8::is_ascii_digit) {
            return false;
        }
        self.cursor = lookahead;
        while self.bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
            self.cursor += 1;
        }
        true
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

/// The prose inside a `/** ... */`, with the decoration a reader supplies for
/// alignment removed: the leading `*` of a continuation line, trailing spaces,
/// and blank lines at either end. Blank lines between paragraphs are kept.
/// `1e9` written out as `1000000000`, and `2e-3` as `0.002`.
///
/// An exponent is a way of writing a number, not a different kind of number, so
/// it is expanded where it is read. Every later pass then sees one decimal
/// spelling and none of them has to learn a second one.
fn expanded_exponent(literal: &str) -> String {
    let (mantissa, exponent) = literal
        .split_once(['e', 'E'])
        .expect("an exponent literal carries its marker");
    let exponent: i64 = exponent.parse().expect("the exponent is a signed integer");
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{whole}{fraction}");
    // Where the point sits once the exponent has moved it.
    let point = whole.len() as i64 + exponent;
    if point <= 0 {
        return format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize));
    }
    let point = point as usize;
    if point >= digits.len() {
        return format!("{digits}{}", "0".repeat(point - digits.len()));
    }
    format!("{}.{}", &digits[..point], &digits[point..])
}

fn documentation(text: &str) -> String {
    let mut lines: Vec<&str> = text
        .lines()
        .map(|line| {
            let line = line.trim();
            line.strip_prefix('*').map_or(line, str::trim_start)
        })
        .map(str::trim_end)
        .collect();
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::lexer::*;

    #[test]
    fn emits_layout_only_for_significant_lines() {
        let kinds = lex(
            "workflow grow() -> Integer:\n  // note\n  x = f(\n    1,\n    2,\n  )\nnext = 3\n",
        )
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

    #[test]
    fn a_documentation_comment_is_a_token_with_its_decoration_removed() {
        let tokens = lex("/**\n * Two composite transcription units.\n *\n * Both are built by Golden Gate assembly.\n */\nx = 1\n")
            .unwrap();
        let TokenKind::DocComment(text) = &tokens[0].kind else {
            panic!(
                "documentation reaches the parser as a token: {:?}",
                tokens[0]
            );
        };
        assert_eq!(
            text,
            "Two composite transcription units.\n\nBoth are built by Golden Gate assembly."
        );
        assert_eq!(
            tokens[1].kind,
            TokenKind::Newline,
            "a documentation comment does not swallow the line it ends"
        );
    }

    #[test]
    fn a_one_line_documentation_comment_keeps_its_prose() {
        let tokens = lex("/** Assemble the reporter plasmid. */\nx = 1\n").unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::DocComment("Assemble the reporter plasmid.".to_owned())
        );
    }

    #[test]
    fn module_documentation_lexes_as_its_own_token() {
        let tokens = lex("/*! What this file is. */\nuse std.bio.build\n").unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::ModuleDoc("What this file is.".to_owned())
        );
    }

    #[test]
    fn rejects_comment_syntax_the_language_does_not_have() {
        let hash = lex("# note\nx = 1\n").unwrap_err();
        assert!(hash.to_string().contains("use '//'"), "{hash}");

        let block = lex("/* note */\nx = 1\n").unwrap_err();
        assert!(block.to_string().contains("'/** */'"), "{block}");
        assert!(block.to_string().contains("'/*! */'"), "{block}");

        let unterminated = lex("/** note\nx = 1\n").unwrap_err();
        assert!(
            unterminated.to_string().contains("unterminated"),
            "{unterminated}"
        );
    }
}

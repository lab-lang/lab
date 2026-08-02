use super::error::{ParseError, syntax};
use super::token::{Token, TokenKind};

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            byte if byte.is_ascii_whitespace() => cursor += 1,
            b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                cursor += 2;
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
            }
            b'{' => {
                tokens.push(Token {
                    kind: TokenKind::LeftBrace,
                    offset: cursor,
                });
                cursor += 1;
            }
            b'}' => {
                tokens.push(Token {
                    kind: TokenKind::RightBrace,
                    offset: cursor,
                });
                cursor += 1;
            }
            b';' => {
                tokens.push(Token {
                    kind: TokenKind::Semicolon,
                    offset: cursor,
                });
                cursor += 1;
            }
            b'"' => {
                let offset = cursor;
                cursor += 1;
                let mut value = String::new();
                let mut terminated = false;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'"' => {
                            cursor += 1;
                            terminated = true;
                            break;
                        }
                        b'\\' => {
                            cursor += 1;
                            let escaped = bytes
                                .get(cursor)
                                .copied()
                                .ok_or_else(|| syntax(offset, "unterminated string literal"))?;
                            let character = match escaped {
                                b'"' => '"',
                                b'\\' => '\\',
                                b'n' => '\n',
                                b't' => '\t',
                                _ => {
                                    return Err(syntax(
                                        cursor,
                                        format!("unsupported escape '\\{}'", escaped as char),
                                    ));
                                }
                            };
                            value.push(character);
                            cursor += 1;
                        }
                        byte if byte.is_ascii() => {
                            value.push(byte as char);
                            cursor += 1;
                        }
                        _ => {
                            return Err(syntax(
                                cursor,
                                "non-ASCII text is not yet supported in string literals",
                            ));
                        }
                    }
                }
                if !terminated {
                    return Err(syntax(offset, "unterminated string literal"));
                }
                tokens.push(Token {
                    kind: TokenKind::String(value),
                    offset,
                });
            }
            byte if byte.is_ascii_digit() => {
                let offset = cursor;
                while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                    cursor += 1;
                }
                let value = source[offset..cursor]
                    .parse()
                    .map_err(|_| syntax(offset, "integer is too large"))?;
                tokens.push(Token {
                    kind: TokenKind::Number(value),
                    offset,
                });
            }
            byte if byte == b'_' || byte.is_ascii_alphabetic() => {
                let offset = cursor;
                cursor += 1;
                while cursor < bytes.len()
                    && (bytes[cursor] == b'_' || bytes[cursor].is_ascii_alphanumeric())
                {
                    cursor += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Word(source[offset..cursor].to_owned()),
                    offset,
                });
            }
            byte => {
                return Err(syntax(
                    cursor,
                    format!("unexpected character '{}'", byte as char),
                ));
            }
        }
    }

    Ok(tokens)
}

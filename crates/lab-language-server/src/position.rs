//! Conversions between byte offsets in document text and the UTF-16
//! line/character positions the protocol speaks.

use lsp_types as lsp;

pub(crate) fn offset_to_position(text: &str, requested: usize) -> lsp::Position {
    let offset = requested.min(text.len());
    let prefix = &text[..text.floor_char_boundary(offset)];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = prefix[line_start..].encode_utf16().count() as u32;
    lsp::Position::new(line, character)
}

pub(crate) fn position_to_offset(text: &str, position: lsp::Position) -> usize {
    let mut offset = 0;
    let mut lines = text.split_inclusive('\n');
    for _ in 0..position.line {
        offset += lines.next().map_or(0, str::len);
    }
    let line = lines.next().unwrap_or_default();
    let mut utf16 = 0;
    for (byte, character) in line.char_indices() {
        if utf16 >= position.character {
            return offset + byte;
        }
        utf16 += character.len_utf16() as u32;
    }
    offset + line.trim_end_matches('\n').len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_positions() {
        let text = "a😀b\nnext";
        assert_eq!(offset_to_position(text, 5), lsp::Position::new(0, 3));
        assert_eq!(position_to_offset(text, lsp::Position::new(0, 3)), 5);
        assert_eq!(position_to_offset(text, lsp::Position::new(1, 2)), 9);
    }
}

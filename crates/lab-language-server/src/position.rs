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

    #[test]
    fn offset_inside_multibyte_moves_to_char_boundary() {
        let text = "a😀b";
        // Byte 2 sits inside the emoji (bytes 1..5). Floor to the emoji start.
        let inside = offset_to_position(text, 2);
        assert_eq!(inside, offset_to_position(text, 1));
        assert_eq!(inside, lsp::Position::new(0, 1));
    }

    #[test]
    fn offset_beyond_source_clamps_to_eof() {
        let text = "hi";
        assert_eq!(
            offset_to_position(text, 100),
            offset_to_position(text, text.len())
        );
        assert_eq!(offset_to_position(text, 100), lsp::Position::new(0, 2));
    }

    #[test]
    fn lsp_character_beyond_line_clamps_to_line_end() {
        let text = "hi\nnext";
        // Line 0 is "hi" (2 UTF-16 units). Past that clamps before the newline.
        assert_eq!(position_to_offset(text, lsp::Position::new(0, 99)), 2);
    }

    #[test]
    fn lsp_line_beyond_document_clamps_to_eof() {
        let text = "a\nb";
        assert_eq!(position_to_offset(text, lsp::Position::new(99, 0)), text.len());
        assert_eq!(position_to_offset(text, lsp::Position::new(99, 5)), text.len());
    }

    #[test]
    fn empty_input_and_newline_boundaries() {
        assert_eq!(offset_to_position("", 0), lsp::Position::new(0, 0));
        assert_eq!(position_to_offset("", lsp::Position::new(0, 0)), 0);
        assert_eq!(position_to_offset("", lsp::Position::new(0, 5)), 0);

        let text = "a\nb";
        // Offset on the newline itself.
        assert_eq!(offset_to_position(text, 1), lsp::Position::new(0, 1));
        assert_eq!(position_to_offset(text, lsp::Position::new(0, 1)), 1);
        // First byte of the next line.
        assert_eq!(offset_to_position(text, 2), lsp::Position::new(1, 0));
        assert_eq!(position_to_offset(text, lsp::Position::new(1, 0)), 2);
    }

    #[test]
    fn round_trip_every_char_boundary_with_emoji() {
        let text = "a😀b\n";
        let mut offsets = vec![0];
        for (i, _) in text.char_indices().skip(1) {
            offsets.push(i);
        }
        offsets.push(text.len());
        for &off in &offsets {
            let pos = offset_to_position(text, off);
            let back = position_to_offset(text, pos);
            assert_eq!(back, off, "round-trip failed at offset {off}");
        }
    }
}

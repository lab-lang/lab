//! Markdown rendering of protocol documents, used where a terminal is the
//! display: `labc --emit manual-protocol` and readable test assertions. The
//! typeset artifact path renders the same documents through `typst`.

use std::fmt::Write;

use crate::backend::document::{Align, Block, Doc, Inline};

pub(in crate::backend) fn render(doc: &Doc) -> String {
    let mut output = String::new();
    writeln!(output, "# {}\n", doc.meta.title).unwrap();
    if !doc.meta.subtitle.is_empty() {
        writeln!(output, "*{}*\n", doc.meta.subtitle).unwrap();
    }
    // Content headings sit under the title, so level 1 renders as `##`.
    output.push_str(&render_blocks(&doc.blocks, 1));
    output
}

fn render_blocks(blocks: &[Block], offset: u8) -> String {
    let mut output = String::new();
    for block in blocks {
        match block {
            Block::Heading { level, label, text } => {
                let marks = "#".repeat(usize::from(level + offset).min(6));
                match label {
                    Some(label) => {
                        writeln!(output, "{marks} {label}: {}\n", inlines(text)).unwrap()
                    }
                    None => writeln!(output, "{marks} {}\n", inlines(text)).unwrap(),
                }
            }
            Block::Paragraph(content) => writeln!(output, "{}\n", inlines(content)).unwrap(),
            Block::Notice(content) => writeln!(output, "> {}\n", inlines(content)).unwrap(),
            Block::Bullets(items) => {
                for item in items {
                    writeln!(output, "- {}", inlines(item)).unwrap();
                }
                output.push('\n');
            }
            Block::Table { columns, rows } => {
                let headers = columns
                    .iter()
                    .map(|column| column.header.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ");
                writeln!(output, "| {headers} |").unwrap();
                let separators = columns
                    .iter()
                    .map(|column| match column.align {
                        Align::Left => "---",
                        Align::Right => "---:",
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                writeln!(output, "| {separators} |").unwrap();
                for row in rows {
                    let cells = row
                        .iter()
                        .map(|cell| inlines(cell))
                        .collect::<Vec<_>>()
                        .join(" | ");
                    writeln!(output, "| {cells} |").unwrap();
                }
                output.push('\n');
            }
        }
    }
    output
}

fn inlines(content: &[Inline]) -> String {
    content
        .iter()
        .map(|inline| match inline {
            Inline::Text(value) => value.clone(),
            Inline::Code(value) => format!("`{value}`"),
            Inline::Bold(value) => format!("**{value}**"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};

    use super::*;

    #[test]
    fn renders_the_full_construct_set() {
        let mut doc = Doc::new(DocMeta {
            title: "Manual protocol".into(),
            subtitle: "Operator manual".into(),
            target: "bench-1".into(),
            instrument: "Test rig".into(),
        });
        doc.notice([
            text("Generated concept protocol for "),
            code("bench-1"),
            text("."),
        ]);
        doc.heading(1, [text("Stage 1")]);
        doc.para([text("Keep everything at 4 °C.")]);
        doc.bullets([vec![text("Volume: 30 µL")]]);
        doc.table(
            [Column::left("Reagent"), Column::right("Volume")],
            [
                vec![vec![text("water")], vec![text("10 µL")]],
                vec![vec![bold("Total")], vec![bold("30 µL")]],
            ],
        );

        let rendered = render(&doc);
        assert!(rendered.starts_with("# Manual protocol\n"));
        assert!(rendered.contains("> Generated concept protocol for `bench-1`."));
        assert!(rendered.contains("## Stage 1"));
        assert!(rendered.contains("Keep everything at 4 °C."));
        assert!(rendered.contains("- Volume: 30 µL"));
        assert!(rendered.contains("| Reagent | Volume |"));
        assert!(rendered.contains("| --- | ---: |"));
        assert!(rendered.contains("| **Total** | **30 µL** |"));
    }
}

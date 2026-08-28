//! Typst rendering of protocol documents — the typeset artifact path. Every
//! rendered document imports `lab-style.typ` from its own directory, so the
//! style sheet is bundled beside each document and the output directory
//! stands alone as a Typst project. `lab build` compiles these sources to
//! PDF in-process; the sources stay on disk beside the PDFs.

use std::fmt::Write;

use crate::backend::document::{Align, Block, Column, Doc, Inline};

/// The style sheet bundled beside every rendered document.
pub(in crate::backend) const STYLE: &str = include_str!("templates/lab-style.typ");

/// The bundle path of the style sheet.
pub(in crate::backend) const STYLE_PATH: &str = "lab-style.typ";

pub(in crate::backend) fn render(doc: &Doc) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "#import \"lab-style.typ\": hl, lab-table, notice, protocol-doc"
    )
    .unwrap();
    writeln!(output, "#show: protocol-doc.with(").unwrap();
    writeln!(output, "  title: \"{}\",", escape_string(&doc.meta.title)).unwrap();
    if !doc.meta.subtitle.is_empty() {
        writeln!(
            output,
            "  subtitle: \"{}\",",
            escape_string(&doc.meta.subtitle)
        )
        .unwrap();
    }
    if !doc.meta.target.is_empty() {
        writeln!(output, "  target: \"{}\",", escape_string(&doc.meta.target)).unwrap();
    }
    if !doc.meta.instrument.is_empty() {
        writeln!(
            output,
            "  instrument: \"{}\",",
            escape_string(&doc.meta.instrument)
        )
        .unwrap();
    }
    writeln!(
        output,
        "  version: \"{}\",",
        escape_string(env!("CARGO_PKG_VERSION"))
    )
    .unwrap();
    writeln!(output, ")").unwrap();
    output.push('\n');
    output.push_str(&render_blocks(&doc.blocks));
    output
}

fn render_blocks(blocks: &[Block]) -> String {
    let mut output = String::new();
    for block in blocks {
        match block {
            Block::Heading { level, label, text } => {
                let marks = "=".repeat(usize::from(*level).min(6));
                match label {
                    Some(label) => writeln!(
                        output,
                        "{marks} #hl(\"{}\"){}\n",
                        escape_string(label),
                        markup(text)
                    )
                    .unwrap(),
                    None => writeln!(output, "{marks} {}\n", markup_line(text)).unwrap(),
                }
            }
            Block::Paragraph(content) => {
                writeln!(output, "{}\n", markup_line(content)).unwrap();
            }
            Block::Notice(content) => {
                writeln!(output, "#notice[{}]\n", markup(content)).unwrap();
            }
            Block::Bullets(items) => {
                for item in items {
                    writeln!(output, "- {}", markup(item)).unwrap();
                }
                output.push('\n');
            }
            Block::Table { columns, rows } => {
                writeln!(output, "#lab-table(").unwrap();
                let align = columns
                    .iter()
                    .map(|column| match column.align {
                        Align::Left => "left",
                        Align::Right => "right",
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(output, "  align: ({align},),").unwrap();
                writeln!(output, "  flex: {},", flexible_column(columns, rows)).unwrap();
                let headers = columns
                    .iter()
                    .map(|column| format!("[{}]", escape(&column.header)))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(output, "  header: ({headers},),").unwrap();
                for row in rows {
                    let cells = row
                        .iter()
                        .map(|cell| format!("[{}]", markup(cell)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(output, "  {cells},").unwrap();
                }
                writeln!(output, ")\n").unwrap();
            }
        }
    }
    output
}

/// A table fills the text width, and the slack goes to whichever left-aligned
/// column carries the most text. Sizing every column equally would wrap long
/// labware names while short ones kept space they never use.
fn flexible_column(columns: &[Column], rows: &[Vec<Vec<Inline>>]) -> usize {
    let width = |index: usize| {
        let header = columns[index].header.chars().count();
        let widest = rows
            .iter()
            .filter_map(|row| row.get(index))
            .map(|cell| {
                cell.iter()
                    .map(|inline| match inline {
                        Inline::Text(value) | Inline::Code(value) | Inline::Bold(value) => {
                            value.chars().count()
                        }
                    })
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);
        header.max(widest)
    };
    columns
        .iter()
        .enumerate()
        .filter(|(_, column)| column.align == Align::Left)
        .max_by_key(|(index, _)| width(*index))
        .map_or(0, |(index, _)| index)
}

/// Inline content as Typst markup.
fn markup(content: &[Inline]) -> String {
    content
        .iter()
        .map(|inline| match inline {
            Inline::Text(value) => escape(value),
            Inline::Code(value) => raw(value),
            Inline::Bold(value) => format!("*{}*", escape(value)),
        })
        .collect()
}

/// Markup that opens a line of its own: a leading list or heading shorthand
/// would change the block's meaning, so it is escaped as well.
fn markup_line(content: &[Inline]) -> String {
    let rendered = markup(content);
    match rendered.chars().next() {
        Some(first @ ('=' | '-' | '+' | '/')) => {
            format!("\\{first}{}", &rendered[first.len_utf8()..])
        }
        _ => rendered,
    }
}

/// Escape the characters that carry meaning in Typst markup. `'`, `"`, `~`,
/// and `-` stay literal so smart quotes and dashes typeset; µ, °, and →
/// pass through as Unicode covered by the embedded fonts.
fn escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '#' | '$' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '@'
        ) {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

/// An identifier in the code face. The backtick form keeps generated sources
/// readable; content a backtick form cannot hold falls back to `#raw("…")`.
fn raw(value: &str) -> String {
    if value.contains('`') || value.contains('\n') || value.starts_with(' ') || value.ends_with(' ')
    {
        format!("#raw(\"{}\")", escape_string(value))
    } else {
        format!("`{value}`")
    }
}

/// Escape a Typst string literal.
fn escape_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use crate::backend::document::{Column, Doc, DocMeta, bold, code, text};

    use super::*;

    #[test]
    fn escapes_every_markup_significant_character() {
        assert_eq!(
            escape(r"a\b#c$d*e_f`g[h]i<j>k@l"),
            r"a\\b\#c\$d\*e\_f\`g\[h\]i\<j\>k\@l"
        );
        // Typographic characters stay literal.
        assert_eq!(
            escape("45 °C, 30 µL, a → b, isn't \"quoted\"-ish ~"),
            "45 °C, 30 µL, a → b, isn't \"quoted\"-ish ~"
        );
    }

    #[test]
    fn line_leading_shorthand_is_escaped() {
        assert_eq!(markup_line(&[text("- not a list")]), "\\- not a list");
        assert_eq!(markup_line(&[text("= not a heading")]), "\\= not a heading");
        assert_eq!(markup_line(&[text("plain")]), "plain");
    }

    #[test]
    fn code_uses_backticks_and_falls_back_to_raw_calls() {
        assert_eq!(raw("p_gfp"), "`p_gfp`");
        assert_eq!(raw("tick`inside"), "#raw(\"tick`inside\")");
    }

    #[test]
    fn renders_one_styled_document() {
        let mut doc = Doc::new(DocMeta {
            title: "Manual protocol".into(),
            subtitle: "Operator manual".into(),
            target: "bench-1".into(),
            instrument: "Test rig".into(),
        });
        doc.heading(1, [text("Stage 1 — assembly")]);
        doc.para([text("Store "), code("p_gfp"), text(" at 4 °C.")]);
        doc.table(
            [Column::left("Reagent"), Column::right("Volume")],
            [vec![vec![text("water")], vec![bold("30 µL")]]],
        );

        let rendered = render(&doc);
        assert_eq!(rendered.matches("#show: protocol-doc.with(").count(), 1);
        assert!(rendered.contains("#import \"lab-style.typ\""));
        assert!(rendered.contains("title: \"Manual protocol\","));
        assert!(rendered.contains("target: \"bench-1\","));
        assert!(rendered.contains("= Stage 1 — assembly"));
        assert!(rendered.contains("Store `p_gfp` at 4 °C."));
        assert!(rendered.contains("align: (left, right,),"));
        assert!(rendered.contains("header: ([Reagent], [Volume],),"));
        assert!(rendered.contains("[water], [*30 µL*],"));
    }

    #[test]
    fn style_template_defines_what_documents_import() {
        for name in ["lab-table", "notice", "protocol-doc"] {
            assert!(
                STYLE.contains(&format!("#let {name}")),
                "lab-style.typ defines {name}"
            );
        }
    }
}

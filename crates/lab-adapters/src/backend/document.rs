//! Format-neutral protocol documents. Adapters describe what an operator
//! document *says* — headings, paragraphs, tables, notices — and the Typst
//! renderer decides how it is written. Escaping is a renderer concern;
//! emitters never see format syntax.

/// A complete operator document: identity plus content.
#[derive(Clone, Debug)]
pub(in crate::backend) struct Doc {
    pub meta: DocMeta,
    pub blocks: Vec<Block>,
}

/// Document identity rendered into the title block, headers, and footers
/// rather than into the content flow.
#[derive(Clone, Debug)]
pub(in crate::backend) struct DocMeta {
    /// Document title, e.g. "Automated plasmid build".
    pub title: String,
    /// The line under the title that says what kind of document this is,
    /// e.g. "Operator manual for one robot session".
    pub subtitle: String,
    /// Exact adapter-profile label. Empty when the document is implementation-independent.
    pub adapter_profile: String,
    /// Instrument label, e.g. "Opentrons OT-2".
    pub instrument: String,
}

impl DocMeta {
    pub fn new(
        title: impl Into<String>,
        subtitle: impl Into<String>,
        adapter_profile: impl Into<String>,
        instrument: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            adapter_profile: adapter_profile.into(),
            instrument: instrument.into(),
        }
    }
}

/// Heading levels are relative to the document: 1 is top. A fragment spliced
/// into another document keeps its internal structure and is shifted as a
/// whole via [`Doc::extend_nested`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::backend) enum Block {
    Heading {
        level: u8,
        /// A short classifier set beside the heading in the label style,
        /// e.g. "Stage 1" or "Run 003". Typography carries the pairing, so
        /// neither renderer needs punctuation invented in the emitters.
        label: Option<String>,
        text: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    /// An admonition set off from the flow, e.g. the generated-concept
    /// disclaimer.
    Notice(Vec<Inline>),
    Bullets(Vec<Vec<Inline>>),
    Table {
        columns: Vec<Column>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::backend) struct Column {
    pub header: String,
    pub align: Align,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backend) enum Align {
    Left,
    Right,
}

impl Column {
    pub fn left(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            align: Align::Left,
        }
    }

    pub fn right(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            align: Align::Right,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::backend) enum Inline {
    /// Prose. May contain any Unicode (µ, °C, →); renderers escape whatever
    /// their format requires.
    Text(String),
    /// An identifier, well address, or file name set in the code face.
    Code(String),
    Bold(String),
}

/// Shorthand constructors so emitters read as content, not as enum plumbing.
pub(in crate::backend) fn text(value: impl Into<String>) -> Inline {
    Inline::Text(value.into())
}

pub(in crate::backend) fn code(value: impl Into<String>) -> Inline {
    Inline::Code(value.into())
}

pub(in crate::backend) fn bold(value: impl Into<String>) -> Inline {
    Inline::Bold(value.into())
}

impl Doc {
    pub fn new(meta: DocMeta) -> Self {
        Self {
            meta,
            blocks: Vec::new(),
        }
    }

    pub fn heading(&mut self, level: u8, text: impl IntoIterator<Item = Inline>) {
        self.blocks.push(Block::Heading {
            level,
            label: None,
            text: text.into_iter().collect(),
        });
    }

    pub fn para(&mut self, content: impl IntoIterator<Item = Inline>) {
        self.blocks
            .push(Block::Paragraph(content.into_iter().collect()));
    }

    pub fn para_text(&mut self, content: impl Into<String>) {
        self.para([text(content)]);
    }

    pub fn notice(&mut self, content: impl IntoIterator<Item = Inline>) {
        self.blocks
            .push(Block::Notice(content.into_iter().collect()));
    }

    pub fn bullets(&mut self, items: impl IntoIterator<Item = Vec<Inline>>) {
        self.blocks
            .push(Block::Bullets(items.into_iter().collect()));
    }

    /// A table with no rows is dropped: a bare header rule carries no
    /// information and reads as a rendering fault on the page.
    pub fn table(
        &mut self,
        columns: impl IntoIterator<Item = Column>,
        rows: impl IntoIterator<Item = Vec<Vec<Inline>>>,
    ) {
        let rows: Vec<_> = rows.into_iter().collect();
        if rows.is_empty() {
            return;
        }
        self.blocks.push(Block::Table {
            columns: columns.into_iter().collect(),
            rows,
        });
    }
}

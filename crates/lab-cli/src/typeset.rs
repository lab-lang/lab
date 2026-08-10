//! In-process Typst compilation of the generated protocol documents. The
//! compiler emits `.typ` sources into the build output; this module typesets
//! each one to a PDF beside it. Everything is hermetic: fonts are embedded in
//! the binary, files resolve only inside the document's own directory, and
//! Typst packages are unavailable: a generated document imports nothing but
//! the bundled `lab-style.typ`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use typst::diag::{FileError, FileResult, SourceDiagnostic, Warned};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::datetime::Time;
use typst_kit::diagnostics::termcolor::{ColorChoice, StandardStream};
use typst_kit::diagnostics::{self, DiagnosticFormat, DiagnosticWorld};
use typst_kit::files::{FileLoader, FileStore};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

/// The engine state shared by every document in one build: the standard
/// library, the embedded fonts, and the time source. Constructed once;
/// [`Typesetter::compile_pdf`] runs per document.
pub(crate) struct Typesetter {
    library: LazyHash<Library>,
    fonts: FontStore,
    time: Time,
}

/// The Lab brand faces (OFL-licensed; see assets/fonts/README.md), embedded
/// so documents render identically on every machine with no font install.
const BRAND_FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/CrimsonPro-Regular.ttf"),
    include_bytes!("../assets/fonts/CrimsonPro-SemiBold.ttf"),
    include_bytes!("../assets/fonts/CrimsonPro-Bold.ttf"),
    include_bytes!("../assets/fonts/Archivo-Regular.ttf"),
    include_bytes!("../assets/fonts/Archivo-Medium.ttf"),
    include_bytes!("../assets/fonts/Archivo-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Archivo-Italic.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"),
];

impl Typesetter {
    pub(crate) fn new() -> Self {
        let mut fonts = FontStore::new();
        for data in BRAND_FONTS {
            fonts.extend(typst::text::Font::iter(Bytes::new(*data)).map(|font| {
                let info = font.info().clone();
                (font, info)
            }));
        }
        fonts.extend(typst_kit::fonts::embedded());
        // SOURCE_DATE_EPOCH pins `datetime.today()` so release builds can be
        // byte-for-byte reproducible.
        let time = std::env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|epoch| epoch.parse::<i64>().ok())
            .and_then(|epoch| Time::fixed_timestamp(epoch).ok())
            .unwrap_or_else(Time::system);
        Self {
            library: LazyHash::new(Library::builder().build()),
            fonts,
            time,
        }
    }

    /// Typeset `document`, a path relative to `root`, and return the PDF
    /// bytes. Engine diagnostics print to stderr with spans into the `.typ`
    /// source on disk, so a failure is inspectable and reproducible with a
    /// standalone `typst compile`.
    pub(crate) fn compile_pdf(&self, root: &Path, document: &str) -> Result<Vec<u8>> {
        let vpath = VirtualPath::new(document)
            .with_context(|| format!("invalid document path {document}"))?;
        let world = LabWorld {
            typesetter: self,
            files: FileStore::new(DirLoader {
                root: root.to_path_buf(),
            }),
            main: FileId::new(RootedPath::new(VirtualRoot::Project, vpath)),
        };

        let Warned { output, warnings } = typst::compile::<PagedDocument>(&world);
        emit_diagnostics(&world, &warnings)?;
        let paged = match output {
            Ok(paged) => paged,
            Err(errors) => {
                emit_diagnostics(&world, &errors)?;
                bail!("failed to typeset {document}");
            }
        };

        // No creation timestamp: the PDF depends only on its sources, so
        // rebuilding a package yields identical documents.
        let options = typst_pdf::PdfOptions::default();
        match typst_pdf::pdf(&paged, &options) {
            Ok(pdf) => Ok(pdf),
            Err(errors) => {
                emit_diagnostics(&world, &errors)?;
                bail!("failed to export {document} as PDF");
            }
        }
    }
}

fn emit_diagnostics(world: &LabWorld, diagnostics: &[SourceDiagnostic]) -> Result<()> {
    if diagnostics.is_empty() {
        return Ok(());
    }
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);
    diagnostics::emit(&mut stderr, world, diagnostics, DiagnosticFormat::Human)
        .context("failed to print typesetting diagnostics")
}

struct LabWorld<'a> {
    typesetter: &'a Typesetter,
    files: FileStore<DirLoader>,
    main: FileId,
}

impl World for LabWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.typesetter.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.typesetter.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.typesetter.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.typesetter.time.today(offset)
    }
}

impl DiagnosticWorld for LabWorld<'_> {
    fn name(&self, id: FileId) -> String {
        let resolved = match id.root() {
            VirtualRoot::Project => id.vpath().realize(&self.files.loader().root).ok(),
            VirtualRoot::Package(_) => None,
        };
        resolved.map_or_else(
            || id.vpath().get_without_slash().to_string(),
            |path| path.display().to_string(),
        )
    }
}

/// Loads files from the document's own directory. Package imports fail:
/// generated documents are self-contained by construction.
struct DirLoader {
    root: PathBuf,
}

impl FileLoader for DirLoader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project => {
                let path = id
                    .vpath()
                    .realize(&self.root)
                    .map_err(|_| FileError::AccessDenied)?;
                std::fs::read(&path)
                    .map(Bytes::new)
                    .map_err(|error| FileError::from_io(error, &path))
            }
            VirtualRoot::Package(_) => Err(FileError::Other(Some(
                "Typst packages are not available in lab builds; generated documents import only the bundled lab-style.typ".into(),
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The templates directory documents a standalone preview workflow, so
    /// its sample document must keep compiling against the style sheet it
    /// imports. Rendering it here catches a style sheet that only works for
    /// the shapes the current emitters happen to produce.
    #[test]
    fn the_style_sheet_sample_compiles() {
        let templates = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lab-compiler/src/backend/typst/templates");
        let directory = tempfile::tempdir().unwrap();
        for name in ["lab-style.typ", "sample.typ"] {
            std::fs::copy(templates.join(name), directory.path().join(name)).unwrap();
        }

        let pdf = Typesetter::new()
            .compile_pdf(directory.path(), "sample.typ")
            .expect("the sample document compiles against the bundled style sheet");
        assert!(pdf.starts_with(b"%PDF-"));
    }
}

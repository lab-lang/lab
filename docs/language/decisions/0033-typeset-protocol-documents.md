# 0033 — Protocol documents are typeset PDFs

## Status

Accepted. Builds on [0007](0007-toolchain-cli-boundary.md), which places
materialization concerns in the CLI and keeps the compiler pure.

## Context

The operator documents a build emits, the manual protocol and the dependency
report, were markdown rendered by each backend with hand-written `writeln!`
calls. Markdown put the burden of presentation on whatever happened to open
the file: a bench protocol read as plain text, tables lost their alignment in
half the viewers, and nothing about the output said that these documents are
the laboratory-facing product of the compiler. The renderers were also coupled
to the format. Every emitter knew markdown syntax, no emitter escaped
anything, and the top-level instruction document spliced wave manuals together
by textually demoting heading lines.

## Decision

Backends describe documents in a small format-neutral model,
`backend::document::Doc`, holding headings, paragraphs, notices, lists, and
tables. Renderers own the syntax. A heading may carry a short label such as
`Stage 1` or `Run 003`, which typography sets apart rather than punctuation,
so emitters never invent a separator. Two renderers exist: `backend::typst`
produces the `.typ` sources bundled with every build, and `backend::markdown`
serves terminals (`labc --emit manual-protocol`) and readable test assertions.

Splicing a wave manual into the consolidated instructions is structural: the
fragment's blocks are appended one heading level down. Each backend's manual
splits into bench, run, and boundary fragments, so the stitched full-build
document states the machine setup and deck layout once above the runs instead
of repeating them under every wave.

`lab build` typesets every emitted `.typ` document to a PDF beside its source
with Typst linked as a library. Typst is chosen over a TeX engine because the
whole stack is Rust: no native libraries to package, no system TeX, and no
first-run download. The Lab brand faces (Crimson Pro, Archivo, IBM Plex Mono)
are embedded in the `lab` binary, so typesetting works offline the moment the
CLI is installed. Compilation is hermetic: files resolve only inside the
document's directory, and Typst packages are refused.

Documents are printed constantly, so the page stays white and the brand is
carried by type, the mark, and amber accents rather than by a colored ground.
Generated prose uses no em dashes. Tables span the text width: a narrow table
sizes its columns to their content and gives the slack to the widest one,
while a table of six or more columns shares the width equally so cells wrap
instead of colliding.

The look of every document lives in one style sheet,
`crates/lab-compiler/src/backend/typst/templates/lab-style.typ`, maintained as
real Typst the way the OT-2 protocol modules are maintained as real Python.
The style sheet is emitted into every output directory that holds a document,
and generated documents import it by relative path, so each directory is a
self-contained Typst project that a `typst` CLI can re-typeset or restyle
without the Lab toolchain.

## Consequences

- The typst crates are a dependency of `lab-cli` only. `lab-compiler` emits
  `.typ` sources and stays lean for `lab-python`. `labc` writes bundles
  without PDFs and prints markdown on stdout, where a terminal is the display.
- A typesetting failure fails the build. The sources are generated, so an
  engine error is an emitter bug, reported with file-and-line diagnostics into
  the `.typ` on disk.
- PDFs carry no creation timestamp, so a rebuilt package yields byte-identical
  documents.
- The `lab` binary grows by the typst stack and its embedded fonts, roughly
  20 MB, which is accepted for an install that typesets offline with no setup.

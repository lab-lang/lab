// The shared look of every generated Lab protocol document, set in the Lab
// brand: paper grounds, warm ink, amber for emphasis, Crimson Pro headlines,
// Archivo body text, IBM Plex Mono for anything that reads like code. The
// palette and type roles mirror the website's /brand page; the `lab` binary
// embeds all three families, so documents render identically everywhere.
//
// Generated documents import this file from their own directory, so each
// output directory is a self-contained Typst project: `typst compile
// <doc>.typ` re-typesets it without the Lab toolchain (pass
// `--font-path` at the brand fonts, or let Typst fall back).

// ── Palette ────────────────────────────────────────────────────────────────
#let paper = rgb("#f9efdd") // the page background
#let shell = rgb("#fef9ef") // cards and panels, a shade lighter than the page
#let sand = rgb("#eee0c3") // sets a section apart from the page around it
#let ink = rgb("#2b1c11") // main text: a warm brown, not black
#let umber = rgb("#7c6551") // secondary text
#let umber-soft = rgb("#a08a74") // the faintest text
#let amber = rgb("#e1901f") // the one emphasis color on the page ground
#let amber-deep = rgb("#99560b") // small emphasized text; readable on paper
#let vessel = rgb("#1d1409") // dark ground: the mark tile

#let font-head = "Crimson Pro"
#let font-body = "Archivo"
#let font-code = "IBM Plex Mono"

// ── The mark ───────────────────────────────────────────────────────────────
// `=` above `<-`: what replay may repeat, and what it may not.
#let lab-mark(size: 24pt) = {
  let u = size / 64
  box(
    width: size,
    height: size,
    clip: true,
    radius: 15 * u,
    fill: vessel,
    {
      place(dx: 17 * u, dy: 17.5 * u, rect(
        width: 31 * u,
        height: 5.5 * u,
        radius: 2.75 * u,
        fill: rgb("#f0e3c9").transparentize(66%),
      ))
      place(dx: 22 * u, dy: 36 * u, rect(
        width: 26 * u,
        height: 5.5 * u,
        radius: 2.75 * u,
        fill: amber,
      ))
      place(curve(
        stroke: (paint: amber, thickness: 5.5 * u, cap: "round", join: "round"),
        curve.move((27 * u, 33.25 * u)),
        curve.line((20 * u, 38.75 * u)),
        curve.line((27 * u, 44.25 * u)),
      ))
    },
  )
}

// ── Building blocks ────────────────────────────────────────────────────────

// A small uppercase label, the way the brand sets kickers.
#let kicker(body) = text(
  font: font-body,
  size: 7.5pt,
  weight: 600,
  tracking: 0.14em,
  fill: amber-deep,
  upper(body),
)

// A heading label: the small classifier ("STAGE 1", "RUN 003") set before
// the heading text, so paired headings need no punctuation.
#let hl(label) = {
  box(baseline: -8%, text(
    font: font-body,
    size: 0.56em,
    weight: 600,
    tracking: 0.12em,
    fill: amber-deep,
    upper(label),
  ))
  h(0.75em)
}

// A protocol table: `header` names the columns, `align` positions them,
// `flex` is the index of the column that absorbs the leftover width, and
// the remaining arguments are the body cells in row-major order.
#let lab-table(align: (), flex: 0, header: (), ..cells) = {
  set text(size: 0.92em)
  // Identifiers like composite_strain_1 are single unbreakable words; a
  // zero-width break opportunity after each underscore lets narrow columns
  // wrap them instead of overflowing into their neighbors.
  show "_": it => it + sym.zws
  // Header labels stay in sentence case: uppercasing corrupts units (µL
  // uppercases to a Greek Mu), and shorter mixed-case words wrap better in
  // narrow columns.
  show table.cell.where(y: 0): set text(
    font: font-body,
    size: 0.82em,
    weight: 600,
    tracking: 0.03em,
    fill: umber,
  )
  table(
    // Tables span the text width. A narrow table sizes its columns to their
    // content and lets the widest one (marked by the renderer) absorb the
    // slack. A wide table has no slack to give, so its text columns share
    // the width equally and wrap instead of colliding.
    columns: if align.len() >= 6 {
      align.map(a => if a == right { auto } else { 1fr })
    } else {
      align.enumerate().map(((i, a)) => if i == flex { 1fr } else { auto })
    },
    align: align,
    stroke: none,
    inset: (x: 7pt, y: 5.5pt),
    table.hline(stroke: 1pt + ink),
    table.header(..header),
    table.hline(stroke: 0.5pt + umber-soft),
    ..cells,
    table.hline(stroke: 1pt + ink),
  )
}

// An admonition set off by the amber accent bar. No panel fill: these
// documents are printed, and the ink-free version reads as a classic
// report epigraph.
#let notice(body) = block(
  width: 100%,
  stroke: (left: 2.5pt + amber),
  inset: (left: 11pt, y: 3pt),
  text(style: "italic", fill: ink.transparentize(18%), body),
)

// ── The document ───────────────────────────────────────────────────────────
#let protocol-doc(
  title: "",
  subtitle: "",
  adapter-profile: "",
  instrument: "",
  version: "",
  kicker-text: "Generated protocol document",
  doc,
) = {
  set document(title: title)
  // No page fill: these documents are printed, so the page stays white and
  // the brand carries through type, the mark, and the amber accents.
  set page(
    paper: "us-letter",
    margin: (x: 2.5cm, top: 2.4cm, bottom: 2.9cm),
    footer: context {
      set text(font: font-body, size: 7.5pt, fill: umber)
      line(length: 100%, stroke: 0.5pt + umber-soft.transparentize(40%))
      v(-3pt)
      grid(
        columns: (auto, 1fr, auto),
        column-gutter: 6pt,
        align: horizon,
        lab-mark(size: 8.5pt),
        [Lab v#version#if adapter-profile != "" [ · #raw(adapter-profile)]],
        counter(page).display("1 of 1", both: true),
      )
    },
  )
  set text(font: font-body, size: 10pt, fill: ink)
  set par(justify: true, leading: 0.62em, spacing: 1.05em)
  show raw: set text(font: font-code, size: 0.88em, fill: amber-deep)

  set heading(numbering: none)
  show heading: set text(hyphenate: false)
  show heading.where(level: 1): it => block(above: 1.8em, below: 0.9em)[
    #set text(font: font-head, size: 16.5pt, weight: 600)
    #it.body
    #v(-3pt)
    #line(length: 100%, stroke: 0.5pt + umber-soft)
  ]
  show heading.where(level: 2): set text(font: font-head, size: 13pt, weight: 600)
  show heading.where(level: 2): set block(above: 1.5em, below: 0.6em)
  show heading.where(level: 3): set text(font: font-body, size: 10pt, weight: 600)
  show heading.where(level: 4): set text(
    font: font-body,
    size: 9.5pt,
    weight: 600,
    style: "italic",
    fill: umber,
  )

  // Title block: mark and wordmark over the document title, then the
  // provenance line, closed by the amber rule that signs the page.
  block[
    #grid(
      columns: (auto, 1fr, auto),
      column-gutter: 9pt,
      align: horizon,
      lab-mark(size: 23pt),
      text(font: font-head, size: 17pt, weight: 600, tracking: -0.012em)[Lab],
      kicker(kicker-text),
    )
    #v(14pt)
    #text(font: font-head, size: 25pt, weight: 600, hyphenate: false)[#title]
    #if subtitle != "" {
      v(3pt)
      text(font: font-body, size: 10.5pt, fill: umber)[#subtitle]
    }
    #v(2pt)
    #set text(font: font-body, size: 8.5pt, fill: umber)
    #grid(
      columns: 3,
      gutter: 16pt,
      if instrument != "" [Instrument: #text(fill: ink)[#instrument]],
      if adapter-profile != "" [Adapter profile: #raw(adapter-profile)],
      [Lab toolchain v#version],
    )
    #v(5pt)
    #line(length: 100%, stroke: 1.5pt + amber)
  ]
  v(1.3em)

  doc
}

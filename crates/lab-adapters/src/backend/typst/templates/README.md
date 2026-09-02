# Typst templates

`lab-style.typ` carries the entire look of the generated protocol documents:
page geometry, fonts, the heading ladder, table rules, notices, and the title
block. The Rust renderer (`../mod.rs`) emits content only: headings, prose,
and calls to the style functions. Typography is edited here, in Typst, not
in Rust.

`sample.typ` is a representative document covering every construct the
renderer emits. Iterate on the style by compiling it directly:

```bash
typst compile --font-path ../../../../../lab-cli/assets/fonts sample.typ
```

Any typst CLI ≥ 0.13 works (`cargo install typst-cli`, or the typst.app web
editor). The brand faces (Crimson Pro, Archivo, and IBM Plex Mono) live in
`crates/lab-cli/assets/fonts/` and are embedded in the `lab` binary; point
`--font-path` at that directory so a standalone preview matches `lab build`
output exactly. Without it, Typst falls back to its own embedded fonts and
warns.

`lab-style.typ` is also emitted verbatim into build output: every directory
that contains a generated `.typ` document also receives a copy, so each output
directory is a self-contained Typst project a user can re-typeset or restyle
without the Lab toolchain.

# Brand fonts embedded in the `lab` binary

The three Lab brand typefaces, as static TrueType instances fetched from
Google Fonts:

- **Crimson Pro** (Regular, SemiBold, Bold) — headlines and the wordmark.
  © 2018 The Crimson Pro Project Authors, <https://github.com/Fonthausen/CrimsonPro>
- **Archivo** (Regular, Medium, SemiBold, Italic) — body text and labels.
  © 2019 The Archivo Project Authors, <https://github.com/Omnibus-Type/Archivo>
- **IBM Plex Mono** (Regular, Medium) — code, measurements, file names.
  © 2017 IBM Corp., <https://github.com/IBM/plex>

All three families are licensed under the SIL Open Font License 1.1
(<https://openfontlicense.org>), which permits bundling and redistribution
inside software. The full license text accompanies each project at the URLs
above.

`lab-cli`'s typesetter embeds these files with `include_bytes!` so generated
protocol documents render in the brand faces on any machine, offline, with no
font installation. The document style sheet that names them lives at
`crates/lab-adapters/src/backend/typst/templates/lab-style.typ`.

# Lab 🧪

Lab is an experimental programming language and toolchain for programmable
biology: expressive biological designs, typed physical materials, and durable
reactive laboratory workflows.

This is a very early prototype. The language frontend can compile the
representative plasmid-design and reactive plasmid-build programs into verified
portable module IR. The older executable pipeline supports one narrower path
from a sequence-verified plasmid specification to an inspectable protocol and
simulated plan. The language, IRs, package model, and runtime will change.

## Project workflow

Development requires Rust 1.95 or newer.

Use `lab` to create projects, check code, build packages, and operate workflows:

```sh
lab new my-lab-project
cd my-lab-project
lab check
lab build
```

To explore the repository example during development:

```sh
cargo run -p lab-cli --bin lab -- check examples/starter-package
cargo run -p lab-cli --bin lab -- build examples/starter-package
```

Build artifacts are written under `.lab/build/` and include a deterministic
package index plus typed portable IR for each source module.

## Compiler development

`labc` intentionally remains a minimal single-source compiler interface:

```sh
labc compiler/docs/language/specimens/plasmid-build.lab --emit module-ir
labc examples/sensor.lab --emit design-ir
labc examples/sensor.lab --emit target-ir
labc examples/sensor.lab --emit simulation
```

See the [plasmid acceptance example](examples/plasmid-acceptance/README.md) for the current pipeline and its limits.

## Repository layout

- `compiler/` contains the language frontend, compilation pipeline, plan model, and outputs.
- `cli/` contains the `lab` project and workflow CLI.
- `editors/vscode/` contains Lab language support for VS Code and Cursor.
- `sdk/` contains experimental Rust and Python APIs.
- `examples/` contains the inputs used to exercise the prototype.

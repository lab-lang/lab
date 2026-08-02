# Lab 🧪

Lab is an experimental programming language and compiler for programmable biology.

This is a very early prototype. It currently supports one narrow path: compiling a single, sequence-verified plasmid specification into an inspectable protocol and a human-readable or simulated plan. The language, intermediate representations, APIs, and output formats will change.

## Try it

Development requires Rust 1.95 or newer.

```sh
labc examples/sensor.lab
```

Other useful outputs are available while developing the compiler:

```sh
labc examples/sensor.lab --emit design-ir
labc examples/sensor.lab --emit target-ir
labc examples/sensor.lab --emit simulation
```

See the [plasmid acceptance example](examples/plasmid-acceptance/README.md) for the current pipeline and its limits.

## Repository layout

- `compiler/` contains the language frontend, compilation pipeline, plan model, and outputs.
- `sdk/` contains experimental Rust and Python APIs.
- `examples/` contains the inputs used to exercise the prototype.

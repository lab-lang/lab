# Compiler implementation notes

`crates/lab-compiler/` builds the experimental `labc` compiler and `lab-opt` IR tool.
These are deliberately compiler-development interfaces. The standard Lab
workflow is exposed through the repository's `lab` binary. None is a stable
interface yet.

The current pipeline is:

1. parse Lab Lang into an `ArtifactSpec`;
2. translate the specification into Design IR;
3. select a protocol for the built-in reference laboratory profile;
4. verify material use and export an `ExecutablePlan`;
5. render or simulate that plan.

The code follows those stages:

- `src/ir/` defines the current Design and Protocol operations;
- `src/analyses/`, `src/passes/`, and `src/stages/` verify and transform IR;
- `src/session/` owns parsing, printing, verification, and pass execution;
- `src/translations/` converts into and out of IR;
- `src/pipeline/` connects the stages;
- `src/plan/` defines and validates the compiler's output plan;
- `src/output/` renders human output and symbolic simulations;
- `src/cli/` and `src/bin/lab-opt/` provide developer-facing commands.

`labc --emit` can expose the specification, Design IR, target-selected IR, executable plan, human rendering, or simulation trace. These formats exist for inspection and testing and may change without compatibility support.

See the [plasmid acceptance example](../examples/plasmid-acceptance/README.md) for runnable commands and current limitations.

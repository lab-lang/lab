# Compiler implementation notes

`crates/lab-compiler/` builds the experimental `labc` compiler and `lab-opt` IR tool. These are deliberately compiler-development interfaces. The standard Lab workflow is exposed through the repository's `lab` binary. None is a stable interface yet.

The current pipeline is:

1. parse Lab Lang into an `ArtifactSpec`;
2. translate the specification into Design IR;
3. select a protocol for the built-in reference laboratory profile;
4. verify material use and export an `ExecutablePlan`;
5. render or simulate that plan.

This is the first vertical slice of a larger progressive lowering stack. LAIR is intended to preserve high-level biological and workflow intent while later dialects select laboratory methods, bind materials and resources, schedule work, and finally produce target-specific operations for instruments, people, and services. A laboratory profile is a compilation target: the same portable Lab program may lower differently for different capabilities and policy preferences, or fail explicitly when a target cannot satisfy its contract.

The current Protocol IR is target-selected but not hardware-level. Containers, inventory lots, locations, timing, scheduling, deck geometry, device commands, and durable dispatch belong to later lowering and runtime layers.

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

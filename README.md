<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/brand/wordmark-full-dark.svg">
    <img alt="The Lab Programming Language" src="docs/assets/brand/wordmark-full-light.svg" width="620">
  </picture>
</p>

<p align="center">
  <em>Lab is a programming language and compiler toolchain for describing biology and orchestrating work in the laboratory.</em>
</p>

It lets scientists describe the biological result they want, the constraints that must hold, and the evidence needed to accept it without binding that intent to a particular laboratory, instrument, or protocol implementation.

## Vision

Lab is working toward a world in which laboratory work is portable, inspectable, and reliable across manual benches, automation, and cloud labs.

Today, protocols commonly entangle scientific intent with site-specific procedures. Lab separates them. A program describes biological designs, physical materials, workflows, and acceptance criteria; the compiler progressively specializes that program for the capabilities, policies, inventory, and hardware of a target laboratory.

One biological program should be adaptable to many valid execution environments without erasing what the scientist meant.

## Approach

Lab treats laboratory automation as a compilation and control problem:

- the language models biological artifacts, physical materials, durable effects, and evidence;
- **LAIR**, the Lab Automation Intermediate Representation, preserves meaning as programs are progressively lowered from portable intent to target-specific operations;
- the compiler checks types, action contracts, and material ownership while keeping specialization decisions inspectable;
- a durable runtime will execute idempotent actions, recover around failures, react to observations, and preserve lineage from intent to outcome.

A dedicated language allows the toolchain to reason about concerns that ordinary APIs tend to hide: sample identity and custody, consumable materials, non-repeatable actions, probabilistic results, and evidence-backed acceptance.

## Status

Lab is an early prototype. The current toolchain parses and checks representative biological designs and workflows, emits typed portable module IR, verifies action contracts and affine material flow, provides editor support, and includes an initial experimental Opentrons OT-2 backend.

The language and its intermediate representations are evolving, and the durable workflow runtime has not yet been built. The next major layers are resource-aware workflow lowering, scheduling and hardware specialization, and durable execution.

## Explore

- [Documentation](docs/README.md)
- [Language design](docs/language/README.md)
- [Golden Gate example](examples/golden-gate/README.md)
- [Compiler internals](crates/lab-compiler/README.md)

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/brand/wordmark-full-dark.svg">
    <img alt="The Lab Compiler" src="docs/assets/brand/wordmark-full-light.svg" width="620">
  </picture>
</p>

<p align="center">
  <em>A compiler for the robotic laboratory. Write the experiment once, in Python or in Lab, and compile it for any lab.</em>
</p>

It lets scientists describe the biological result they want, the constraints that must hold, and the evidence needed to accept it without binding that intent to a particular laboratory, instrument, or protocol implementation.

## Two ways in

The compiler accepts an experiment in two forms, and both produce the same checked module, so neither is a wrapper over the other and nothing downstream can tell them apart.

**Python** is where most work starts, because it is the language a laboratory already has code in. Typed `lab.sbol` builders keep biological designs separate from explicit `build` and `buy` declarations, materialize and validate ordinary [pySBOL3](https://pysbol3.readthedocs.io) documents during compilation, and compose with [LOICA](https://github.com/RudgeLab/LOICA) genetic networks. The `lab` package adds what neither standard has a vocabulary for: artifact provenance, the acceptance claims a build is judged on, and the durable workflows that carry it out.

**Lab** is the native language, the one the checker's vocabulary is built around. It states the same designs, claims, and workflows in the fewest words, and it is what the language documentation teaches.

SBOL is not a third way in. It is the vocabulary designs are written and exchanged in, which both frontends speak: roles carry ontology terms, so a design can arrive from a registry and leave for one.

## Vision

Lab is working toward a world in which laboratory work is portable, inspectable, and reliable across manual benches, automation, and cloud labs.

Today, protocols commonly entangle scientific intent with site-specific procedures. Lab separates them. A program describes biological designs, physical materials, workflows, and acceptance criteria; the compiler progressively specializes that program against the capabilities, policies, inventory, and hardware of a selected facility.

One biological program should be adaptable to many valid execution environments without erasing what the scientist meant.

## Approach

Lab treats laboratory automation as a compilation and control problem:

- the type system models biological artifacts, physical materials, durable effects, and evidence;
- two frontends, Python and Lab, lower to one checked module, which is the portable boundary nothing downstream reaches behind;
- **LAIR**, the Lab Automation Intermediate Representation, preserves meaning as programs are progressively lowered from portable intent to method-selected procedures and facility-bound device operations;
- the compiler checks types, action contracts, and material ownership while keeping specialization decisions inspectable;
- a durable runtime will execute idempotent actions, recover around failures, react to observations, and preserve lineage from intent to outcome.

Modeling these ideas in a type system, rather than in a library's conventions, lets the toolchain reason about concerns that ordinary APIs tend to hide: sample identity and custody, consumable materials, non-repeatable actions, probabilistic results, and evidence-backed acceptance. Writing in Python does not give any of that up, because the Python frontend is checked by the same compiler rather than layered over it.

## Status

Lab is an early prototype. The current toolchain accepts both frontends, parses and checks representative biological designs and workflows, emits typed portable module IR, verifies action contracts and affine material flow, provides editor support, and includes experimental Opentrons OT-2 and Flex backends.

The language and its intermediate representations are evolving, and the durable workflow runtime has not yet been built, so nothing replays today. Generated protocols are a compiler concept spike: a laboratory must verify and qualify them before anything is executed. The next major layers are resource-aware workflow lowering, scheduling and hardware specialization, and durable execution.

## Explore

- [Documentation](docs/README.md)
- [Python SDK](crates/lab-python/README.md) — typed SBOL designs, circuits in LOICA
- [Language design](docs/language/README.md)
- [Golden Gate example](examples/golden-gate/README.md)
- [Golden Gate in Python](examples/golden-gate-python/README.md) — the same checked program through the typed Python frontend
- [Golden Gate, extended](examples/golden-gate-extended/README.md) — the same laboratory with most of the language in it
- [LAIR internals](crates/lab-lair/README.md)
- [Facility planning internals](crates/lab-facility/README.md)

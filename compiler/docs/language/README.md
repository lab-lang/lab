# Lab language design

This directory records the intended Lab language independently from the subset
that the compiler can execute today.

The documents have distinct jobs:

- `syntax.md` records accepted surface-language rules;
- `semantics.md` records the meaning of laboratory values and effects;
- `modules.md` records package imports and idiomatic source organization;
- `open-questions.md` keeps unresolved design choices visible;
- `support.md` records how far each feature has progressed through the compiler;
- `decisions/` records design decisions and their status;
- `specimens/` contains representative programs used to test the design.

A specimen is not necessarily executable. Runnable examples remain under the
repository-level `examples/` directory. The support matrix is the authoritative
statement of what `labc` currently accepts, checks, lowers, and executes.

The language is organized around three source-level concerns:

1. **Design** describes reusable biological intent such as parts, circuits, and
   plasmids.
2. **Workflow** describes durable transformations and reactions involving
   physical materials, instruments, people, observations, and time.
3. **Program** composes concrete designs, policies, and reusable workflows into
   a runnable entry point.

Actual executions are runtime records, not source modules. A program may be run
many times, with each run receiving an independent identity and event journal.

Inspect any syntax specimen without asking the executable subset to lower it:

```sh
labc compiler/docs/language/specimens/plasmid-build.lab --emit source-ast
```

# Core semantics

## Laboratory value kinds

- A **record** is immutable structured information.
- A **design** is immutable biological intent and may be reused freely.
- A **material** has physical identity and is affine: it cannot be copied
  implicitly and must be consumed, retained, stored, transferred, or disposed.
- An **observation** is immutable information recorded by an inspection or
  measurement and carries provenance.
- **Evidence** is immutable information shaped for evaluating a scientific
  claim. Evidence does not imply that the claim passes.
- An **event** is an immutable occurrence stored in the durable workflow
  journal.
- An **outcome** is a tagged result of a workflow or scientific decision.

## Commands and events

An effect binding records a command and durably waits for the corresponding
event. Dispatching a command does not establish that a physical action happened.
Only the recorded completion event establishes the result observed by the
workflow.

Workflow replay must not repeat completed physical actions. Time, randomness,
inventory queries, device interaction, network access, and human decisions are
effects rather than ambient language operations.

## Acceptance

The compiler may establish that a workflow can produce the kinds of evidence
required by a plasmid's acceptance predicates. It cannot establish the values of
future measurements. Acceptance therefore has three distinct judgments:

1. the design and claims are well-typed;
2. the selected workflow covers every evidence obligation;
3. runtime evidence satisfies the predicates.

Only the third judgment produces an accepted physical material.

## Reactive execution

A workflow is a deterministic durable state machine over recorded events.
Handlers for one workflow instance are processed atomically and in journal
order. Returning an outcome terminates the workflow and closes its remaining
logical subscriptions; it does not implicitly cancel physical actions already
in progress.

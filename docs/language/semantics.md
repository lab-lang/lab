# Core semantics

## Laboratory value kinds

- A **record** is immutable structured information.
- A **design** is immutable biological intent and may be reused freely.
- A **material** has physical identity and is affine: it cannot be copied implicitly and must be consumed, retained, stored, transferred, or disposed.
- An **observation** is immutable information recorded by an inspection or measurement and carries provenance.
- **Evidence** is immutable information shaped for evaluating a scientific claim. Evidence does not imply that the claim passes.
- An **event** is an immutable occurrence stored in the durable workflow journal.
- An **outcome** is a tagged result of a workflow or scientific decision.

## Declarations, properties, and identities

A biological declaration is immutable intent. Its `name: value` entries are typed declarative properties, not executable assignments. Portable module IR preserves them as named checked expressions without assigning target-specific meaning to every property name.

A source value may stand for an external identity. For example, `J23101 = part("J23101")` creates a source symbol of type `Part` associated with the external identifier string. The symbol name and external identifier are distinct: renaming one does not silently rewrite the other. The typed symbol can appear in properties and expressions; using a bare string where a `Part`, `Backbone`, `Strain`, or `Antibiotic` is required is a type error.

Typed identity is not availability. It does not establish a lot, quantity, location, provenance chain, or fitness for use. Those claims require inventory resolution and runtime evidence.

## Commands and events

An effect binding records a command and durably waits for the corresponding event. Dispatching a command does not establish that a physical action happened. Only the recorded completion event establishes the result observed by the workflow.

Workflow replay must not repeat completed physical actions. Time, randomness, inventory queries, device interaction, network access, and human decisions are effects rather than ambient language operations.

Every resolved action contract names the capability required to dispatch it, the type of each operand and result, and how each operand participates in physical ownership. `copy` is for freely reusable information, `borrow` permits observation without consuming a material, and `take` transfers a material into the action.

`=` and `<-` therefore have different replay laws. `=` evaluates a deterministic expression or commits an explicit state transition. `<-` creates a durable command boundary and obtains its value from a recorded completion event. The result may look like a local binding, but the physical action must not be repeated merely because a workflow is replayed.

The portable material-flow verifier follows these modes through each workflow. It tracks material places, including projections such as `colony_result.plate`, and rejects copying, use after `take`, hidden loss at a terminating path, and incompatible ownership at control-flow joins. A returned, stored, transferred, or disposed material leaves the workflow's ownership; an action result establishes fresh ownership. `borrow` never changes ownership.

Reactive handlers are checked from their shared captured state. A handler that does not terminate the workflow must preserve captured material ownership so a later event cannot observe a material consumed by an earlier invocation. Iteration over material collections remains unavailable until the language has an explicit consuming iterator contract.

## Workflow interfaces and artifact dependencies

A workflow's parameters and results are part of its declaration signature. A parameter of type `Material<T>` transfers an owned physical input into a workflow instance; it is not documentation attached to the first lines of the body. Inputs express requirements and caller-controlled variability. Results express guarantees and the values or physical ownership transferred back to the caller.

A workflow may declare one result type or an ordered parenthesized list of named typed results. A direct comma-separated `return` must match that arity and each corresponding type. Named results are operation results rather than an implicit record value; a workflow call binds them with the same multi-result effect syntax used by other durable actions. Returning several materials transfers each of them out of the terminating path before the affine checker requires the workflow to own nothing else.

Artifact dependencies are expressed with these ordinary typed interfaces. When a realization workflow consumes `List<Material<Plasmid>>`, the checked operand values identify which artifacts must already exist. A later planner may derive edges, roots, build waves, cycles, and inventory blockers from that dataflow. The language does not encode biological assembly levels or infer dependencies by matching component-name strings.

Heterogeneous reusable design values may acquire a union element type. A component list containing a plasmid symbol and part symbols is checked as `List<Plasmid | Part>`; this does not weaken either symbol into an untyped name.

## Acceptance

The compiler may establish that a workflow can produce the kinds of evidence required by a plasmid's acceptance predicates. It cannot establish the values of future measurements. Acceptance therefore has three distinct judgments:

1. the design and claims are well-typed;
2. the selected workflow covers every evidence obligation;
3. runtime evidence satisfies the predicates.

Only the third judgment produces an accepted physical material.

## Portable semantics and target specialization

Portable module checking resolves module-provided contracts, types expressions, verifies workflow returns, and checks affine material ownership. It does not choose a robot, a deck, a reaction chemistry, or a laboratory schedule.

A target specialization may interpret a documented set of checked properties and resolved operations. It must fail explicitly when required properties, capabilities, value shapes, capacities, or operation sequences are unsupported. Target diagnostics should describe generic constraints where possible; experiment names and tutorial-specific sequences do not belong in the core language checker.

## Reactive execution

A workflow is a deterministic durable state machine over recorded events. Durable mutable memory must be declared explicitly with `state`. Ordinary bindings are immutable, so assigning to an existing non-state name is an error. State initializers and transitions are deterministic typed expressions; a runtime must journal their committed values together with the event transition. Handlers for one workflow instance are processed atomically and in journal order. Returning an outcome terminates the workflow and closes its remaining logical subscriptions; it does not implicitly cancel physical actions already in progress.

# 0055: Solving and Pliron are compiler internals, not contribution seams

## Status

Accepted. Clarifies [0045: LAIR represents method alternatives before facility allocation](0045-lair-method-refinement-and-facility-allocation.md) after generic Intent lowering and explicit Procedure-program builders.

## Context

Lab must accept focused contributions from scientists, Method authors, and instrument specialists without requiring any of them to understand the whole compiler. Two implementation choices deserve particular scrutiny because either could accidentally become a system-wide abstraction: the facility solver and Pliron.

The source boundary is now generic. Every reachable action in the current straight-line Design/Intent lowering subset lowers to one `workflow.perform` shape carrying its exact resolved definition, typed arguments and results, ownership, lineage, and provenance. Method refinement looks up the action's durable operation ID in one `MethodRegistry`; the full Intent separately retains the exact resolved `DefinitionId`. Adding a scientific action therefore does not justify another Intent operation class. Conditional, repeated, and reactive effects remain explicit unsupported-control errors until LAIR represents their control semantics; lowering never flattens them.

Procedure construction is explicit too. A program-producing Method task embeds a declarative template or names a builder, together with the versioned contract the result must satisfy. The task's descriptive operation IRI is not a dispatch key. Adapter compatibility follows from the program contract, required features, facility capability, and validated operational profile, never from a biological-operation allowlist.

These generic boundaries make it possible to judge solving and Pliron on the work they actually perform rather than on extension mechanisms that no longer need them.

## Audit

The 2026-09-05 source audit found:

- `solve_facility_planning` uses owned `PlanningProblem`, inventory, material, adapter-binding, and policy records. It has no SAT, SMT, MILP, or other general solver dependency.
- The solver implementation has 1,366 production lines followed by 837 lines of colocated tests. Most of that production surface performs domain validation, candidate explanation, typed property comparison, and construction of frozen evidence rather than search.
- Search is deliberately bounded. Candidate products are retained only until the planner can distinguish zero, one, or more than one complete solution. Ordered maps, ordered sets, and explicit sorting make diagnostics deterministic.
- Pliron is a direct dependency of `lab-compiler` only. Seventeen of that crate's 79 Rust source files import it. Those files account for 11,362 of 25,149 source lines in the audit snapshot, including their tests.
- No public production boundary exposes a Pliron `Context`, operation pointer, region, value, or analysis manager. Stage-typed program wrappers own those together. `lab-opt` and `CompilerSession` accept textual LAIR for compiler development, not as a contributor compatibility contract.

The counts are an audit snapshot, not an API promise. They make the tradeoff visible: Pliron is a substantial internal commitment, while the facility search is a small domain algorithm with a large validation and explanation surface.

## Decision

Keep the purpose-built facility solver. It answers one safety-relevant question: does the stated scientific work, immutable facility snapshot, configured adapter set, and explicit policy admit exactly one complete plan?

It owns:

- Method and Asset pins;
- active MaterialLot availability and same-component lot interchangeability;
- exact capability-kind, qualification, control-mode, and typed property matching;
- independent versus atomic-Asset requirement binding;
- Procedure contract, feature, and adapter-profile feasibility; and
- deterministic zero-solution explanations and multi-solution ambiguity reports.

It does not own scheduling, batching, routing, stock reservation, material depletion, live inventory queries, cost or duration optimization, device commands, or automatic tie-breaking between scientifically distinct plans. Adapter scheduling occurs after allocation and is validated separately. A laboratory chooses between distinct valid instruments or Methods through explicit policy.

Do not add a general constraint-solver dependency while the problem remains a finite product of independently enumerable candidates and the required result is only infeasible, unique, or ambiguous. Reconsider that choice when constraints couple choices globally, such as consumable quantities, shared capacity over time, reservations, or an explicit optimization objective. The serializable planning-problem and planning-solution contracts must remain the boundary so the search implementation can change without changing Methods, adapters, Python bindings, or LAIR.

Keep Pliron inside `lab-compiler`. It currently earns its cost by providing typed SSA use-def edges, regions for Method alternatives, operation and stage verification, rewrite infrastructure for refinement and allocation, whole-module affine material analysis, and round-trippable textual inspection. Replacing it now would recreate those mechanisms in a private graph implementation without simplifying any contributor interface.

Pliron is not a plugin API. Scientific packages contribute checked Lab declarations. Method packs contribute versioned Method records and name registered Procedure construction contracts. Instrument crates contribute adapter registrations over immutable invocation records. Python surfaces are generated from checked package interfaces. None of those contributors imports Pliron or defines a dialect.

Textual LAIR, `lab-opt`, dialect operations, and pass registration remain compiler-development interfaces with no compatibility guarantee. New public records must continue to own their data and must not contain Pliron handles or lifetimes. A built-in component may use Pliron privately only when it needs verified graph transformations; application composition must still occur through the same owned contracts available to third parties.

Reconsider Pliron if its operations become passive mirrors of owned records, transformations stop using SSA, regions, rewrites, or analyses, or a measured owned-graph prototype provides the same invariants with materially less code. Any such evaluation must compare verification coverage and transformation behavior, not dependency count alone.

## Contribution boundaries

| Contribution | Stable input | Focused implementation work | Must not require |
| --- | --- | --- | --- |
| Scientific vocabulary | checked package interfaces | Lab types, facets, actions, and workflows | a new Intent op, Method choice, adapter, solver rule, or Pliron dialect |
| Method | durable action operation ID and typed signature; exact resolved `DefinitionId` remains in Intent | a versioned Method graph plus explicit Procedure execution contract | facility identities, adapter dispatch, or Pliron |
| Procedure semantics | declarative template or versioned contract and builder registration | complete device-neutral program validation and capability derivation | biological-operation matching or facility policy |
| Instrument adapter | immutable allocated invocation plus validated profile | one adapter registration, feasibility check, lowerer, and optional simulator/runtime service | checked source, unresolved Methods, solver internals, or Pliron |

`lab-project` is the application composition boundary. The CLI and Python bindings call that service; they do not reconstruct lowering, allocation, or artifact persistence.

## Consequences

- Contributors select a seam by subject-matter expertise rather than by compiler stage.
- New biology cannot grow the Intent dialect or an adapter allowlist.
- The solver remains explainable and deterministic until the domain truly needs a general optimization engine.
- Pliron remains substantial but localized implementation machinery. Its value and removal criteria are explicit.
- Immutable versioned records isolate package, Method, adapter, Python, facility, and runtime work from either internal implementation choice.

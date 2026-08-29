# Capability requirements

A durable Lab action identifies scientific intent through a stable operation identity, typed operands and results, material ownership, exact parameters, and source origin. It does not permanently own one capability kind. Facility-independent Method refinement expands each reachable action into one or more candidate Procedure graphs, and those Procedure tasks own the first-class Capability requirements compared with exact `fac:capabilityKind` values on SBOLInventory offerings.

A primitive Method may produce one requirement, a composite Method may produce several operational tasks and requirements, and a high-level service offering may remain a valid alternative primitive Method. An unpinned Method is selected together with exact facility resources rather than before the facility is known. This architecture is recorded by [0045](decisions/0045-lair-method-refinement-and-facility-allocation.md), and [0046](decisions/0046-allocated-procedure-is-the-device-boundary.md) makes its allocated result the only production adapter input.

SBOLInventory Profile 0.2 deliberately keeps capability kinds open. Lab uses the profile's normative terms where they fit exactly and explicit absolute IRIs where the current vocabulary has a gap. Lab never substitutes a local abbreviation, compares suffixes, or guesses equivalence between capability terms.

## Built-in Method coverage

The compiler's validated standard Method registry currently covers the Intent operations reachable in the Golden Gate vertical slice:

| Intent operation | Built-in Method alternatives | Procedure requirements |
| --- | --- | --- |
| `std.bio.build.realize` | manual artifact-realization service; automated Golden Gate | `ArtifactRealization`, or `LiquidHandling` followed by `ThermalCycling` |
| `std.lab.plasmid.provision` | manual material provisioning | `MaterialProvisioning` |
| `std.lab.plasmid.transform` | manual chemical transformation | `ChemicalTransformation` |
| `std.lab.plasmid.recover` | manual recovery; controlled recovery | `Incubation` with an exact duration constraint |
| `std.lab.plasmid.dilute` | serial dilution | `LiquidHandling` |
| `std.lab.plasmid.plate` | manual antibiotic selection | `AntibioticSelection` |

The automated Golden Gate Method is deliberately composite. Reaction setup and thermal cycling are separate Procedure tasks with a typed material edge between them, so a facility may allocate them to different Assets. The source action does not pretend that assembly is one device capability.

Terms such as `ArtifactRealization`, `MaterialProvisioning`, `ChemicalTransformation`, and `AntibioticSelection` are open capability-namespace extensions where Profile 0.2 does not yet provide an exact normative term. `LiquidHandling`, `ThermalCycling`, and `Incubation` are Profile 0.2 vocabulary terms. The compiler preserves exact IRIs either way.

## Portable Method contract

Method authors use the RDF-free `lab-method` crate or the equivalent typed Python `lab.methods` API. A `MethodDefinition` contains:

- one stable absolute Method IRI and one exact Intent operation identity;
- a typed input and output signature shared by all alternatives for that Intent;
- the exact Intent parameters the candidate requires;
- topologically ordered Procedure tasks with open operation IRIs;
- typed task inputs, outputs, scalar or homogeneous-list parameters, and material inputs;
- one or more Capability requirements on every task; and
- typed property constraints with exact property-kind and canonical unit IRIs.

The registry rejects duplicate identities, incompatible candidate signatures, dangling or forward value references, missing requirements, descriptive `UnspecifiedControl`, invalid parameter references, and invalid material expressions before LAIR is constructed. Candidate order is lexical and deterministic only for review; it is never a selection policy.

Methods cannot name a Facility, Zone, Asset, CapabilityOffering, MaterialLot, adapter, location, schedule, or runtime endpoint. Those facts and decisions belong later.

## Authoritative compiler representation

`PortableLairProgram::refine_methods` replaces every supported reachable Intent operation with a `method.choice`. Each candidate region contains generic `procedure.task`, `procedure.parameter`, `procedure.material`, `capability.requirement`, and `capability.constraint` operations and yields a compatible typed result. Source-to-Method ancestry and stable local identities are retained.

The resulting `refined-alternatives` LAIR graph is the only authoritative requirement graph. Facility planning does not walk `CheckedModule`, an action-capability string table, or a separate requirement catalog to rediscover work. A read-only analysis projects `lab.planning-problem.v4` directly from verifier-valid LAIR.

The graph-wide solver combines Method choices with one validated immutable SBOLInventory snapshot, exact active MaterialLot evidence, configured adapter bindings, and manifest policy. It selects exact Methods, CapabilityOfferings, Assets, adapters, material sources, and dependencies together. Zero eligible solutions is an explained failure. Several semantically equal solutions remain an explained ambiguity unless explicit policy distinguishes them.

The allocation pass validates the complete `lab.facility-planning-solution.v2` against the exact problem and applies it back to the same LAIR identities. The resulting `allocated-procedure` module has no unresolved Method choice and carries one exact binding for every Capability requirement and material input. Whole-module affine material analysis runs before adapter projection.

## Matching rules

Capability matching is exact IRI equality. The following checks remain independent:

- the offering belongs to the selected Facility and is effectively active;
- observed qualification satisfies the requirement's minimum;
- the offering's closed control mode is accepted;
- every typed property constraint matches by exact property kind, scalar type, relation, value, and canonical unit;
- exact material sources are available and unambiguous;
- the configured adapter explicitly supports the offering's capability kind and control mode when policy requires an implementation; and
- implementation-specific capacity and feature checks succeed during lowering.

Qualification belongs to the offering, not the Asset or adapter. A runtime implementation cannot promote a Plannable offering to Executable, and an Executable offering is not operable unless a configured adapter supports its exact control mode.

## Adapter uptake

`lab.adapter-invocations.v5` is projected from Allocated Procedure LAIR and retains selected Methods, tasks, requirements, parameters, materials, offerings, Assets, adapters, profile digests, and compiler-evidence digests. Invocations group only the tasks and requirements allocated to one exact Asset and adapter.

Shared typed Procedure views validate operation semantics, capability kind, parameter identity and type, canonical QUDT unit, material role, and allocation ownership before device-specific code runs. OT-2, Flex, and STAR use this boundary today. Unsupported Procedure operations or values fail explicitly instead of being ignored.

The current independently executable adapter contract requires one exact allocated requirement per lowered task. A future atomic multi-capability device contract must model that coordination explicitly and version its invocation record; it cannot regain whole-program compiler access.

## Python uptake

Python-authored Methods serialize the same `lab-method` contract and are validated by Rust. `lab.refine` constructs the same refined LAIR and planning problem as the native frontend. `lab.plan` and `lab.plan_project` call the shared `lab-project` facility service and return typed Method, Procedure task, material, requirement, offering, Asset, adapter, and invocation views. `lab.adapters` exposes the exact built-in adapter catalog and validates operational profiles through the Rust implementation.

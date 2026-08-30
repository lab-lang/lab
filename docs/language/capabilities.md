# Capability requirements

A durable Lab action identifies scientific intent through a stable operation identity, typed operands and results, material ownership, exact parameters, and source origin. It does not permanently own one capability kind. Facility-independent Method refinement expands each reachable action into one or more candidate Procedure graphs, and those Procedure tasks own the first-class Capability requirements compared with exact `fac:capabilityKind` values on SBOLInventory offerings.

A primitive Method may produce one requirement, a composite Method may produce several operational tasks and requirements, and a high-level service offering may remain a valid alternative primitive Method. An unpinned Method is selected together with exact facility resources rather than before the facility is known. This architecture is recorded by [0045](decisions/0045-lair-method-refinement-and-facility-allocation.md), and [0046](decisions/0046-allocated-procedure-is-the-device-boundary.md) makes its allocated result the only production adapter input.

SBOLInventory Profile 0.2 deliberately keeps capability kinds open. Lab uses the profile's normative terms where they fit exactly and explicit absolute IRIs where the current vocabulary has a gap. Lab never substitutes a local abbreviation, compares suffixes, or guesses equivalence between capability terms.

## Built-in Method coverage

The compiler's validated standard Method registry currently covers the Intent operations reachable in the Golden Gate vertical slice:

| Intent operation | Built-in Method alternatives | Procedure requirements |
| --- | --- | --- |
| `std.bio.build.realize` | manual artifact-realization service; automated Golden Gate | `ArtifactRealization`, or atomic `MeteredLiquidTransfer` + `InWellMixing` followed by atomic `ProgrammedBlockTemperatureControl` + `HeatedLidTemperatureControl` |
| `std.lab.plasmid.provision` | manual material provisioning | `MaterialProvisioning` |
| `std.lab.plasmid.transform` | manual chemical transformation | `ChemicalTransformation` |
| `std.lab.plasmid.recover` | manual recovery; controlled recovery | `Incubation` with an exact duration constraint |
| `std.lab.plasmid.dilute` | serial dilution | atomic `MeteredLiquidTransfer` + `InWellMixing` |
| `std.lab.plasmid.plate` | manual antibiotic selection | `AntibioticSelection` |

The automated Golden Gate Method is deliberately composite. Reaction setup and thermal cycling are separate Procedure tasks with a typed material edge between them, so a facility may allocate them to different Assets. The source action does not pretend that assembly is one device capability.

Profile 0.2 defines broad taxonomy terms such as `LiquidHandling`, `ThermalCycling`, and `Incubation` and explicitly leaves its capability and property vocabularies open. Lab uses stable `https://sbol.io/ns/capability#` extension IRIs for the finer operational terms and parameters introduced by canonical contracts, including `MeteredLiquidTransfer`, `InWellMixing`, `ProgrammedBlockTemperatureControl`, and `HeatedLidTemperatureControl`. Terms such as `ArtifactRealization`, `MaterialProvisioning`, `ChemicalTransformation`, and `AntibioticSelection` are also open extensions where the profile has no exact normative term. The compiler preserves exact IRIs in every case. Broad terms remain useful taxonomy parents but do not authorize normalized program dispatch.

## Canonical Procedure programs

Method tasks with supported operational semantics normalize into versioned programs from `lab-procedure` before planning. `PipettingProgramV1` stores logical vessels, references to enclosing task inputs, exact material sources and products, initial liquid quantities when known, ordered liquid operations, contamination-path policies, portable aspiration and dispense strategies, air gaps, blowout, touch-tip, and environmental constraints without naming a device or facility. Validation constructs an exact liquid ledger and rejects known underflow or impossible mixes. Golden Gate setup and serial dilution both use this contract. `ThermalProgramV1` stores the incoming and resulting material states, sample count and fill volume, a global heated-lid setpoint, ordered stages, repeated plateaus, exact temperatures and hold durations, optional controlled ramp rates, and an optional indefinite final hold. Golden Gate cycling uses this contract.

A validated program derives its capability formula. The pipetting formula requests `MeteredLiquidTransfer` with exact minimum and maximum volume bounds, `InWellMixing` with an exact maximum-volume bound, and `TemperatureControlledStaging` when the program constrains source temperature. It additionally requests `LiquidLevelAwareAspiration`, `VesselRelativeLiquidAccess`, `AirGapHandling` with an exact maximum air-gap volume, `PostDispenseBlowout`, and `TouchTip` only when the ordered program uses those techniques. The thermal formula requests `ProgrammedBlockTemperatureControl` with exact block range, sample count, and minimum/maximum working-volume compatibility, `HeatedLidTemperatureControl` only when a setpoint is present, and `ControlledTemperatureRamp` only when at least one step states an explicit rate. Both formulas have `AtomicAssetAssembly` scope, so every clause must bind to offerings on one Asset through one adapter and one exact Procedure implementation. This decision and its compiler invariants are recorded by [0048](decisions/0048-canonical-procedures-derive-capabilities.md).

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

## Package-contributed Method catalogs

Compiler Rust is not the extension boundary. A package can contribute one or more versioned Method documents independently of its planning policy:

```toml
[methods]
documents = ["methods/site-methods.json"]

[[planning.methods]]
source-operation = "std.lab.plasmid.recover"
method = "https://example.org/method/custom-recovery"
```

The document uses the shared `lab.method-catalog.v1` envelope:

```json
{
  "schema_version": "lab.method-catalog.v1",
  "methods": []
}
```

An empty document is valid and contributes no alternatives. A useful document fills `methods` with complete definitions that implement an Intent operation's typed signature and contain at least one valid Procedure task with a Capability requirement. The complete schema is the serialized `lab_method::MethodCatalogDocument` and `MethodDefinition` contract.

Lab loads catalogs from the default package and every reachable path dependency in dependency-first order. Each path must remain inside its package, each document is version-checked and validated independently, and the combined definitions are validated again with the standard catalog. `lab check` therefore catches unknown schemas, duplicate Method IRIs, incompatible alternatives, and malformed task graphs before planning. `lab build` uses the same captured registry whether it emits an inventory-free planning frontier or a facility-bound plan.

Method documents and planning pins remain intentionally separate. A catalog states scientifically valid alternatives; a pin restricts one exact source operation or planning choice to one of those Method IRIs. Neither record contains facility or adapter facts. This boundary is recorded by [0047](decisions/0047-packages-contribute-versioned-method-catalogs.md).

## Authoritative compiler representation

`PortableLairProgram::refine_methods` replaces every supported reachable Intent operation with a `method.choice`. Each candidate region contains generic `procedure.task`, `procedure.parameter`, `procedure.material`, `capability.requirement`, and `capability.constraint` operations and yields a compatible typed result. Source-to-Method ancestry and stable local identities are retained.

The resulting `refined-alternatives` LAIR graph is the only authoritative requirement graph. Facility planning does not walk `CheckedModule`, an action-capability string table, or a separate requirement catalog to rediscover work. A read-only analysis projects `lab.planning-problem.v6` directly from verifier-valid LAIR.

The graph-wide solver combines Method choices with one validated immutable SBOLInventory snapshot, exact active MaterialLot evidence, configured adapter bindings, and manifest policy. It selects exact Methods, CapabilityOfferings, Assets, adapters, material sources, and dependencies together. Zero eligible solutions is an explained failure. Several semantically equal solutions remain an explained ambiguity unless explicit policy distinguishes them.

The allocation pass validates the complete `lab.facility-planning-solution.v3` against the exact problem and applies it back to the same LAIR identities. The resulting `allocated-procedure` module has no unresolved Method choice and carries one exact binding for every Capability requirement and material input. When a task has a normalized Procedure program, each non-manual adapter selection also freezes the exact operation-aware Procedure implementation IRI. Whole-module affine material analysis runs before adapter projection.

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

`lab.adapter-invocations.v8` is projected from Allocated Procedure LAIR and retains selected Methods, exact input/output/yield edges, tasks, normalized programs, Procedure implementation identities, requirements, parameters, materials, offerings, Assets, adapters, profile digests, and compiler-evidence digests. Each allocated requirement freezes its Procedure implementation while an invocation groups the tasks assigned to one exact Asset, adapter, and profile; one physical adapter invocation may therefore realize several explicit Procedure contracts without fragmenting the Asset's output bundle.

Shared typed Procedure views validate operation semantics, canonical program structure, derived capability clauses, parameter identity and type, canonical QUDT units, material roles, and allocation ownership before device-specific code runs. OT-2, Flex, and STAR implement canonical pipetting; OT-2, Flex, and Inheco ODTC implement canonical thermal programs. The Opentrons implementations emit their respective application formats while the ODTC implementation emits `lab.thermocycle-run.v0` for the existing runtime executor. Unsupported Procedure operations or values fail explicitly instead of being ignored.

One independently executable child document realizes one exact Procedure task and names its complete non-empty requirement set. For an atomic formula, invocation validation requires those requirements to share one Asset, adapter binding, and Procedure implementation. Runtime preserves that set on one `Execute` node and dispatches the reviewed document once.

## Python uptake

Python-authored Methods serialize the same `lab-method` contract and are validated by Rust. `MethodCatalog.write(path)` writes a versioned package document; the `include_standard` authoring option controls in-memory composition and is not serialized into that portable document. `lab.refine` constructs the same refined LAIR and planning problem as the native frontend. `lab.plan` and `lab.plan_project` load package-contributed catalogs, compose any additional Python Methods, call the shared `lab-project` facility service, and return typed Method, Procedure task, canonical program, material, requirement, offering, Asset, adapter, and invocation views. `lab.procedures` exposes the same `PipettingProgramV1` and `ThermalProgramV1` structures with exact `Decimal` quantities; it does not reimplement normalization, validation, capability derivation, or allocation. `lab.adapters` exposes the exact built-in adapter catalog and validates operational profiles through the Rust implementation.

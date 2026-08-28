# 0044: Facility graphs and capability binding replace workcell targets

## Status

Accepted. Supersedes [0031: Workcell targets](0031-workcell-targets.md).

## Context

The workcell target proved that one workflow may span several instruments and explicit material movements, but it encoded the facility as a compiler-specific list of stations with fixed kinds. That made one target profile responsible for persistent facility facts, capability assignment, execution configuration, and coordination. It could not naturally represent nested locations, material lots, several interchangeable offerings, distinct qualification levels, or a facility that contains more than one liquid handler.

Lab now depends on the SBOLInventory Profile 0.2 implementation in `sbol-rs`. The profile gives facilities, zones, assets, capability offerings, typed parameters, material lots, and run provenance stable RDF identities without adding those extension classes to core SBOL 3.

## Decision

Lab uses this ownership model:

```text
a facility contains zones
zones locate assets and material lots
assets expose capability offerings
workflows require capabilities
plans bind requirements to offerings and assets
runs record material changes and evidence
```

The persistent catalog and run ledger are SBOLInventory graphs. Workflow requirements remain compiler IR. Allocation, scheduling, adapter selection, and dispatch remain Lab concerns.

A package selects one RDF document through `[inventory].document` and may select one Facility by absolute IRI. If the selector is omitted, the document must contain exactly one Facility. Lab validates both SBOL 3 and SBOLInventory before exposing an immutable inventory snapshot, and a reviewed plan records the exact Facility IRI and source-file SHA-256.

Workflow operations refine into capability requirements identified by stable absolute capability-kind IRIs, minimum qualification, accepted control modes, typed parameter constraints, and material inputs and outputs. The facility planner binds each reachable requirement to an exact `CapabilityOffering` IRI and its owning `Asset` IRI. Candidate ordering is not allocation, so unresolved equal candidates remain an explained ambiguity.

Operational configuration is an overlay keyed by exact Asset IRI. An adapter descriptor states the capability kinds, control modes, document formats, and planning, lowering, simulation, or runtime services its implementation supports. Manufacturer and model never select a driver. The `lab.adapter-profile.v2` schema contains no target, backend, or Asset selector: the manifest's exact Asset-to-driver binding selects the implementation, while the profile supplies only its checked non-secret configuration. Endpoints and credentials remain local runtime configuration rather than facility facts.

The reviewed coordination artifact is `lab.execution-plan.v1`. It freezes inventory, requirement, offering, Asset, MaterialLot, adapter-profile, and reviewed-document hashes in one dependency DAG containing `Execute`, `MoveMaterial`, and `Manual` nodes. Device-specific reviewed formats remain independent child documents.

When an adapter still lowers a whole program rather than one capability requirement at a time, the plan freezes a reviewed adapter-lowering bundle containing the exact triggering requirements and every emitted artifact path, role, format, and digest. Lab does not assign one bundle protocol arbitrarily to one requirement. Runtime preflight verifies the complete bundle, while its Execute nodes remain planning-only until a requirement-aware adapter can attach independently executable child documents.

The runtime executes only the frozen bindings through a registry keyed by Asset IRI, adapter ID, and document format. It never re-queries the facility or substitutes an Asset. Its durable ledger is bound to the plan digest, inventory digest, and execution mode. Live and simulation resume state are deliberately incompatible.

A completed live run writes a new `inventory-after.ttl`; a completed simulation writes `inventory-simulation.ttl`. Both preserve the source graph and add a PROV Activity, exact Asset and input MaterialLot Usages, reviewed evidence Attachments, and timing. Only live execution may generate output MaterialLots.

The workcell target, station taxonomy, `lab.workcell-run.v0`, workcell runtime, independent single-device target profiles, `[build] target`, `lab build --target`, and `lab targets` are removed. Device backends are reachable through explicit Asset-to-adapter bindings only after facility allocation.

## Consequences

- Facility composition is open to any conformant SBOLInventory graph rather than a closed product or station enum.
- Qualification belongs to each capability offering, not to an Asset or an adapter, and neither catalog data nor adapter availability promotes the other.
- Material binding uses exact `MaterialLot -> sbol:built -> Component` identity rather than display-name matching.
- Explicit movement nodes work across two or many Assets and do not assume that the mover is a human or a robot.
- EBEF is an acceptance facility, not a special compiler backend. Public equipment remains `Described` with `UnspecifiedControl`; explicitly synthetic twins establish simulation behavior without implying hardware access.
- General robotics, physics, scene, and remote-compute concerns remain outside this repository under [0042](0042-robotics-incubates-separately.md).

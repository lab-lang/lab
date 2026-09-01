# 0047: Packages contribute versioned Method catalogs

Status: accepted and implemented

## Context

[0045](0045-lair-method-refinement-and-facility-allocation.md) makes a portable Method the facility-independent refinement from scientific Intent to a typed Procedure and Capability graph. [0046](0046-allocated-procedure-is-the-device-boundary.md) makes the selected Allocated Procedure the only production device boundary. Initially, every persistent Method definition still lived in compiler Rust, while Python could supply only in-memory definitions to one API call. Supporting the breadth of a facility such as EBEF requires laboratories, instrument integrators, and workflow packages to distribute new Methods without adding compiler operation classes or rebuilding Lab.

Putting Method definitions directly in `lab.toml` would mix a large typed graph with package selectors and planning policy. Putting them in SBOLInventory would collapse scientific realization knowledge into a facility catalog whose responsibility is persistent physical resources and run records. Loading arbitrary Rust or Python plugins during compilation would make package checking language-dependent and non-reproducible.

## Decision

`lab_lair::method` owns a versioned, RDF-free JSON document contract named `lab.method-catalog.v1`. A document contains only its schema version and portable `MethodDefinition` records. Every Method retains an absolute identity, one exact Intent operation, a typed signature, topologically ordered Procedure tasks, typed values and material expressions, and first-class Capability requirements. It cannot name a Facility, Zone, Asset, CapabilityOffering, MaterialLot, adapter, schedule, endpoint, or credential.

A package contributes documents by path:

```toml
[methods]
documents = ["methods/site-methods.json"]
```

Paths are package-relative JSON files without parent traversal. Project loading canonicalizes each path and rejects symlink escape. The default runnable package receives the documents of every reachable path dependency in dependency-first order. Each document is version-checked and validated independently, then all contributed definitions are composed with the compiler's standard catalog and validated again as one `MethodRegistry`. Duplicate Method identities, incompatible signatures for one Intent operation, malformed Procedure graphs, and invalid Capability requirements therefore fail before LAIR construction. Candidate order remains deterministic review order and never becomes selection policy.

`lab check` and project compilation validate the composed registry even when no facility plan is requested. `lab build` uses that captured registry for both inventory-free refinement and facility planning. `lab.toml` planning entries remain separate exact pins over the resulting alternatives; they do not redefine a Method.

Python's `lab.methods` types serialize the same records. `MethodCatalog.write` emits `lab.method-catalog.v1`, and `lab.plan` or `lab.plan_project` composes package-contributed documents with any additional in-memory Python Methods before invoking the same Rust validator and planner. Python does not parse, validate, refine, or allocate the persistent catalog independently.

## Consequences

- A Method extension is portable package data rather than a compiler fork, Pliron plugin, device target, or frontend-specific callback.
- Package dependencies can distribute scientific Methods together with the Lab modules whose Intent operations and types they refine.
- The Rust and Python authoring surfaces share one serialized contract and one authoritative validation implementation.
- Facility descriptions remain SBOLInventory RDF, Method knowledge remains facility-independent compiler input, and adapter configuration remains a local exact-Asset overlay.
- An unknown catalog version or any conflict fails closed instead of being partially loaded.
- Only JSON is accepted for this initial compiler-owned graph contract. Additional serializations require a new explicit compatibility decision rather than format inference.
- Registry package resolution and integrity pinning remain future work; current dependency composition is limited to the path dependencies the project loader already resolves.

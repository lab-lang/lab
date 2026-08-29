# Capability requirements

A durable Lab action identifies scientific intent through a stable operation identity, typed operands and results, material ownership, and exact source origin. It does not permanently own one capability kind. Facility-independent method refinement expands each reachable action into one or more candidate Procedure graphs, and those procedure tasks produce the capability requirements facility planning compares with exact `fac:capabilityKind` values on SBOLInventory offerings.

A primitive method may produce one requirement, a composite method may produce several operational requirements, and a high-level service offering may remain a valid alternative primitive method. An unpinned method is selected together with exact facility resources rather than before the facility is known. This architecture is recorded by [0045](decisions/0045-lair-method-refinement-and-facility-allocation.md).

SBOLInventory Profile 0.2 deliberately keeps capability kinds open. Lab uses the profile's normative terms where they fit exactly and uses explicit terms in the same capability namespace where the current vocabulary has a gap. An extension term is a stable exact IRI, but it is not represented as a Profile 0.2 vocabulary term until it is contributed upstream. Lab never substitutes a local abbreviation or guesses equivalence from an IRI suffix.

## Current standard-action audit

The current checked-module schema still attaches one capability IRI to each standard action as transitional scaffolding. The table records that implementation and the terms that must be preserved while method definitions replace the direct mapping; it is not the final requirement graph.

| Operation | Capability kind | Profile 0.2 status |
| --- | --- | --- |
| `std.bio.build.realize` | `https://sbol.io/ns/capability#ArtifactRealization` | open extension; abstract requirement that must be refined before asset allocation |
| `std.lab.plasmid.capture` | `https://sbol.io/ns/capability#PlateImaging` | open extension |
| `std.lab.plasmid.synthesize` | `https://sbol.io/ns/capability#DnaSynthesis` | open extension |
| `std.lab.plasmid.assemble` | `https://sbol.io/ns/capability#DnaAssembly` | open extension |
| `std.lab.plasmid.provision` | `https://sbol.io/ns/capability#MaterialProvisioning` | open extension |
| `std.lab.plasmid.transform` | `https://sbol.io/ns/capability#ChemicalTransformation` | open extension |
| `std.lab.plasmid.recover` | `https://sbol.io/ns/capability#Incubation` | Profile 0.2 vocabulary |
| `std.lab.plasmid.dilute` | `https://sbol.io/ns/capability#LiquidHandling` | Profile 0.2 vocabulary |
| `std.lab.plasmid.plate` | `https://sbol.io/ns/capability#AntibioticSelection` | open extension |
| `std.lab.plasmid.pick` | `https://sbol.io/ns/capability#ColonyPicking` | open extension |
| `std.lab.plasmid.screen` | `https://sbol.io/ns/capability#CloneScreening` | open extension |
| `std.lab.plasmid.grow` | `https://sbol.io/ns/capability#Incubation` | Profile 0.2 vocabulary |
| `std.lab.plasmid.purify` | `https://sbol.io/ns/capability#PlasmidPurification` | open extension |
| `std.lab.plasmid.split` | `https://sbol.io/ns/capability#LiquidHandling` | Profile 0.2 vocabulary |
| `std.lab.plasmid.sequence` | `https://sbol.io/ns/capability#SangerSequencing` | open extension |
| `std.lab.plasmid.quantify` | `https://sbol.io/ns/capability#DnaQuantification` | open extension |
| `std.lab.plasmid.store` | `https://sbol.io/ns/capability#ColdStorage` | Profile 0.2 vocabulary |
| `std.lab.plasmid.dispose` | `https://sbol.io/ns/capability#WasteHandling` | open extension |

The audit does not assert that every source action maps directly to one instrument operation. `ArtifactRealization`, `DnaAssembly`, `ChemicalTransformation`, and similar biological requirements refine into candidate methods containing operational requirements such as liquid handling, thermal cycling, incubation, transport, or manual work. Refinement preserves source-action identity, method identity, Procedure node identity, and requirement ancestry so a reviewed plan can explain why each allocated offering is present.

## Current compiler requirement artifacts

`lab build` emits `capability_requirements.json` with schema `lab.capability-requirements.v2` and links it from the portable package index. Each requirement template has a deterministic ID and exact source module, workflow, statement path, and operation. It records the capability-kind IRI, a typed minimum qualification, a typed set of accepted SBOLInventory control modes, exact typed scalar constraints, typed design or data value ports, and typed material inputs and outputs with ownership modes. Every operational parameter names an absolute `fac:propertyKind` IRI, and every quantity also carries a canonical QUDT unit IRI, so allocation never joins RDF facts through source argument names or unit abbreviations.

Runnable packages also emit `capability_instances.json` with schema `lab.capability-requirement-instances.v2`, which records the exact requirement-catalog schema it references. The compiler begins at the exact module named by `build.entry` and its `main` workflow, follows resolved workflow declaration identities across package boundaries, and emits one instance for each reachable call path. Calling one workflow twice creates two distinct instances. Uncalled workflow templates remain portable but are not allocated. Structural branches and loops are retained conservatively as potential work, while recursive workflow expansion is rejected because it cannot yield a finite reviewed plan.

These v2 records describe the implemented transition state. A workflow call does not duplicate its callee's template, and an unused workflow remains visible as a reusable template. The accepted architecture replaces their independent requirement traversal with one canonical reachable program and requirements projected from verifier-valid Method, Procedure, and Capability LAIR. Facility-independent artifacts contain no Asset or CapabilityOffering IRI.

## Accepted requirement IR

Method candidate regions contain first-class capability requirements. Each requirement identifies its Procedure task, exact capability kind, qualification floor, control policy, typed constraint expression, material and value ports, and source-to-method refinement trace. Requirements inside an unselected method candidate are inactive and cannot be allocated.

The compiler now implements the first executable slice of this IR: validated portable definitions are projected into verifier-valid `method.choice` candidate regions containing generic `procedure.task`, exact `procedure.parameter`, `capability.requirement`, and exact `capability.constraint` operations. The bundled registry gives the current build Intent complete coverage and retains real alternatives for artifact realization and culture recovery. The result round-trips as the named `refined-alternatives` LAIR stage and rejects missing methods, signature mismatches, cross-candidate references, duplicate stable identities, and Procedure tasks without requirements. Global solver extraction, solution application, and `allocated-procedure` are not yet connected, so current `lab build` continues to emit the v2 transitional records described above.

Portable method authors do not construct Pliron operations. The RDF-free [`lab-method`](../../crates/lab-method/README.md) contract represents typed method signatures, topologically ordered Procedure tasks, value edges, requirements, and exact constraints as serializable owned Rust values. Its registry validates graphs and candidate compatibility before `lab-compiler` projects them into LAIR; the same contract is the intended boundary for Python method packages.

Planning extracts a constraint problem from the verified refined-alternatives LAIR stage. It selects method candidates, offerings, Assets, adapters, MaterialLots, locations, and scheduling together. The solution is applied back to the same stable LAIR identities, producing an allocated-procedure stage with no unresolved method choice. Requirement extraction never walks the checked workflow independently.

## Matching rules

Capability matching is exact IRI equality. Qualification, control mode, typed parameters, material compatibility, containment, capacity, and configured adapter availability are separate predicates. Candidate order is deterministic for review but never constitutes allocation.

The currently implemented facility phase run automatically by a facility-configured `lab build`, or separately by `lab plan`, emits `lab.facility-allocation.v1` and then projects it into the reviewed `lab.execution-plan.v1` document. Each allocation is `Requirement instance -> CapabilityOffering IRI -> Asset IRI` and records the required and observed qualification, control mode, exact matched parameters, rejected candidates, and an optional exact adapter/profile hash. The accepted graph-wide allocator additionally freezes the selected Method and binds requirements, offerings, Assets, adapters, Component-to-MaterialLot identities, movements, and scheduling in one solution.

An adapter declaration is resolved independently of workflow allocation. `lab.adapter-bindings.v2` joins its exact Asset IRI to only those owned CapabilityOfferings whose capability-kind and control-mode IRIs the declared driver supports, records each offering's exact typed parameters, then records effective activity and separate planning, simulation, and execution eligibility. A Plannable offering does not become Executable because a runtime adapter exists, and an Executable offering does not become operable unless a configured adapter supports its exact control mode.

# Capability requirements

Every durable Lab action carries one absolute capability-kind IRI in checked portable IR. The IRI identifies semantic work required by the workflow; it does not identify a device, driver, asset, or product model. Facility planning compares these requirements with exact `fac:capabilityKind` values on SBOLInventory capability offerings.

SBOLInventory Profile 0.2 deliberately keeps capability kinds open. Lab uses the profile's normative terms where they fit exactly and uses explicit terms in the same capability namespace where the current vocabulary has a gap. An extension term is a stable exact IRI, but it is not represented as a Profile 0.2 vocabulary term until it is contributed upstream. Lab never substitutes a local abbreviation or guesses equivalence from an IRI suffix.

## Standard-action audit

| Operation | Capability kind | Profile 0.2 status |
| --- | --- | --- |
| `std.bio.build.realize` | `https://draggon.org/ns/capability#ArtifactRealization` | open extension; abstract requirement that must be refined before asset allocation |
| `std.lab.plasmid.capture` | `https://draggon.org/ns/capability#PlateImaging` | open extension |
| `std.lab.plasmid.synthesize` | `https://draggon.org/ns/capability#DnaSynthesis` | open extension |
| `std.lab.plasmid.assemble` | `https://draggon.org/ns/capability#DnaAssembly` | open extension |
| `std.lab.plasmid.provision` | `https://draggon.org/ns/capability#MaterialProvisioning` | open extension |
| `std.lab.plasmid.transform` | `https://draggon.org/ns/capability#ChemicalTransformation` | open extension |
| `std.lab.plasmid.recover` | `https://draggon.org/ns/capability#Incubation` | Profile 0.2 vocabulary |
| `std.lab.plasmid.dilute` | `https://draggon.org/ns/capability#LiquidHandling` | Profile 0.2 vocabulary |
| `std.lab.plasmid.plate` | `https://draggon.org/ns/capability#AntibioticSelection` | open extension |
| `std.lab.plasmid.pick` | `https://draggon.org/ns/capability#ColonyPicking` | open extension |
| `std.lab.plasmid.screen` | `https://draggon.org/ns/capability#CloneScreening` | open extension |
| `std.lab.plasmid.grow` | `https://draggon.org/ns/capability#Incubation` | Profile 0.2 vocabulary |
| `std.lab.plasmid.purify` | `https://draggon.org/ns/capability#PlasmidPurification` | open extension |
| `std.lab.plasmid.split` | `https://draggon.org/ns/capability#LiquidHandling` | Profile 0.2 vocabulary |
| `std.lab.plasmid.sequence` | `https://draggon.org/ns/capability#SangerSequencing` | open extension |
| `std.lab.plasmid.quantify` | `https://draggon.org/ns/capability#DnaQuantification` | open extension |
| `std.lab.plasmid.store` | `https://draggon.org/ns/capability#ColdStorage` | Profile 0.2 vocabulary |
| `std.lab.plasmid.dispose` | `https://draggon.org/ns/capability#WasteHandling` | open extension |

The audit does not assert that every source action maps directly to one instrument operation. `ArtifactRealization`, `DnaAssembly`, `ChemicalTransformation`, and similar biological requirements may refine into several operational requirements such as liquid handling, thermal cycling, incubation, transport, or manual work. Requirement refinement must preserve the parent requirement and source-action identity so a reviewed plan can explain why each allocated offering is present.

## Compiler requirement IR

`lab build` emits `capability_requirements.json` with schema `lab.capability-requirements.v2` and links it from the portable package index. Each requirement template has a deterministic ID and exact source module, workflow, statement path, and operation. It records the capability-kind IRI, a typed minimum qualification, a typed set of accepted SBOLInventory control modes, exact typed scalar constraints, typed design or data value ports, and typed material inputs and outputs with ownership modes. Every operational parameter names an absolute `fac:propertyKind` IRI, and every quantity also carries a canonical QUDT unit IRI, so allocation never joins RDF facts through source argument names or unit abbreviations.

Runnable packages also emit `capability_instances.json` with schema `lab.capability-requirement-instances.v2`, which records the exact requirement-catalog schema it references. The compiler begins at the exact module named by `build.entry` and its `main` workflow, follows resolved workflow declaration identities across package boundaries, and emits one instance for each reachable call path. Calling one workflow twice creates two distinct instances. Uncalled workflow templates remain portable but are not allocated. Structural branches and loops are retained conservatively as potential work, while recursive workflow expansion is rejected because it cannot yield a finite reviewed plan.

These records describe workflow definitions. A workflow call does not duplicate its callee's template, and an unused workflow remains visible as a reusable template. Facility planning must instantiate only the workflows reached by the selected program, preserve call and refinement ancestry, and then allocate the resulting operational requirements. The root templates deliberately contain no Asset or CapabilityOffering IRI.

## Matching rules

Capability matching is exact IRI equality. Qualification, control mode, typed parameters, material compatibility, containment, capacity, and configured adapter availability are separate predicates. Candidate order is deterministic for review but never constitutes allocation.

An adapter declaration is resolved independently of workflow allocation. `lab.adapter-bindings.v2` joins its exact Asset IRI to only those owned CapabilityOfferings whose capability-kind and control-mode IRIs the declared driver supports, records each offering's exact typed parameters, then records effective activity and separate planning, simulation, and execution eligibility. A Plannable offering does not become Executable because a runtime adapter exists, and an Executable offering does not become operable unless a configured adapter supports its exact control mode.

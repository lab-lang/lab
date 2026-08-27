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

## Matching rules

Capability matching is exact IRI equality. Qualification, control mode, typed parameters, material compatibility, containment, capacity, and configured adapter availability are separate predicates. Candidate order is deterministic for review but never constitutes allocation.

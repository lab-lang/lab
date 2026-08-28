# golden-gate-extended

A four-strain reporter panel written to exercise most of the language rather than the shortest path to a protocol. The smaller [`golden-gate`](../golden-gate) example is the one to read first; this one is what the same laboratory looks like once the interesting parts are in.

```bash
lab check
lab build
lab run .lab/build --dry-run
```

`lab build` emits portable experiment artifacts, consumes `inventory/facility.ttl`, binds the reachable requirements across the exact Opentrons OT-2 and manual-workstation offerings, resolves the ordered reference plasmid through its exact MaterialLot, and derives five OT-2 protocols and the operator PDFs through the Asset's installed adapter. It prints every emitted bundle, protocol, document, and reviewed-plan path.

## What it shows

**Provenance per thing.** Two plasmids are assembled here and one is ordered from a repository. Being built is a fact about a particular plasmid rather than about plasmids, so `build plasmid` and `buy plasmid` declare the same kind of thing and only differ in where it came from. `buy restriction_enzyme BsaI:` carries its own datasheet: the temperature a digest runs at belongs to the enzyme, so no design repeats it.

**Generics.** `regulated_expression` is written once and works for any signal. The panel type `List<Circuit<any Signal, GreenFluorescentProtein>>` says the trigger varies and the product does not, which is what makes three readings comparable. `characterize` keeps the signal named, so inducing a tet-responsive circuit with arabinose is a type error rather than a wasted plate.

**A package extending the vocabulary.** `Isopropylthiogalactoside` is a signal the standard library never heard of. Declaring it is all it takes: a promoter for it can be bought, a circuit can respond to it, and the compiler refuses to induce that circuit with anything else.

**Reaction chemistry and facility realization.** What a plasmid is comes from `std.bio.designs`; what Golden Gate needs to build one comes from `std.bio.golden_gate`; what this laboratory can perform comes from `inventory/facility.ttl`. The operational overlay binds Lab's adapter implementation to one exact Asset without introducing another target model.

**Evidence.** `across 3 biological replicates` says what a claim is believed on. Three measurements of one colony are one biological replicate however many times they are repeated, and the compiler knows the difference because it knows where each sample came from.

**Reacting rather than waiting.** A plate is ready when enough colonies have appeared, not on a schedule, so `await_colonies` images on a timer and finishes on whichever comes first.

**Fetching without asking where it came from.** `provision reference_gfp` takes the ordered plasmid off the shelf the same way `provision BL21` takes competent cells. It does not consult provenance, deliberately: a plasmid this laboratory bought and one it assembled last month are both simply available, and which is which is a question for the inventory rather than for a workflow.

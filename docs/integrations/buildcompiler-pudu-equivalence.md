# BuildCompiler and PUDU equivalence for the Golden Gate vertical slice

This document records the behavior Lab must preserve from the Myers Research Group Golden Gate toolchain while moving that behavior into reusable compiler contracts. It is an implementation audit, not an endorsement of every upstream implementation detail and not a second source of biological truth.

## Pinned reference revisions

The comparison is against:

- BuildCompiler `cae798452450ec152881cfe447a83469202b5bed`;
- PUDU `1214d2f9efd557aa84bc96502379554174355eae`; and
- Lab `a75f87339df56d6d0753802a331adfd8ec23e5c0`, the first compiler revision with Python views over canonical pipetting and thermal programs.

BuildCompiler and PUDU have different responsibilities. BuildCompiler turns SBOL design and inventory information into deterministic assembly, transformation, and plating jobs, preserves product identity and source-well lineage, and emits thin Opentrons wrappers. PUDU owns the mature OT-2 procedure implementations, including resource checks, liquid-handling order, tuned movements, and plate-map outputs. Functional equivalence therefore means preserving the combined semantics of both systems, not reproducing either repository's internal class structure or byte-for-byte Python.

## Equivalence boundary

| Concern | Reference behavior | Lab owner |
| --- | --- | --- |
| Design identity | BuildCompiler carries exact plasmid and strain URIs through the build | Design and Intent LAIR, then material lineage |
| Build ordering | BuildCompiler chains assembly products into transformation and transformation products into plating | Typed material dataflow and Method task dependencies |
| Product-to-well handoff | BuildCompiler/PUDU pass plasmid and transformed-culture location maps between stages | Allocated Procedure values and one reviewed physical-location ledger |
| Biological parameters | Reaction composition, transformation replicates, thermal profiles, dilution factor, and plating replicates are explicit | Method parameters normalized into canonical Procedure programs |
| Liquid operation order | PUDU fixes reagent order and orders dilution-2 seeding before agar contact | Ordered canonical pipetting steps |
| Contamination control | PUDU uses fresh or shared tips deliberately and avoids re-entering a source after destination contact | Canonical fluid-path constraints, checked by each adapter planner |
| Liquid-access technique | PUDU uses source mixing, air gaps, blowout, touch-tip, above-liquid dispense, agar-surface spotting, and volume-aware aspiration | Canonical technique requirements plus calibrated adapter realization data |
| Resource allocation | PUDU validates wells, tubes, tips, and per-vessel volume | Adapter planner over the immutable canonical program and validated profile |
| Robot commands | PUDU calls the Opentrons Python API | OT-2 adapter reviewed run document |
| Plate maps | PUDU emits JSON and XLSX descriptions of agar wells | Compiler-generated evidence derived from the same allocation used by the robot protocol |

The portable Procedure contract states what must be observable in any valid realization. Exact device calibration belongs to the implementation profile. For example, a program can require tracked-surface aspiration and touch-tip; the OT-2 implementation profile can retain a calibrated 10 mm submersion offset, 3 mm floor, 20 percent low-volume fallback, 0.5 touch radius, -14 mm touch offset, and 20 mm/s touch speed. A STAR realization can satisfy the same semantic requirements with its measured labware height model, liquid classes, and firmware coordinates rather than copying Opentrons arguments.

## Assembly behavior

The equivalence target for Golden Gate setup includes exact water balancing, ordered additions of water, buffer, ligase, restriction enzyme, backbone, and ordered inserts, one independently addressable destination per assembly replicate, and an exact construct-to-reaction-well map. PUDU mixes every non-water source before aspirating, uses deliberately reduced aspiration flow, blows out and touches the well wall after transfers, and finishes each reaction with repeated bottom-to-bottom liquid movement to clear bubbles. Lab must represent the source mixes, transfer techniques, and bubble-clearing operation explicitly enough that they survive normalization and appear in the reviewed plan.

The thermal program remains a separate task so a facility can bind setup and cycling to different qualified Assets. Its exact plateaus, repeats, working volume, lid behavior, and final hold are canonical thermal semantics. The Golden Gate example authors the 42 °C/16 °C, 75-cycle profile followed by 60 °C and 80 °C steps, a 42 °C lid, and a 4 °C hold in ordinary Lab source. Both built-in automated Golden Gate Methods carry those source values unchanged. The temperature-staged Method changes preparation technique only; neither Method nor the OT-2 adapter substitutes a reference implementation's hidden thermal defaults.

## Transformation behavior

The equivalence target includes exact competent-cell and DNA volumes, transformation replicates, multi-plasmid co-transformation into the same destination, exact plasmid-product-to-source-well lineage, and a distinct destination culture for every transformation replicate. PUDU groups competent-cell distribution by physical source tube, mixes each source before distribution, uses no disposal volume, mixes each DNA source before transfer, touches the destination well, and performs two explicit bubble-clearing strokes after DNA addition.

Heat shock is an ordered thermal program over the transformation mixtures: 4 °C for the long cold incubation, the authored heat-shock plateau, and 4 °C for the short recovery. Recovery-medium addition happens after that thermal program. PUDU dispenses recovery medium above the liquid to avoid source-path contamination and uses a 10 µL air gap. The subsequent recovery hold is a separate workflow operation and must remain distinguishable in LAIR and provenance even when the same OT-2 thermocycler realizes both operations.

The PUDU revision passes `block_max_volume=30` to the recovery profile after adding recovery medium. The actual sample volume is larger, so Lab must calculate block volume from its volume ledger rather than reproduce that literal. PUDU also enables water-testing behavior automatically during simulation; Lab simulation must exercise the real reviewed steps and may not silently omit thermal work.

## Dilution and plating behavior

Every transformed-culture replicate has its own serial-dilution series. PUDU's default 10-fold dilution uses 2 µL culture and 18 µL medium for two ordered dilution steps, with five 19 µL mixes. Medium is distributed before culture, and the second dilution is seeded before the first-dilution tip contacts agar. Lab must preserve this ordering and the replicate/dilution shape rather than flattening a batch into one source and one series.

PUDU loads a 10 mL LB source into a 15 mL conical tube and recomputes the aspiration position for each chunk of eight destinations. Its current approximation maps tracked volume linearly onto `well.depth - 10 mm`, aspirates 10 mm below the estimated surface with a 3 mm minimum height, and falls back to the ordinary well location below 20 percent capacity. The reusable requirement is volume-aware safe aspiration from a declared source geometry. Those current values are an attributed OT-2 calibration profile, not universal pipetting semantics and not a formula adapters should duplicate independently.

For plating, each dilution and transformation replicate maps to exact selective-agar wells. PUDU spots 4 µL at 8 mm below the well top and blows out. One clean path may plate all first-dilution replicates only after seeding dilution two; a new path plates the second-dilution replicates. The compiler must emit the plate map as a static build artifact from the same allocation that produced the protocol, not rely on a simulator writing files as a side effect.

## Current Lab gaps at the pinned revision

The pinned Lab revision emits standalone OT-2 protocols for Golden Gate reaction setup, Golden Gate thermal cycling, and one serial-dilution series per strain. It does not allocate or emit automated transformation, recovery-medium addition, recovery thermal control, or selective plating. Its pipetting contract preserves volumes, ordering, mixing counts, and two fluid-path policies, but cannot yet state the PUDU liquid-access and post-dispense techniques. It also treats each recovered-culture input as one logical position, losing transformation-replicate shape before dilution.

## Implementation status after the audit

The `marpaia/compiler` work after the pinned revision closes the semantic OT-2 gaps identified above. Automated transformation, heat shock, recovery-medium addition, recovery incubation, replicate-aware serial dilution, and selective plating normalize into the same canonical pipetting and thermal contracts as assembly. The generic `temperature-staged-golden-gate` Method normalizes 4 °C source staging, three-cycle mixing of every non-water source at its transfer volume, isolated transfer paths with blowout and touch-tip, final-path reuse, and bottom-relative bubble clearing into `PipettingProgramV1`. Downstream tasks retain a distinct strain `subject`, exact material dataflow, and transformation-replicate shape. Technique requirements derive `TemperatureControlledStaging`, `LiquidLevelAwareAspiration`, `VesselRelativeLiquidAccess`, `AirGapHandling`, `PostDispenseBlowout`, and `TouchTip` capability clauses instead of hiding those behaviors behind `LiquidHandling`.

The OT-2 profile owns the PUDU-derived calibration values. The adapter validates source, destination, plate, tip, and thermocycler capacities, then writes a versioned allocated-procedure schedule with persistent physical locations and three dependency-ordered execution groups. For the bundled two-plasmid/four-strain build, one facility route emits three standalone Python protocols and four PDFs: one assembly run, one transformation/recovery run, one dilution/plating run, and one aggregate plate map. Every Procedure task remains explicit inside its reviewed group. The plate-map JSON and PDF carry the strain subject, selection MaterialLot, exact source and destination wells, culture and plating replicate numbers, and cumulative dilution ratios.

The checked Golden Gate regression asserts the high-risk reference behavior in both immutable manifests and generated Python: 4 °C assembly-source staging; water-first reagent order; three-cycle non-water source mixing at the transfer volume; reduced-rate transfer with blowout and calibrated touch-tip; reuse of the final-part tip for two 20 µL bottom-to-8-mm bubble-clearing strokes; the exact authored 42/16/60/80 °C thermal program; zero-disposal competent-cell distribution; DNA source mixing; one blowout before two DNA bubble-clearing strokes and one touch-tip afterward; 22 µL heat-shock volume; no simulation-only thermal omission; above-liquid recovery dispense with a 10 µL air gap; 82 µL recovery incubation volume; volume-aware medium aspiration with eight-well recalculation chunks and 4 µL disposal volume; layer-major dilution prefill; the two-tip dilution/plating order; 4 µL material-surface plating with blowout; and agreement between the robot allocation and static plate-map evidence. The repository scripts make byte-compilation and official simulation repeatable, but the hosted CI environment does not currently install the simulator.

The allocated schedule closes the former resource-schedule gap without erasing the compiler's task graph. Shared competent-cell and recovery-medium distributions run across compatible transformations, and dilution/plating uses two tips per recovered-culture replicate: the first seeds dilution two before contacting agar and then plates dilution one, while the second plates dilution two. The schedule—not a template heuristic—proves which allocated tasks share the exact Asset, freezes every inter-task well, preserves dependency order, and carries the union of task requirements into each reviewed execution node. Transformation DNA may be either an exact preceding assembly output or an exact SBOLInventory MaterialLot staged into a non-conflicting DNA-plate well. Partial or incompatible invocations retain the task-level fallback rather than being fused unsafely.

`examples/golden-gate/reference/pudu-equivalence.json` is the machine-readable external oracle. It pins the BuildCompiler and PUDU revisions, the real GEN1 thermocycler load name, reference techniques and values, and an exemplar lineage from assembly well A1 through transformation wells A1/B1 to the A1/B1 and A7/B7 dilution and agar positions. Integration tests compare canonical manifests, generated command ordering, and static plate-map evidence with that semantic data rather than comparing Python bytes. The fixture does not select a Method and no compiler production path reads it.

## Acceptance gates

The Golden Gate OT-2 slice reaches functional equivalence when all of the following are true:

1. One build emits independently reviewable setup, cycle, transformation setup, heat shock, recovery-medium addition, recovery hold, serial dilution, and plating work for every reachable artifact.
2. Exact design, MaterialLot, upstream product, transformation replicate, dilution step, and destination well identities remain traceable across every task boundary.
3. Canonical programs preserve PUDU's meaningful operation order, source/destination mixing, fluid-path isolation, air gap, touch-tip, blowout, aspiration strategy, dispense strategy, and bubble-clearing requirements without containing deck slots or Opentrons API calls.
4. OT-2 plans retain the pinned implementation calibration and validate well capacity, loaded source volume, aspirated volume, tip capacity, pipette working range, module limits, and every physical location before rendering.
5. Agar plate-map JSON and a human-readable plate-map document are generated from the reviewed allocation and agree with the Python protocol.
6. Golden fixtures derived from BuildCompiler inputs produce semantically equivalent construct, source-well, culture-well, dilution-well, and agar-well mappings.
7. Every emitted Python protocol byte-compiles and passes the official Opentrons simulator with the real thermal operations present.
8. Rust tests pin canonical normalization and device planning separately, Python tests expose the complete immutable Procedure views, and adapter golden tests pin the operational techniques most likely to regress.

## Remaining qualification work

The pinned semantic equivalence boundary is implemented and regression-tested, but this is not yet a hardware-qualification claim. The next gate is an operator-reviewed smoke run on the actual OT-2 with its Thermocycler Module GEN1, starting with water or inexpensive surrogates, followed by a biological acceptance run whose evidence is recorded. The official Opentrons simulator should also become a hosted CI gate with pinned Python and simulator versions.

Flex and STAR intentionally retain the basic Golden Gate Method. Their adapters do not advertise temperature-controlled staging or the vessel-relative finish techniques required by the temperature-staged Method, and their lowerers reject an enriched canonical setup defensively. They can support the same portable Method only after independently implementing and qualifying those semantics; copying OT-2 calibration values into either adapter would violate the abstraction established here.

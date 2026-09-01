# 0048: Canonical Procedure programs derive atomic capability formulas

## Status

Accepted. Extends [0045: LAIR represents method alternatives before facility allocation](0045-lair-method-refinement-and-facility-allocation.md) and [0046: Allocated Procedure is the only device-lowering boundary](0046-allocated-procedure-is-the-device-boundary.md).

## Context

Broad facility taxonomy terms such as `LiquidHandling` and `ThermalCycling` are useful for describing classes of work and equipment, but they are too coarse to be executable compiler contracts. A liquid handler does not implement one undifferentiated operation: a scientific procedure may need metered transfers over a particular volume range, in-well mixing, temperature-controlled staging, contamination isolation, ordered barriers, or other independently stated effects. A thermocycler likewise combines programmed block control, sample and plate limits, heated-lid control, and sometimes explicit ramp-rate control. Encoding the missing semantics in each adapter makes an Opentrons Flex and Hamilton STAR reconstruct the same liquid recipe differently, makes an Opentrons module and standalone Inheco ODTC reconstruct the same thermal recipe differently, and prevents the facility planner from determining whether an Asset actually offers every operation the recipe requires.

One high-level Procedure task may also require several capabilities that must be provided by one physical Asset and one adapter implementation. Treating those requirements as independently allocatable can produce an impossible plan in which transfer and mixing are assigned to different instruments even though one ordered pipetting program must remain intact.

## Decision

Lab defines versioned, device-neutral operational contracts in `lab_lair::procedure`. Method refinement recognizes supported open Procedure operation IRIs and normalizes their exact parameters, material inputs, value inputs, and outputs into one canonical `ProcedureProgram` before facility planning. The program contains observable operations and scientific constraints, but no facility identity, deck coordinate, labware model, pipette model, firmware instruction, endpoint, or credential.

Each validated canonical program deterministically derives a `CapabilityFormula`. A formula contains stable clauses with exact capability-kind IRIs and typed property constraints plus an explicit binding scope. `AtomicAssetAssembly` requires every clause to bind to offerings on one Asset through one adapter and one exact Procedure implementation. Clause ordering is stable for evidence generation and never acts as allocation policy.

The canonical program and its derived formula remain authoritative throughout the compiler:

1. Method refinement stores the program on the `procedure.task` and replaces its provisional execution-policy requirement with the derived clauses.
2. LAIR verification and planning-problem validation require the stored requirements to equal the derived formula exactly.
3. Facility solving matches every clause against exact SBOLInventory CapabilityOfferings and enforces the formula's binding scope.
4. Allocation freezes each `Requirement -> CapabilityOffering -> Asset` binding and the common Procedure implementation IRI.
5. Adapter-invocation validation re-derives the formula, verifies all bindings, and passes the immutable canonical program to the selected implementation.
6. A concrete adapter validates the program shape it implements, allocates private device resources, and emits one reviewed child document naming the complete non-empty requirement set it realizes.
7. The execution plan preserves that requirement set on one `Execute` node, and runtime validates and executes the child document once.

Matching happens at two levels, because "which Asset can do this work" and "can this code realize this program" are different questions. A `CapabilityFormula` answers the first against SBOLInventory offerings, whose vocabulary describes classes of equipment, and is deliberately coarser than the program. A program feature set answers the second against the exact operation being lowered, because one implementation lowers several operations through different emitters: a plating run spots onto agar while a reaction setup cannot, and an aspiration strategy one emitter dispatches is one another would silently ignore. Declaring features per implementation rather than per operation would assert that every operation it covers handles every value any one of them handles. Features are derived from the canonical program by matching every variant and technique field by name, so extending a contract fails to compile until the new value is mapped, and an implementation that does not declare a feature is refused the program rather than left to render it without. Declaring a feature is a claim that the emitted document preserves the value, not that the planner parses it. The feature check is the outer gate; a concrete adapter still rejects program shapes its own emission cannot render.

Procedure implementation descriptors use stable absolute IRIs and declare the exact Procedure contract version, supported semantic operation IRIs, required fine-grained capability kinds, the program features realized for each declared operation, accepted control modes, document formats, and truthful services. Broad adapter capability declarations remain only a compatibility surface for operations that have not yet been normalized; they cannot authorize a normalized program.

`PipettingProgramV1` represents logical vessels, task inputs, material sources, products, exact volumes, ordered transfer, distribute, mix, and barrier steps, contamination-path policies, and optional environmental constraints. Golden Gate reaction setup and serial dilution both normalize to this contract. OT-2, Flex, and STAR consume the same programs but retain separate resource planners and emit their own reviewed formats.

`ThermalProgramV1` represents the exact task input transformed into an exact output, sample count and fill volume, a global heated-lid setpoint, ordered stages with repeats, exact block plateaus and durations, optional controlled ramp rates, and an optional indefinite final hold. Its contents derive an atomic block-control clause with exact temperature, capacity, and working-volume constraints; an optional heated-lid clause; and an optional controlled-ramp clause. Golden Gate cycling normalizes once to this contract. OT-2 and Flex lower it into their distinct Opentrons application formats, while the Inheco ODTC lowers it to the device-neutral reviewed thermal run document already accepted by Lab runtime.

## Consequences

- Facility capability offerings describe the concrete operations and quantitative envelopes an Asset can provide instead of merely claiming a broad device class.
- A Method author describes scientific work once; adapter implementations do not independently reconstruct that recipe from loosely related parameters.
- Planning can reject a device before lowering when any required fine-grained offering or typed bound is absent.
- Atomic multi-capability work remains on one Asset and implementation while independent Procedure tasks can still be composed across a facility.
- Reviewed plans attribute one child document to every capability binding it jointly realizes without dispatching the document more than once.
- Adding a new device in an existing class normally means implementing an existing Procedure contract. Adding genuinely new semantics requires a new backward-compatible contract version or a new contract rather than vendor conditionals in the shared IR.
- Coarse taxonomy offerings may coexist for discovery and unnormalized compatibility, but an implementation of a canonical contract must bind every fine-grained clause the program derives.

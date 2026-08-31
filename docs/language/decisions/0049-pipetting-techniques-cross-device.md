# 0049: Pipetting techniques are canonical constraints with calibrated device realizations

## Status

Accepted. Extends [0048: Canonical Procedure programs derive atomic capability formulas](0048-canonical-procedures-derive-capabilities.md) and [0030: Reviewed frames are the execution boundary](0030-reviewed-frames-are-the-execution-boundary.md).

## Context

Volumes and source/destination identities do not fully specify reliable liquid handling. Mature protocols also depend on when a source is mixed, how a tip may be reused, where aspiration and dispense occur relative to a vessel or liquid surface, whether an air gap is carried, and whether dispense is followed by blowout, touch-tip, or a bubble-clearing stroke. If these facts live only in an OT-2 Python template or STAR choreographer, two adapters silently implement different procedures and the reviewed compiler plan cannot explain the difference.

The exact numeric realization is not always portable. PUDU's optimized OT-2 plating code estimates the liquid height in a 15 mL conical tube and applies calibrated offsets and a low-volume fallback. The STAR planner uses measured per-labware volume-to-height models, liquid classes, and firmware coordinates. Requiring both devices to share one numeric formula would discard useful calibration and confuse a scientific requirement with a hardware implementation.

## Decision

Canonical pipetting programs state observable technique requirements in addition to liquid effects. Ordered steps express source and destination mixing explicitly. Transfer and distribute steps state fluid-path policy, aspiration strategy, dispense strategy, optional air-gap volume, and required post-dispense actions. Mix steps state their liquid-access strategy. Logical vessels and a validated per-position volume ledger make every operation's precondition and effect checkable before a device is selected.

Canonical strategies use physical meaning rather than vendor calls: ordinary in-liquid access, tracked-liquid-surface access, above-liquid dispense, fixed vessel-relative dispense, and material-surface spotting. They do not name `Well.bottom`, `Well.top`, Hamilton firmware coordinates, a pipette model, a deck slot, or a vendor liquid class.

An adapter Procedure implementation declares which canonical strategies it realizes. Its validated profile owns calibrated numeric data and labware geometry: submersion depth, bottom clearance, surface clearance, low-volume fallback, touch radius and speed, vendor-relative flow settings, liquid-class correction, and any permitted sensing overlay. The adapter planner combines the immutable canonical program, exact facility allocation, and profile into a fully numeric reviewed device plan. Runtime executes that plan without recomputing scientific intent or choosing a fallback strategy.

Volume tracking is deterministic compiler state. Source fills, retained volume, working capacity, every aspirate, every dispense, and every mix precondition are validated while planning.

A vessel states the volume each of its positions starts with. Only a material source may leave that open, because its adapter computes a load covering the planned withdrawals; a vessel the program fills itself starts empty, and a value arriving from an upstream task has a knowable volume that must be stated. An aspiration from a position whose volume the ledger cannot follow is rejected rather than exempted from checking, and following a falling liquid surface additionally requires a stated starting volume, since a surface the compiler cannot locate is not a plan.

A vessel may also state the volume the program must not draw below and the largest volume one position may hold. These are the Method's own bounds, such as leaving residual above a pellet. An adapter knows the labware it will use and enforces its own dead volume and capacity on top of them.

Hardware liquid-level detection may verify the planned state but may not replace it. A generated protocol may maintain the same frozen ledger to select already-reviewed positions at runtime when the vendor API requires the final coordinate calculation; the formula and all constants must be embedded in the reviewed child document and covered by adapter tests.

Flow rates that are expressed as fractions of one vendor pipette's maximum are implementation calibration, not canonical scientific quantities. A Method may require a physically meaningful flow or shear constraint when the science demands it. Until such a capability property is defined, calibrated relative aspiration and dispense rates belong to the adapter profile and are recorded in the device plan.

## Consequences

- PUDU's optimized transformation and plating behavior can be retained without importing PUDU classes or making the canonical Procedure contract OT-2-specific.
- OT-2, Flex, STAR, and future liquid handlers consume the same ordered liquid program while producing different numeric movement plans.
- Technique support becomes part of Procedure implementation matching; an adapter must reject a program containing a strategy it cannot faithfully realize.
- Review documents can distinguish biological recipe choices, portable technique requirements, facility allocation, and device calibration.
- Adapter defaults are versioned operational claims. Changing a calibrated offset, fallback, or vendor-relative flow rate changes the profile digest and therefore invalidates resume against an older reviewed plan.
- A compiler-generated volume ledger replaces template-local estimates and exposes source-loading and capacity errors before motion.

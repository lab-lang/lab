# lab-procedure

`lab-procedure` defines versioned, device-neutral operational programs shared by Method refinement, facility planning, adapter implementations, reviewed invocations, and Python authoring.

A scientific Method may describe work such as preparing a Golden Gate reaction or serially diluting a culture. Before facility allocation, Lab normalizes that domain operation into a canonical Procedure contract such as `PipettingProgramV1`. The normalized program states observable work and constraints: logical material and vessel relationships, exact quantities, ordered transfer and mixing effects, contamination boundaries, portable aspiration and dispense strategies, air gaps, blowout, touch-tip, and required environmental conditions. It contains no deck slots, carrier rails, pipette models, firmware commands, vendor liquid classes, calibrated hardware offsets, runtime endpoints, or facility identities.

Capability demands are derived from the normalized program rather than repeated manually by the Method author. Broad terms such as `LiquidHandling` may remain useful taxonomy parents, but they are not sufficient execution contracts. A pipetting program derives exact demands for metered liquid transfer, in-well mixing, temperature-controlled source staging, liquid-level-aware aspiration, vessel-relative access, air-gap handling, post-dispense blowout, and touch-tip when those operations occur. Its validator also constructs an exact liquid ledger, rejecting known source underflow and mixes larger than the liquid present while preserving deliberately open source-load calculations for the adapter.

Concrete adapters implement Procedure contracts. An Opentrons Flex and a Hamilton STAR can therefore prepare different device plans and emit different formats for the same immutable pipetting program while preserving the same semantic effects. Facility qualification remains attached to exact SBOLInventory CapabilityOfferings, and adapter support never promotes that qualification.

## Structure

The crate keeps a flat public API while organizing its implementation around semantic ownership:

- `pipetting/mod.rs` is the pipetting facade. Its private modules separate program, vessel, and operation contracts from structural validation, liquid-ledger replay, capability and feature derivation, and diagnostics.
- `thermal/mod.rs` is the thermal facade. Its private modules separate the serialized program from validation, capability derivation, and feature derivation.
- `quantity/mod.rs` is the quantity facade. Dimension-specific types share one exact-value implementation for parsing, bounds, units, and serialization.
- `feature/mod.rs` owns the common implementation-feature vocabulary, while each Procedure domain owns the exhaustive derivation of its required features.
- `program.rs` owns the open, versioned Procedure envelope and dispatches it into the typed contract validators.

These implementation modules remain private. Consumers continue to use the stable root exports from `lab_procedure`, and serialized Procedure shapes do not depend on the source layout.

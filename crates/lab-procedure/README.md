# lab-procedure

`lab-procedure` defines versioned, device-neutral operational programs shared by Method refinement, facility planning, adapter implementations, reviewed invocations, and Python authoring.

A scientific Method may describe work such as preparing a Golden Gate reaction or serially diluting a culture. Before facility allocation, Lab normalizes that domain operation into a canonical Procedure contract such as `PipettingProgramV1`. The normalized program states observable work and constraints: logical material and vessel relationships, exact quantities, transfer and mixing effects, ordering, contamination boundaries, and required environmental conditions. It contains no deck slots, carrier rails, pipette models, firmware commands, vendor liquid classes, runtime endpoints, or facility identities.

Capability demands are derived from the normalized program rather than repeated manually by the Method author. Broad terms such as `LiquidHandling` may remain useful taxonomy parents, but they are not sufficient execution contracts. A pipetting program currently derives exact demands for metered liquid transfer, in-well mixing, and temperature-controlled source staging when those operations occur.

Concrete adapters implement Procedure contracts. An Opentrons Flex and a Hamilton STAR can therefore prepare different device plans and emit different formats for the same immutable pipetting program while preserving the same semantic effects. Facility qualification remains attached to exact SBOLInventory CapabilityOfferings, and adapter support never promotes that qualification.

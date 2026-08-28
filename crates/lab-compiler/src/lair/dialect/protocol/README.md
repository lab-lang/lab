# Protocol dialect

The `protocol` dialect is LAIR's method-selected biological-procedure layer. Its operations describe what must happen to materials and what evidence must be produced, without prematurely embedding containers, inventory lots, schedules, locations, or instrument instructions.

The initial dialect contains operations for provision, synthesis, assembly, transformation, recovery, dilution, plating, selection, screening, growth, purification, sampling, sequencing, quantification, and acceptance. Its material-state types include `CircularDna`, `RecoveredCulture`, `DilutedCulture`, `SelectionPlate`, `ColonyPool`, `CloneCulture`, and `PurifiedPlasmid`; its evidence types represent sequence identity, concentration, and volume.

Protocol material values are affine: they may have at most one consumer. Branching physical matter requires an explicit operation such as `protocol.sample`, which returns a retained sample and a separate assay aliquot. Evidence values are information rather than matter and may be reused.

## Current boundary

This dialect represents method-selected biological procedures, not robot instructions. Source workflow structure lives in the preceding Workflow dialect; protocol selection retains only the material dataflow and policy required by the selected procedure. Inventory lots, containers, locations, schedules, device resources, and robot commands belong to facility allocation, adapter lowering, and runtime layers rather than fields collapsed into Protocol operations.

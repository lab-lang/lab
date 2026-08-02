# Protocol dialect

The `protocol` dialect is LAIR's target-selected biological-procedure layer. Its operations describe what must happen to materials and what evidence must be produced, without prematurely embedding containers, inventory lots, schedules, locations, or instrument instructions.

The initial dialect contains operations for provision, synthesis, assembly, transformation, recovery, selection, screening, growth, purification, sampling, sequencing, quantification, and acceptance. Its material-state types include `CircularDna`, `ColonyPool`, `CloneCulture`, and `PurifiedPlasmid`; its evidence types represent sequence identity, concentration, and volume.

Protocol material values are affine: they may have at most one consumer. Branching physical matter requires an explicit operation such as `protocol.sample`, which returns a retained sample and a separate assay aliquot. Evidence values are information rather than matter and may be reused.

## Current boundary

This dialect represents target-selected biological procedures, not robot instructions. Workflow structure, inventory lots, quantities, containers, locations, time, scheduling, and execution-target dialects belong to later LAIR lowering layers rather than fields collapsed into Protocol operations.

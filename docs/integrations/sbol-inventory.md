# SBOLInventory integration

Lab reads and validates the draft SBOLInventory Profile 0.2 with `sbol-inventory` from sbol-rs. The profile is under review in its initial specification PR.

Only Zones serialize `fac:facility`. Assets and MaterialLots use `fac:locatedIn` to reference a Zone or another Asset, and an Asset without a direct location can inherit membership through `fac:partOf`. The Rust views derive the governing facility from that path. An unlocated object has no known facility and cannot be selected by Lab's facility filter. Placement, position, lifecycle, composition, and occupancy constraints also apply.

For example, a sample may be located in a tube inside a rack inside a freezer in a storage Zone. Moving the rack changes the derived facility of its contained objects. A physical module that remains part of an instrument cannot have a conflicting known facility. Zones remain spatial or policy boundaries; containers remain Assets.

## Run provenance

A completed Lab run writes a separate inventory result graph from the frozen source snapshot. It records exact Asset and MaterialLot usages and each input or output design as a `fac:RunComponent` usage. Designs already present in the reviewed snapshot are informational inputs. Producing a physical MaterialLot does not regenerate its Component.

The run's ExperimentalData relates to those Components through `fac:forComponent`. ExperimentalData and Components reference the same hashed SBOL Attachment objects for the reviewed inputs and execution ledger. Simulation evidence remains explicitly labeled, and simulation does not create physical MaterialLots.

The profile also supports generating new Components using `prov:wasGeneratedBy`. Lab's runtime currently realizes outputs against existing reviewed Components; a future operation that creates a new design must record that separate creation truthfully.

## Repository links

The profile represents `fac:ExperimentalDataDatabase` and `fac:MetadataDatabase` as SBOL TopLevels. ExperimentalData can record `fac:submittedTo` a data repository; Components can record `fac:submittedTo` or `fac:retrievedFrom` a metadata repository. These references resolve to repository objects in the same SBOL document.

Lab preserves these links in the frozen source and resulting graph. They record completed transfers performed by a consuming application. This integration does not upload to Field Journal or SynBioHub, infer a successful submission, or store credentials.

## Preparing a package

1. Set `fac:facility` on Zones only.
2. Give inventory residents their actual Zone or container location. Preserve `partOf` for physical composition.
3. Build or plan the package with the compiler. Editing the catalog changes its digest, so prior reviewed plans remain bound to their original snapshot and must not be silently retargeted.
4. Review the resulting plan and its exact material and capability bindings before execution.

The bundled EBEF and Golden Gate catalogs use this draft profile. Validation includes the shared profile fixtures, nested containment and facility filtering, planner tests, and runtime provenance tests that check evidence links and preservation of retrieved-design metadata.

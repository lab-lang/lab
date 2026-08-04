# LAIR

LAIR—the Lab Automation Intermediate Representation—is the Lab ecosystem's multi-layer compiler IR. It is implemented as a family of Pliron dialects that preserve biological and physical meaning while programs are progressively lowered from artifact intent toward laboratory execution.

LAIR is currently internal to the `labc` package. This matches its present use: only compiler lowering, verification, and plan export consume these representations. It can be extracted into a crate later when an independent consumer requires a stable LAIR API.

Pliron is an implementation detail of this internal layer. Pliron contexts, modules, values, and pointers must not appear in `labc`'s public API. Public session and compilation APIs exchange Lab domain types, textual LAIR, plans, and compiler-owned errors; raw Pliron entities remain inside `compiler::ir` and the session/lowering adapters that operate on it.

## Initial dialects

The first vertical slice contains:

- the `design` dialect for declarative plasmid artifact values;
- the `protocol` dialect for target-selected provision, synthesis, assembly, transformation, recovery, selection, screening, growth, purification, sampling, sequencing, quantification, and acceptance;
- Protocol material-state types such as `CircularDna`, `ColonyPool`, `CloneCulture`, and `PurifiedPlasmid`;
- Protocol evidence types for sequence identity, concentration, and volume.

The dialects are layers within LAIR; no individual dialect is itself “the LAIR dialect.” Planned lower layers add workflow structure, resource binding and scheduling, and execution-target operations.

## Physical-resource rule

Protocol values representing physical material are affine: they may have at most one consumer. Branching physical matter requires an explicit operation such as `protocol.sample`, which returns a retained sample and a separate assay aliquot. Design and evidence values are information rather than matter and may be reused.

Pliron operation verifiers check only operation-local material states. The separate `MaterialLinearityAnalysis` follows SSA use lists across the complete module, and `protocol-check-material-linearity` makes that analysis a required pipeline gate. This separation keeps non-local reasoning out of operation verifiers.

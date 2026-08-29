# LAIR

LAIR—the Lab Automation Intermediate Representation—is the Lab ecosystem's multi-layer compiler IR. It is implemented as a family of Pliron dialects that preserve biological and physical meaning while programs are progressively lowered from artifact intent toward laboratory execution.

LAIR is currently maintained inside `lab-compiler`. Its active consumer is the `lab-opt` textual IR tool, which parses, verifies, transforms, and reprints LAIR modules. It can be extracted into a crate later when an independent consumer requires a stable LAIR API.

Pliron is an implementation detail of this layer. Pliron contexts, modules, values, and pointers do not cross the public session boundary; callers exchange textual LAIR, stage and pipeline descriptions, and compiler-owned errors. Raw Pliron entities remain inside the dialect, analysis, pipeline, stage, and session modules.

## Initial dialects

The first vertical slice contains:

- the `design` dialect for declarative plasmid artifact values;
- the `workflow` dialect for method-neutral realization, provision, transformation, recovery, dilution, and plating intent with typed material use-def edges;
- the `protocol` dialect for method-selected provision, synthesis, assembly, transformation, recovery, dilution, plating, selection, screening, growth, purification, sampling, sequencing, quantification, and acceptance;
- Protocol material-state types such as `CircularDna`, `ColonyPool`, `CloneCulture`, and `PurifiedPlasmid`;
- Protocol evidence types for sequence identity, concentration, and volume.

The dialects are layers within LAIR; no individual dialect is itself “the LAIR dialect.” Design and Workflow form the portable source-lowering boundary. A dialect conversion selects Protocol operations and eliminates Workflow operations while remaining independent of any facility. Planning then binds requirements to capability offerings and assets before an adapter lowers the bound protocol to device operations.

## Physical-resource rule

Protocol values representing physical material are affine: they may have at most one consumer. Branching physical matter requires an explicit operation such as `protocol.sample`, which returns a retained sample and a separate assay aliquot. Design and evidence values are information rather than matter and may be reused.

Pliron operation verifiers check only operation-local material states. The separate `MaterialLinearityAnalysis` follows SSA use lists across the complete module, and `protocol-check-material-linearity` makes that analysis a required pipeline gate. This separation keeps non-local reasoning out of operation verifiers.

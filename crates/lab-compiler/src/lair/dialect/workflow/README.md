# Workflow dialect

The `workflow` dialect is LAIR's method-neutral procedure-intent layer. It is emitted from checked Lab effects and preserves the source program's operation order through typed material use-def edges.

The current plasmid-build slice contains `workflow.realize`, `workflow.provision`, `workflow.transform`, `workflow.recover`, `workflow.dilute`, and `workflow.plate`. Build policy is owned by the operation to which it applies: assembly inputs and replicates are on `realize`, transformation replicates are on `transform`, serial-dilution count is on `dilute`, and plating selection and replicates are on `plate`. Cross-workflow artifact dependencies remain explicit identities on `realize` because workflow parameters represent materials supplied by another workflow invocation rather than SSA values produced in the same module body.

Workflow operations do not select a laboratory Method, inventory lot, offering, Asset, adapter, container, schedule, deck position, or robot command. Method refinement replaces every supported Workflow operation with candidate Procedure and Capability regions. Global facility planning selects one complete solution, and only verifier-valid Allocated Procedure LAIR may be projected into adapter invocations.

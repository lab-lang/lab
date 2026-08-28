# Workflow dialect

The `workflow` dialect is LAIR's method-neutral procedure-intent layer. It is emitted from checked Lab effects and preserves the source program's operation order through typed material use-def edges.

The current plasmid-build slice contains `workflow.realize`, `workflow.provision`, `workflow.transform`, `workflow.recover`, `workflow.dilute`, and `workflow.plate`. Build policy is owned by the operation to which it applies: assembly inputs and replicates are on `realize`, transformation replicates are on `transform`, serial-dilution count is on `dilute`, and plating selection and replicates are on `plate`. Cross-workflow artifact dependencies remain explicit identities on `realize` because workflow parameters represent materials supplied by another workflow invocation rather than SSA values produced in the same module body.

Workflow operations do not select a laboratory method, inventory lot, container, schedule, deck position, instrument, or robot command. A method-selection dialect conversion must replace every Workflow operation and material value with Protocol LAIR before the method-selected Protocol stage contract can pass.

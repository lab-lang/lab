# 0004: Portable module compilation boundary

Status: accepted, initial implementation

Complete Lab modules compile first to verified portable module IR. This boundary contains resolved imports, checked declaration properties, structured nominal and union types, typed expression trees, explicit workflow signatures and state, resolved operation identities, action capabilities and operand ownership modes, checked outcome constructors, control-flow continuation, and reactive handler structure.

Portable module IR is intentionally above laboratory target selection. Producing verified module IR means frontend type, return, and affine material-flow checks have passed for the supported control-flow subset. It does not mean that a workflow has been scheduled, that a target can lower every operation or property, or that any command has been durably dispatched. Those are later compiler and runtime boundaries and must have distinct outputs.

`CheckedModule` is the sole source-compilation boundary. Narrow specializations consume checked module IR and lower only the properties and resolved operations they support; output selection never switches to a different source frontend. Lower IRs must preserve the workflow, event, evidence, dependency, and material semantics required by their backend rather than reconstructing them from a compact compatibility model.

An execution specialization owns its lower target IRs and concrete emitters. The initial OT-2 path therefore lowers portable checked module IR into an `Ot2BuildIr`, then into a validated and resource-allocated `Ot2ExecutionPlan`; its Python, Markdown, and manifest outputs consume that execution plan. Hardware constants and rendering logic do not belong in the portable AST, checked IR, or generic output module. Dependency graph resolution remains separate target-neutral planning because it operates only on artifact edges, required materials, and inventory availability.

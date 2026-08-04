# 0004: Portable module compilation boundary

Status: accepted, initial implementation

Complete Lab modules compile first to verified portable module IR. This boundary
contains resolved imports, structured nominal and laboratory types, typed
expression trees, explicit workflow state, action capabilities and operand
ownership modes, checked outcome constructors, control-flow continuation, and
reactive handler structure.

Portable module IR is intentionally above laboratory target selection. Producing
it does not mean that a workflow has been scheduled, that material linearity has
been proven across every branch, or that any command has been durably dispatched.
Those are later compiler and runtime boundaries and must have distinct outputs.

The older `ArtifactSpec` to Design IR to Protocol IR pipeline remains the first
physically planned vertical slice. It is not used as a lossy container for richer
modules. The two paths may converge only when the lower IR can preserve workflow,
event, evidence, and material semantics.

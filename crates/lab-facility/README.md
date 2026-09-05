# Lab facility planning

`lab-facility` owns the facility-aware portion of compilation. It binds configured adapters to an exact inventory snapshot, derives MaterialLot evidence from checked declarations, solves one global LAIR planning problem, explains allocation failures, and constructs the reviewed facility execution plan.

The durable planning problem, policy and solution records, and allocated LAIR remain owned by `lab-compiler`. Exact MaterialLot evidence and its cross-validation against an allocation are owned here. Concrete adapter implementations and immutable adapter-invocation records are owned by `lab-adapters`; this crate joins them to facility inventory and planning evidence.

Planning, allocation cross-validation, and execution-plan construction all require the caller's exact `ProcedureContractRegistry`. `lab-facility` never chooses built-in Procedure semantics on an application's behalf.

The solver is a bounded feasibility and ambiguity checker over those owned records. It matches Methods, lots, offerings, Assets, adapter implementations, and exact profiles, retaining at most the alternatives needed to distinguish zero, one, or several complete plans. It does not schedule, batch, reserve, query live inventory, or optimize cost or time. [Decision 0055](../../docs/language/decisions/0055-solving-and-pliron-are-compiler-internals.md) records why this purpose-built search remains appropriate and what would justify replacing it.

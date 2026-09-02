# Lab facility planning

`lab-facility` owns the facility-aware portion of compilation. It binds configured adapters to an exact inventory snapshot, derives MaterialLot evidence from checked declarations, solves one global LAIR planning problem, explains allocation failures, and constructs the reviewed facility execution plan.

The durable planning problem, policy and solution records, and allocated LAIR remain owned by `lab-compiler`. Exact MaterialLot evidence and its cross-validation against an allocation are owned here. Concrete adapter implementations and immutable adapter-invocation records are owned by `lab-adapters`; this crate joins them to facility inventory and planning evidence.

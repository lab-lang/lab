# Lab adapters

`lab-adapters` owns the executable edge of the compiler: immutable adapter-invocation and schedule records, concrete adapter catalogs and profiles, device-specific lowering, and generated artifact bundles.

The crate consumes verifier-valid Allocated LAIR. It preserves the Method, task, requirement, Asset, and material identities already selected by facility planning; it does not solve facility constraints or infer new scientific choices. Applications remain responsible for persistence and execution policy.

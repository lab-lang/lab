# Lab adapter API

`lab-adapter-api` is the focused extension boundary for adapter authors. It owns descriptors, validated operational profiles, planning feasibility, immutable invocation plans, collision-checked artifact bundles, lowering results, and deterministic registry composition. It has no built-in device or runtime dependency.

Implement one `AdapterRegistration` with a profile validator, a pure Procedure-program feasibility callback, and an invocation lowerer. After parsing and semantic checks, the validator returns `canonical_adapter_profile(driver, name, &typed_profile)`. That API-owned helper stamps the Lab API version, canonicalizes TOML and JSON, and computes the digest. `AdapterRegistry` independently rechecks profile identity, both canonical forms, and the exact lowercase SHA-256 before planning or lowering can use it.

All Lab-owned types in those callback signatures are re-exported here. Import `PlanningProcedureTask`, `ProcedureContractRegistry`, `AdapterInvocationPlan`, `AdapterInvocation`, and `ProcedureImplementationDescriptor` directly from `lab_adapter_api`; do not add `lab-compiler` or `lab-capability` to an adapter crate. The same facade exposes allocated task records, capability identities, the open `ProcedureProgram`, and the typed pipetting and thermal V1 models needed to inspect a canonical program. This keeps compiler module ownership out of the contributor contract.

The callbacks should be ordinary named functions. `lab new adapter` generates their complete signatures, wires them into the registration, and includes a registry conformance test. Its placeholders fail closed until the adapter declares an exact Procedure implementation and implements profile-specific feasibility and lowering.

Runtime document loading and hardware execution are intentionally separate. Applications that need them use the optional integration traits and records in `lab-adapters`; an adapter that only plans and emits files depends on this crate alone.

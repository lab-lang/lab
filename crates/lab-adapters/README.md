# Lab adapters

`lab-adapters` supplies Lab's built-in device integrations, schedules, reviewed-document loaders, simulators, and optional live executors. The implementation-independent extension boundary lives in `lab-adapter-api`.

The crate consumes verifier-valid Allocated LAIR. It preserves the Method, task, requirement, Asset, and material identities already selected by facility planning; it does not solve facility constraints or infer new scientific choices. Applications remain responsible for persistence and execution policy.

One `lab_adapter_api::AdapterRegistration` composes an adapter's descriptor, profile validator, exact-profile feasibility check, and invocation lowerer without importing built-in devices, robot SDKs, or `lab-runtime`. Compatibility is the intersection of a canonical Procedure contract, its required feature set, exact facility CapabilityOfferings and control mode, and the validated selected profile. A task's descriptive biological operation IRI is evidence, never an adapter allowlist or dispatch key. Adapters consume owned invocation records and do not import Pliron.

Runtime is deliberately a separate optional layer. `AdapterRuntimeRegistrationExt` attaches reviewed-document loaders and executor factories to the same registration when an application links `lab-adapters`; planning and file-lowering integrations do not pay for those dependencies.

Invocation validation and lowering take the same explicit `ProcedureContractRegistry` used during compilation. The registry is threaded through structural projection and device lowering; adapter helpers do not silently substitute the built-in contracts.

Built-in liquid-handler profiles describe physical resources, not scientific workflows. OT-2 and Flex expose `resources.sources`, `resources.work`, `resources.small_tips`, and `resources.large_tips`; the work resource is the thermocycler-backed plate and Flex also declares its trash area. STAR exposes the same four resource roles alongside its carrier placements. Assembly, transformation, plating, DNA, agar, dilution, and media are Method or Procedure concerns and have no adapter-profile fields.

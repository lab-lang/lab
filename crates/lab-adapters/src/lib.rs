//! Concrete adapter contracts and device artifact lowering for Lab.
//!
//! LAIR owns the selected laboratory program and durable allocation records. This crate projects
//! that verifier-valid aggregate into immutable adapter invocations and schedules, validates
//! concrete adapter profiles, and emits reviewable device artifacts. It does not choose Methods,
//! Assets, material lots, or adapter bindings.

mod backend;
mod runtime;
mod schedule;

pub use backend::{
    ADAPTER_CATALOG_FORMAT, ADAPTER_PROFILE_SCHEMA_VERSION, AdapterCatalog, AdapterConstraintError,
    AdapterDescriptor, AdapterInvocationDocument, AdapterInvocationLowering, AdapterLoweringError,
    AdapterProfileContractError, AdapterRegistration, AdapterRegistry, AdapterServices,
    InvocationLowerer, ProcedureImplementationDescriptor, ProfileValidator, ProgramFeasibility,
    ValidatedAdapterProfile, adapter_catalog, builtin_adapter_registry, default_adapter_profile,
    validate_adapter_profile,
};
pub use backend::{hamilton, inheco, opentrons, run_sheet};
pub use lab_adapter_api::{
    ADAPTER_INVOCATIONS_SCHEMA_VERSION, AdapterInvocation, AdapterInvocationError,
    AdapterInvocationPlan, AdapterInvocationValidationError, ArtifactBundle, ArtifactError,
    GeneratedArtifact, adapter_invocation_id,
};
pub use runtime::{
    AdapterRuntimeRegistrationExt, AdapterRuntimeRegistryExt, BuiltLiveExecutor,
    LiveExecutorFactory, LiveExecutorFactoryConstructor, LiveExecutorFactoryRequest,
    ReviewedDocumentLoader, RuntimeDocumentRegistration, SimulationExecutorFactory,
};
pub use schedule::{
    ALLOCATED_PROCEDURE_SCHEDULE_SCHEMA_VERSION, AllocatedExecutionGroup,
    AllocatedProcedureSchedule, AllocatedProcedureScheduleError, ScheduledPhysicalLocation,
    ScheduledValueRef,
};

#[cfg(test)]
pub(crate) fn test_source_intent(operation: &str) -> lab_compiler::workflow::IntentAction {
    test_source_intent_with_ports(operation, &[], &[])
}

#[cfg(test)]
pub(crate) fn test_source_intent_with_ports(
    operation: &str,
    inputs: &[(String, lab_compiler::method::PortType)],
    outputs: &[(String, lab_compiler::method::PortType)],
) -> lab_compiler::workflow::IntentAction {
    use lab_compiler::method::PortType;
    use lab_compiler::workflow::{IntentAction, IntentSource};
    use lab_language::{
        CheckedActionArgument, CheckedActionResult, CheckedExpression, CheckedField, CheckedType,
        DefinitionId, OwnershipMode, ResolvedAction, ResolvedActionCallee, ResultLineage,
        TypedExpression,
    };

    fn checked_type(port_type: &PortType) -> CheckedType {
        match port_type {
            PortType::Design => CheckedType::Named {
                name: "SyntheticDesign".to_owned(),
                arguments: Vec::new(),
            },
            PortType::Material { state } => CheckedType::Named {
                name: "Material".to_owned(),
                arguments: vec![CheckedType::InState {
                    subject: Box::new(CheckedType::Named {
                        name: "SyntheticMaterial".to_owned(),
                        arguments: Vec::new(),
                    }),
                    state: state.to_string(),
                }],
            },
            PortType::Data { data_kind } => CheckedType::Named {
                name: data_kind.to_string(),
                arguments: Vec::new(),
            },
            PortType::MaterialAsRequested | PortType::MaterialAsSupplied => CheckedType::Named {
                name: "Material".to_owned(),
                arguments: Vec::new(),
            },
        }
    }

    let module = "compiler.synthetic";
    let arguments = inputs
        .iter()
        .map(|(name, port_type)| CheckedActionArgument {
            name: name.clone(),
            mode: if port_type.is_material() {
                OwnershipMode::Take
            } else {
                OwnershipMode::Copy
            },
            value: TypedExpression {
                r#type: checked_type(port_type),
                value: CheckedExpression::Reference {
                    definition: DefinitionId::exported(module, name),
                    path: vec![name.clone()],
                },
            },
        })
        .collect();
    let results = outputs
        .iter()
        .map(|(name, port_type)| CheckedActionResult {
            name: name.clone(),
            r#type: checked_type(port_type),
            lineage: ResultLineage::Begins,
        })
        .collect::<Vec<_>>();
    IntentAction {
        source: IntentSource {
            module: module.to_owned(),
            workflow: DefinitionId::exported(module, "synthetic"),
            statement_path: vec![0],
        },
        action: ResolvedAction {
            callee: ResolvedActionCallee::Action {
                definition: DefinitionId::exported(module, "synthetic"),
                operation: operation.to_owned(),
            },
            arguments,
            results: results.clone(),
        },
        ssa_operands: inputs.iter().map(|(name, _)| name.clone()).collect(),
        result_bindings: results
            .into_iter()
            .map(|result| CheckedField {
                name: result.name,
                r#type: result.r#type,
            })
            .collect(),
        artifact: None,
        artifact_dependencies: Vec::new(),
        parameters: Default::default(),
    }
}

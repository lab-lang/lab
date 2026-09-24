//! Facility evidence, allocation, and reviewed-plan construction for Lab.
//!
//! LAIR owns the planning problem and durable allocation contracts. This crate combines those
//! contracts with an exact facility inventory and configured adapters, solves the resulting
//! constraint problem, and projects allocated invocations into a reviewed execution plan.

mod adapters;
mod execution;
mod explain;
mod inventory;
mod provision;
mod solver;

pub use adapters::{
    ADAPTER_BINDINGS_SCHEMA_VERSION, AdapterBindingError, AdapterBindingRequest,
    AdapterBindingSnapshot, BoundCapabilityOffering, BoundCapabilityParameter,
    BoundCapabilityParameterValue, BoundProcedureImplementation, ResolvedAdapterBinding,
};
pub use execution::{
    ExecutionPlanBuildError, ExecutionPlanOptions, build_execution_plan_from_invocations,
    manual_run_steps,
};
pub use explain::explain_facility_planning_error;
pub use inventory::{
    AllocatedMaterialInventoryValidationError, MaterialLotCandidates, MaterialLotInventory,
    MaterialLotInventoryError, MaterialLotInventoryValidationError, MaterialStock,
    build_material_lot_inventory, validate_allocated_material_inventory,
};
pub use solver::{
    AlternativeMaterialBinding, AlternativeMethod, AlternativeRequirementBinding,
    FacilityPlanningError, PlanningAlternative, PlanningMaterialRejectionReason,
    RejectedMethodCandidate, RejectedPlanningMaterial, RejectedPlanningRequirement,
    solve_facility_planning,
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

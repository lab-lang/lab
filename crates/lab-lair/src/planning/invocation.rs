//! Immutable adapter invocations projected from one exact allocated Procedure program.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::allocation::{
    AllocatedMethod, AllocatedProcedureTask, AllocatedProgram, AllocatedProgramExtractionError,
    InvocationAdapter,
};
use crate::method::LocalId;
use crate::procedure::binding::{
    ProcedureBindingError, ProcedureCapabilityRequirement, ProcedureTaskInterface,
};
use crate::procedure::{
    BindingScope, ProcedureTaskProgramValidationError, ValidatedProcedureProgram,
    validate_task_program,
};
use lab_capability::{AbsoluteIri, ControlMode, PropertyConstraint, QualificationLevel};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{
    MaterialLotCandidates, MaterialLotInventory, MaterialLotInventoryValidationError,
    SelectedMaterialBinding, SelectedMaterialSource,
};

pub const ADAPTER_INVOCATIONS_SCHEMA_VERSION: &str = "lab.adapter-invocations.v1";

/// The complete, immutable backend-facing projection of an allocated Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterInvocationPlan {
    pub schema_version: String,
    pub problem_sha256: String,
    pub allocated_lair_sha256: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub material_inventory: MaterialLotInventory,
    pub methods: Vec<AllocatedMethod>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invocations: Vec<AdapterInvocation>,
}

/// One exact Asset/adapter invocation. Tasks and requirements refer to the semantic graph above;
/// an adapter never receives unresolved method alternatives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterInvocation {
    pub id: String,
    pub asset: String,
    pub adapter: InvocationAdapter,
    pub tasks: Vec<LocalId>,
    pub requirements: Vec<LocalId>,
}

impl AdapterInvocationPlan {
    /// Digest the canonical serde representation consumed by execution scheduling and adapters.
    pub fn sha256(&self) -> String {
        let bytes = serde_json::to_vec(self)
            .expect("AdapterInvocationPlan contains only infallibly serializable semantic values");
        hex_sha256(&bytes)
    }

    /// Project backend invocations from an exact semantic allocation.
    pub fn from_allocated(
        allocated: AllocatedProgram,
        allocated_lair_sha256: String,
        material_inventory: MaterialLotInventory,
    ) -> Result<Self, AdapterInvocationValidationError> {
        let AllocatedProgram {
            problem_sha256,
            inventory_sha256,
            facility,
            methods,
        } = allocated;
        let mut groups = BTreeMap::<(String, InvocationAdapter), InvocationMembers>::new();
        for method in &methods {
            for task in &method.tasks {
                for requirement in &task.requirements {
                    if let Some(adapter) = &requirement.adapter {
                        let members = groups
                            .entry((requirement.asset.clone(), adapter.clone()))
                            .or_default();
                        members.tasks.insert(task.id.clone());
                        members.requirements.insert(requirement.id.clone());
                    }
                }
            }
        }
        let invocations = groups
            .into_iter()
            .map(|((asset, adapter), members)| AdapterInvocation {
                id: adapter_invocation_id(&asset, &adapter),
                asset,
                adapter,
                tasks: members.tasks.into_iter().collect(),
                requirements: members.requirements.into_iter().collect(),
            })
            .collect();
        let plan = Self {
            schema_version: ADAPTER_INVOCATIONS_SCHEMA_VERSION.to_owned(),
            problem_sha256,
            allocated_lair_sha256,
            inventory_sha256,
            facility,
            material_inventory,
            methods,
            invocations,
        };
        plan.validate()?;
        Ok(plan)
    }

    /// Revalidate a deserialized invocation document before a backend consumes it.
    pub fn validate(&self) -> Result<(), AdapterInvocationValidationError> {
        if self.schema_version != ADAPTER_INVOCATIONS_SCHEMA_VERSION {
            return Err(AdapterInvocationValidationError::WrongSchema {
                found: self.schema_version.clone(),
            });
        }
        for (label, digest) in [
            ("planning problem", &self.problem_sha256),
            ("allocated LAIR", &self.allocated_lair_sha256),
            ("inventory", &self.inventory_sha256),
        ] {
            if !is_sha256(digest) {
                return Err(AdapterInvocationValidationError::InvalidDigest { label });
            }
        }
        if AbsoluteIri::new(&self.facility).is_err() {
            return Err(AdapterInvocationValidationError::InvalidFacility);
        }
        validate_material_inventory(self)?;
        if self.methods.is_empty() {
            return Err(AdapterInvocationValidationError::EmptyMethods);
        }
        let known_choices = self
            .methods
            .iter()
            .map(|method| method.choice.clone())
            .collect::<BTreeSet<_>>();
        let known_methods = self
            .methods
            .iter()
            .map(|method| (method.choice.clone(), method))
            .collect::<BTreeMap<_, _>>();
        let mut choices = BTreeSet::new();
        let mut tasks = BTreeSet::new();
        let mut parameters = BTreeSet::new();
        let mut materials = BTreeSet::new();
        let mut requirements = BTreeMap::new();
        for method in &self.methods {
            if !choices.insert(method.choice.clone()) {
                return Err(AdapterInvocationValidationError::DuplicateChoice {
                    choice: method.choice.clone(),
                });
            }
            if method.tasks.is_empty() {
                return Err(AdapterInvocationValidationError::EmptyMethod {
                    choice: method.choice.clone(),
                });
            }
            validate_allocated_method_graph(method, &known_methods)?;
            for task in &method.tasks {
                if !tasks.insert(task.id.clone()) {
                    return Err(AdapterInvocationValidationError::DuplicateTask {
                        task: task.id.clone(),
                    });
                }
                if task.requirements.is_empty() {
                    return Err(AdapterInvocationValidationError::EmptyTask {
                        task: task.id.clone(),
                    });
                }
                for parameter in &task.parameters {
                    if !parameters.insert(parameter.id.clone()) {
                        return Err(
                            AdapterInvocationValidationError::DuplicateProcedureParameter {
                                parameter: parameter.id.clone(),
                            },
                        );
                    }
                }
                let validated = validate_task_program(
                    &task.id,
                    &task.operation,
                    task.inputs.len(),
                    &task
                        .outputs
                        .iter()
                        .map(|output| output.name.clone())
                        .collect::<Vec<_>>(),
                    task.parameters
                        .iter()
                        .map(|parameter| (&parameter.id, &parameter.value)),
                    task.materials
                        .iter()
                        .map(|material| (&material.input, material.symbol.as_str())),
                    task.program.as_ref(),
                )?;
                if let Some(validated) = validated {
                    validate_program_contract(task, &validated)?;
                }
                for material in &task.materials {
                    if !materials.insert(material.input.clone()) {
                        return Err(AdapterInvocationValidationError::DuplicateMaterialInput {
                            input: material.input.clone(),
                        });
                    }
                    if !valid_material_binding(material, &known_choices, &self.material_inventory) {
                        return Err(AdapterInvocationValidationError::InvalidMaterialBinding {
                            input: material.input.clone(),
                        });
                    }
                }
                for requirement in &task.requirements {
                    if requirement.accepted_control_modes.is_empty()
                        || requirement
                            .accepted_control_modes
                            .contains(&ControlMode::Unspecified)
                    {
                        return Err(AdapterInvocationValidationError::InvalidControlPolicy {
                            requirement: requirement.id.clone(),
                        });
                    }
                    if AbsoluteIri::new(&requirement.offering).is_err()
                        || AbsoluteIri::new(&requirement.asset).is_err()
                    {
                        return Err(AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        });
                    }
                    let observed_qualification =
                        QualificationLevel::try_from(requirement.observed_qualification.as_str())
                            .map_err(|_| AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        })?;
                    let observed_control = ControlMode::try_from(requirement.control_mode.as_str())
                        .map_err(|_| AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        })?;
                    if !requirement
                        .minimum_qualification
                        .is_satisfied_by(observed_qualification)
                        || !requirement
                            .accepted_control_modes
                            .contains(&observed_control)
                    {
                        return Err(AdapterInvocationValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        });
                    }
                    let mut offering_parameters = BTreeSet::new();
                    for parameter in &requirement.parameters {
                        let constraint = PropertyConstraint {
                            property_kind: parameter.property_kind.clone(),
                            relation: parameter.relation,
                            required: parameter.required.clone(),
                        };
                        if AbsoluteIri::new(&parameter.offering_parameter).is_err()
                            || !offering_parameters.insert(parameter.offering_parameter.as_str())
                            || constraint.is_satisfied_by(&parameter.observed) != Ok(true)
                        {
                            return Err(
                                AdapterInvocationValidationError::InvalidParameterBinding {
                                    requirement: requirement.id.clone(),
                                    offering_parameter: parameter.offering_parameter.clone(),
                                },
                            );
                        }
                    }
                    let needs_implementation =
                        task.program.is_some() && requirement.adapter.is_some();
                    if requirement.procedure_implementation.is_some() != needs_implementation {
                        return Err(
                            AdapterInvocationValidationError::ProcedureImplementationBinding {
                                task: task.id.clone(),
                                requirement: requirement.id.clone(),
                            },
                        );
                    }
                    if requirements
                        .insert(requirement.id.clone(), (task.id.clone(), requirement))
                        .is_some()
                    {
                        return Err(AdapterInvocationValidationError::DuplicateRequirement {
                            requirement: requirement.id.clone(),
                        });
                    }
                }
            }
        }
        validate_allocated_method_dependencies(&self.methods)?;
        validate_allocated_material_linearity(&self.methods)?;

        let mut invocation_ids = BTreeSet::new();
        let mut invoked_requirements = BTreeSet::new();
        for invocation in &self.invocations {
            if invocation.id != adapter_invocation_id(&invocation.asset, &invocation.adapter) {
                return Err(AdapterInvocationValidationError::InvalidInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if !invocation_ids.insert(invocation.id.as_str()) {
                return Err(AdapterInvocationValidationError::DuplicateInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if AbsoluteIri::new(&invocation.asset).is_err()
                || invocation.adapter.driver.is_empty()
                || !is_relative_path(&invocation.adapter.profile_path)
                || invocation.adapter.profile_path.to_str().is_none()
                || !is_sha256(&invocation.adapter.profile_sha256)
                || invocation
                    .adapter
                    .features
                    .iter()
                    .any(|feature| feature.is_empty())
                || invocation
                    .adapter
                    .accepted_run_formats
                    .iter()
                    .chain(&invocation.adapter.emitted_run_formats)
                    .any(|format| format.is_empty())
            {
                return Err(AdapterInvocationValidationError::InvalidInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if invocation.requirements.is_empty() || invocation.tasks.is_empty() {
                return Err(AdapterInvocationValidationError::EmptyInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            let mut invocation_tasks = BTreeSet::new();
            for task in &invocation.tasks {
                if !invocation_tasks.insert(task) || !tasks.contains(task) {
                    return Err(AdapterInvocationValidationError::UnknownTask {
                        invocation: invocation.id.clone(),
                        task: task.clone(),
                    });
                }
            }
            let mut invocation_requirements = BTreeSet::new();
            let mut requirement_owners = BTreeSet::new();
            for requirement_id in &invocation.requirements {
                let Some((task, requirement)) = requirements.get(requirement_id) else {
                    return Err(AdapterInvocationValidationError::UnknownRequirement {
                        invocation: invocation.id.clone(),
                        requirement: requirement_id.clone(),
                    });
                };
                requirement_owners.insert(task);
                if !invocation_requirements.insert(requirement_id)
                    || !invocation.tasks.contains(task)
                    || requirement.asset != invocation.asset
                    || requirement.adapter.as_ref() != Some(&invocation.adapter)
                    || !invoked_requirements.insert(requirement_id.clone())
                {
                    return Err(AdapterInvocationValidationError::InvocationMismatch {
                        invocation: invocation.id.clone(),
                        requirement: requirement_id.clone(),
                    });
                }
            }
            if invocation_tasks != requirement_owners {
                return Err(
                    AdapterInvocationValidationError::InvocationTaskOwnershipMismatch {
                        invocation: invocation.id.clone(),
                    },
                );
            }
        }
        let expected = requirements
            .into_iter()
            .filter_map(|(id, (_, requirement))| requirement.adapter.is_some().then_some(id))
            .collect::<BTreeSet<_>>();
        if invoked_requirements != expected {
            return Err(AdapterInvocationValidationError::InvocationCoverage);
        }
        Ok(())
    }
}

fn validate_program_contract(
    task: &AllocatedProcedureTask,
    program: &ValidatedProcedureProgram,
) -> Result<(), AdapterInvocationValidationError> {
    let interface = ProcedureTaskInterface::new(
        task.inputs.len(),
        task.materials.iter().map(|material| material.input.clone()),
        task.outputs.iter().map(|output| output.name.clone()),
    );
    let requirements = task
        .requirements
        .iter()
        .map(|requirement| ProcedureCapabilityRequirement {
            id: requirement.id.clone(),
            capability_kind: requirement.capability_kind.clone(),
            minimum_qualification: requirement.minimum_qualification,
            accepted_control_modes: requirement.accepted_control_modes.clone(),
            constraints: requirement
                .parameters
                .iter()
                .map(|parameter| PropertyConstraint {
                    property_kind: parameter.property_kind.clone(),
                    relation: parameter.relation,
                    required: parameter.required.clone(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let contract = program
        .validate_task_contract(&task.id, &interface, None, &requirements)
        .map_err(|error| map_procedure_binding_error(&task.id, error))?;
    validate_atomic_program_binding(task, contract.capability_formula().binding_scope)
}

fn validate_atomic_program_binding(
    task: &AllocatedProcedureTask,
    scope: BindingScope,
) -> Result<(), AdapterInvocationValidationError> {
    if scope == BindingScope::AtomicAssetAssembly {
        let Some(first) = task.requirements.first() else {
            unreachable!("the Procedure task contract requires a non-empty capability formula")
        };
        if task.requirements.iter().any(|requirement| {
            requirement.asset != first.asset
                || requirement.adapter != first.adapter
                || requirement.procedure_implementation != first.procedure_implementation
        }) {
            return Err(
                AdapterInvocationValidationError::ProcedureCapabilityBindings {
                    task: task.id.clone(),
                },
            );
        }
    }
    Ok(())
}

fn map_procedure_binding_error(
    task: &LocalId,
    error: ProcedureBindingError,
) -> AdapterInvocationValidationError {
    match error {
        ProcedureBindingError::UnavailableInput { .. } => {
            AdapterInvocationValidationError::ProcedureInputBindings { task: task.clone() }
        }
        ProcedureBindingError::DuplicateTaskOutput
        | ProcedureBindingError::OutputMismatch { .. } => {
            AdapterInvocationValidationError::ProcedureOutputBindings { task: task.clone() }
        }
        ProcedureBindingError::DuplicateTaskMaterial
        | ProcedureBindingError::UndeclaredMaterial { .. }
        | ProcedureBindingError::UnexpectedThermalMaterials { .. } => {
            AdapterInvocationValidationError::ProcedureMaterialBindings { task: task.clone() }
        }
        ProcedureBindingError::BindingScopeMismatch { .. }
        | ProcedureBindingError::DuplicateRequirement { .. }
        | ProcedureBindingError::RequirementCount { .. }
        | ProcedureBindingError::MissingRequirement { .. }
        | ProcedureBindingError::RequirementKindMismatch { .. }
        | ProcedureBindingError::RequirementConstraintsMismatch { .. }
        | ProcedureBindingError::RequirementPolicyMismatch { .. } => {
            AdapterInvocationValidationError::ProcedureCapabilityBindings { task: task.clone() }
        }
    }
}

fn validate_allocated_method_graph(
    method: &AllocatedMethod,
    known_methods: &BTreeMap<LocalId, &AllocatedMethod>,
) -> Result<(), AdapterInvocationValidationError> {
    let invalid = |message: String| AdapterInvocationValidationError::InvalidMethodGraph {
        choice: method.choice.clone(),
        message,
    };
    let mut dependencies = BTreeSet::new();
    for dependency in &method.after {
        if dependency == &method.choice
            || !known_methods.contains_key(dependency)
            || !dependencies.insert(dependency)
        {
            return Err(invalid(format!(
                "completion dependency '{dependency}' is unknown, repeated, or self-referential"
            )));
        }
    }

    let mut inputs = BTreeMap::new();
    for input in &method.inputs {
        if inputs.insert(input.name.clone(), input).is_some() {
            return Err(invalid(format!("input '{}' is repeated", input.name)));
        }
        if let Some(source) = &input.source {
            let super::PlanningValueSource::ChoiceOutput { choice, output } = source else {
                return Err(invalid(format!(
                    "input '{}' has a non-choice output source",
                    input.name
                )));
            };
            let Some(producer) = known_methods.get(choice) else {
                return Err(invalid(format!(
                    "input '{}' references unknown choice '{choice}'",
                    input.name
                )));
            };
            if choice == &method.choice
                || !producer.outputs.iter().any(|candidate| {
                    &candidate.name == output && candidate.port_type == input.port_type
                })
            {
                return Err(invalid(format!(
                    "input '{}' references a missing or type-incompatible output '{choice}::{output}'",
                    input.name
                )));
            }
        }
    }
    let mut outputs = BTreeMap::new();
    for output in &method.outputs {
        if output.source.is_some() || outputs.insert(output.name.clone(), output).is_some() {
            return Err(invalid(format!(
                "output '{}' is repeated or carries an input-only source",
                output.name
            )));
        }
    }

    let mut available = inputs
        .values()
        .map(|input| {
            (
                super::PlanningValueSource::ChoiceInput {
                    input: input.name.clone(),
                },
                input.port_type.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut task_ids = BTreeSet::new();
    let mut material_uses = BTreeMap::<super::PlanningValueSource, usize>::new();
    for task in &method.tasks {
        if !task_ids.insert(task.id.clone()) {
            return Err(invalid(format!("task '{}' is repeated", task.id)));
        }
        for task_input in &task.inputs {
            match &task_input.source {
                super::PlanningValueSource::ChoiceInput { .. }
                | super::PlanningValueSource::TaskOutput { .. } => {}
                super::PlanningValueSource::ChoiceOutput { choice, output } => {
                    return Err(invalid(format!(
                        "task '{}' directly references choice output '{choice}::{output}' instead of a Method input",
                        task.id
                    )));
                }
            }
            if available.get(&task_input.source) != Some(&task_input.port_type) {
                return Err(invalid(format!(
                    "task '{}' references a value that is unavailable, forward-defined, or type-incompatible",
                    task.id
                )));
            }
            record_material_use(
                &mut material_uses,
                &task_input.source,
                &task_input.port_type,
            );
        }
        let mut output_names = BTreeSet::new();
        for output in &task.outputs {
            if !output_names.insert(output.name.clone()) {
                return Err(invalid(format!(
                    "task '{}' repeats output '{}'",
                    task.id, output.name
                )));
            }
            available.insert(
                super::PlanningValueSource::TaskOutput {
                    task: task.id.clone(),
                    output: output.name.clone(),
                },
                output.port_type.clone(),
            );
        }
    }

    let mut yields = BTreeSet::new();
    for method_yield in &method.yields {
        if !outputs.contains_key(&method_yield.output)
            || !yields.insert(method_yield.output.clone())
        {
            return Err(invalid(format!(
                "yield '{}' is unknown or repeated",
                method_yield.output
            )));
        }
        match &method_yield.source {
            super::PlanningValueSource::ChoiceInput { .. }
            | super::PlanningValueSource::TaskOutput { .. } => {}
            super::PlanningValueSource::ChoiceOutput { choice, output } => {
                return Err(invalid(format!(
                    "yield '{}' directly references choice output '{choice}::{output}' instead of a Method input",
                    method_yield.output
                )));
            }
        }
        let output_type = &outputs[&method_yield.output].port_type;
        if available.get(&method_yield.source) != Some(output_type) {
            return Err(invalid(format!(
                "yield '{}' references an unavailable or type-incompatible local value",
                method_yield.output
            )));
        }
        record_material_use(&mut material_uses, &method_yield.source, output_type);
    }
    if yields != outputs.into_keys().collect() {
        return Err(invalid(
            "selected Method yields do not cover every output exactly once".to_owned(),
        ));
    }
    if let Some((source, uses)) = material_uses.into_iter().find(|(_, uses)| *uses > 1) {
        return Err(invalid(format!(
            "physical material value {source:?} has {uses} consumers; use an explicit split or sample operation"
        )));
    }
    Ok(())
}

fn record_material_use(
    uses: &mut BTreeMap<super::PlanningValueSource, usize>,
    source: &super::PlanningValueSource,
    port_type: &crate::method::PortType,
) {
    if matches!(port_type, crate::method::PortType::Material { .. }) {
        *uses.entry(source.clone()).or_default() += 1;
    }
}

fn validate_allocated_method_dependencies(
    methods: &[AllocatedMethod],
) -> Result<(), AdapterInvocationValidationError> {
    let mut dependencies = methods
        .iter()
        .map(|method| {
            let mut dependencies = method.after.iter().collect::<BTreeSet<_>>();
            dependencies.extend(
                method
                    .inputs
                    .iter()
                    .filter_map(|input| match &input.source {
                        Some(super::PlanningValueSource::ChoiceOutput { choice, .. }) => {
                            Some(choice)
                        }
                        _ => None,
                    }),
            );
            dependencies.extend(method.yields.iter().filter_map(|method_yield| {
                match &method_yield.source {
                    super::PlanningValueSource::ChoiceOutput { choice, .. } => Some(choice),
                    _ => None,
                }
            }));
            dependencies.extend(method.tasks.iter().flat_map(|task| {
                task.materials
                    .iter()
                    .filter_map(|material| match &material.source {
                        SelectedMaterialSource::ChoiceOutput { choice } => Some(choice),
                        SelectedMaterialSource::MaterialLot { .. } => None,
                    })
            }));
            (method.choice.as_str(), dependencies)
        })
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    let mut indegree = BTreeMap::new();
    for (choice, choices) in &dependencies {
        indegree.insert(*choice, choices.len());
        for dependency in choices {
            dependents
                .entry(dependency.as_str())
                .or_default()
                .push(choice);
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(choice, degree)| (*degree == 0).then_some(*choice))
        .collect::<Vec<_>>();
    let mut visited = 0;
    while let Some(choice) = ready.pop() {
        visited += 1;
        for dependent in dependents.get(choice).into_iter().flatten() {
            let degree = indegree
                .get_mut(dependent)
                .expect("allocated Method dependencies were validated before cycle checking");
            *degree -= 1;
            if *degree == 0 {
                ready.push(dependent);
            }
        }
    }
    dependencies.clear();
    if visited != methods.len() {
        return Err(AdapterInvocationValidationError::MethodDependencyCycle);
    }
    Ok(())
}

fn validate_allocated_material_linearity(
    methods: &[AllocatedMethod],
) -> Result<(), AdapterInvocationValidationError> {
    let mut uses = BTreeMap::<(LocalId, LocalId), usize>::new();
    for method in methods {
        for input in &method.inputs {
            let Some(super::PlanningValueSource::ChoiceOutput { choice, output }) = &input.source
            else {
                continue;
            };
            if matches!(input.port_type, crate::method::PortType::Material { .. }) {
                *uses.entry((choice.clone(), output.clone())).or_default() += 1;
            }
        }
    }
    if let Some(((choice, output), uses)) = uses.into_iter().find(|(_, uses)| *uses > 1) {
        return Err(AdapterInvocationValidationError::MaterialLinearity {
            value: format!("{choice}::{output}"),
            uses,
        });
    }
    Ok(())
}

fn validate_material_inventory(
    plan: &AdapterInvocationPlan,
) -> Result<(), AdapterInvocationValidationError> {
    plan.material_inventory
        .validate()
        .map_err(AdapterInvocationValidationError::InvalidMaterialInventory)?;
    if plan.material_inventory.source_sha256() != plan.inventory_sha256
        || plan.material_inventory.facility() != plan.facility
    {
        return Err(AdapterInvocationValidationError::MaterialInventoryMismatch);
    }
    Ok(())
}

fn valid_material_binding(
    binding: &SelectedMaterialBinding,
    choices: &BTreeSet<LocalId>,
    inventory: &MaterialLotInventory,
) -> bool {
    if binding.symbol.is_empty() {
        return false;
    }
    let mut alternatives = BTreeSet::new();
    if binding
        .interchangeable_alternatives
        .iter()
        .any(|alternative| {
            AbsoluteIri::new(alternative).is_err() || !alternatives.insert(alternative.as_str())
        })
    {
        return false;
    }
    match &binding.source {
        SelectedMaterialSource::MaterialLot {
            component,
            material_lot,
        } => {
            let Some(MaterialLotCandidates::Identified {
                component: expected_component,
                material_lots,
            }) = inventory.candidates(&binding.symbol)
            else {
                return false;
            };
            let expected_alternatives = material_lots
                .iter()
                .filter_map(|candidate| (candidate != material_lot).then_some(candidate.as_str()))
                .collect::<BTreeSet<_>>();
            AbsoluteIri::new(component).is_ok()
                && AbsoluteIri::new(material_lot).is_ok()
                && component == expected_component
                && material_lots.contains(material_lot)
                && alternatives == expected_alternatives
        }
        SelectedMaterialSource::ChoiceOutput { choice } => {
            choices.contains(choice) && alternatives.is_empty()
        }
    }
}

#[derive(Default)]
struct InvocationMembers {
    tasks: BTreeSet<LocalId>,
    requirements: BTreeSet<LocalId>,
}

/// Derive the stable logical ID for an exact Asset and adapter binding.
pub fn adapter_invocation_id(asset: &str, adapter: &InvocationAdapter) -> String {
    let mut identity = Vec::new();
    append_identity_field(&mut identity, asset.as_bytes());
    append_identity_field(&mut identity, adapter.driver.as_bytes());
    append_identity_field(
        &mut identity,
        adapter.profile_path.as_os_str().as_encoded_bytes(),
    );
    append_identity_field(&mut identity, adapter.profile_sha256.as_bytes());
    for values in [
        &adapter.features,
        &adapter.accepted_run_formats,
        &adapter.emitted_run_formats,
    ] {
        append_identity_field(&mut identity, &(values.len() as u64).to_be_bytes());
        for value in values {
            append_identity_field(&mut identity, value.as_bytes());
        }
    }
    let digest = hex_sha256(&identity);
    format!("{}-{}", adapter.driver.replace('.', "-"), &digest[..12])
}

fn append_identity_field(identity: &mut Vec<u8>, field: &[u8]) {
    identity.extend_from_slice(&(field.len() as u64).to_be_bytes());
    identity.extend_from_slice(field);
}

pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

#[derive(Debug, Error)]
pub enum AdapterInvocationError {
    #[error(transparent)]
    InvalidAllocatedProgram(#[from] AllocatedProgramExtractionError),
    #[error(transparent)]
    InvalidProjection(#[from] AdapterInvocationValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AdapterInvocationValidationError {
    #[error(
        "adapter invocations declare schema `{found}`, expected `{ADAPTER_INVOCATIONS_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("adapter invocations contain an invalid {label} SHA-256 digest")]
    InvalidDigest { label: &'static str },
    #[error("adapter invocations name a facility that is not an absolute IRI")]
    InvalidFacility,
    #[error("adapter invocation material inventory does not match its inventory hash and facility")]
    MaterialInventoryMismatch,
    #[error("adapter invocation contains invalid material inventory: {0}")]
    InvalidMaterialInventory(#[source] MaterialLotInventoryValidationError),
    #[error("adapter invocations contain no selected Methods")]
    EmptyMethods,
    #[error("adapter invocations repeat Method choice `{choice}`")]
    DuplicateChoice { choice: LocalId },
    #[error("selected Method choice `{choice}` contains no Procedure tasks")]
    EmptyMethod { choice: LocalId },
    #[error("selected Method choice `{choice}` has an invalid value graph: {message}")]
    InvalidMethodGraph { choice: LocalId, message: String },
    #[error("selected Method choices contain a cyclic value or completion dependency")]
    MethodDependencyCycle,
    #[error("physical material value `{value}` has {uses} semantic consumers")]
    MaterialLinearity { value: String, uses: usize },
    #[error("adapter invocations repeat Procedure task `{task}`")]
    DuplicateTask { task: LocalId },
    #[error("Procedure task `{task}` contains no capability requirements")]
    EmptyTask { task: LocalId },
    #[error("adapter invocations repeat Procedure parameter `{parameter}`")]
    DuplicateProcedureParameter { parameter: LocalId },
    #[error(transparent)]
    InvalidProcedureTaskProgram(#[from] ProcedureTaskProgramValidationError),
    #[error("Procedure task `{task}` normalized program references an undeclared material input")]
    ProcedureMaterialBindings { task: LocalId },
    #[error("Procedure task `{task}` normalized program references an unavailable task input")]
    ProcedureInputBindings { task: LocalId },
    #[error("Procedure task `{task}` normalized program does not bind exactly its outputs")]
    ProcedureOutputBindings { task: LocalId },
    #[error(
        "Procedure task `{task}` does not preserve its normalized capability formula and atomic bindings"
    )]
    ProcedureCapabilityBindings { task: LocalId },
    #[error(
        "Procedure task `{task}` requirement `{requirement}` has an inconsistent Procedure implementation binding"
    )]
    ProcedureImplementationBinding { task: LocalId, requirement: LocalId },
    #[error("adapter invocations repeat Procedure material input `{input}`")]
    DuplicateMaterialInput { input: LocalId },
    #[error("Procedure material input `{input}` has an invalid physical source")]
    InvalidMaterialBinding { input: LocalId },
    #[error("adapter invocations repeat capability requirement `{requirement}`")]
    DuplicateRequirement { requirement: LocalId },
    #[error(
        "capability requirement `{requirement}` has an empty or non-operational control policy"
    )]
    InvalidControlPolicy { requirement: LocalId },
    #[error("capability requirement `{requirement}` has an invalid offering or Asset IRI")]
    InvalidBinding { requirement: LocalId },
    #[error(
        "capability requirement `{requirement}` has an invalid selected offering parameter `{offering_parameter}`"
    )]
    InvalidParameterBinding {
        requirement: LocalId,
        offering_parameter: String,
    },
    #[error("adapter invocation ID `{invocation}` is empty or repeated")]
    DuplicateInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` has invalid Asset, driver, profile, or digest data")]
    InvalidInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` contains no tasks or requirements")]
    EmptyInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` references unknown task `{task}`")]
    UnknownTask { invocation: String, task: LocalId },
    #[error("adapter invocation `{invocation}` references unknown requirement `{requirement}`")]
    UnknownRequirement {
        invocation: String,
        requirement: LocalId,
    },
    #[error("adapter invocation `{invocation}` does not match requirement `{requirement}`")]
    InvocationMismatch {
        invocation: String,
        requirement: LocalId,
    },
    #[error("adapter invocation `{invocation}` tasks do not exactly own its requirements")]
    InvocationTaskOwnershipMismatch { invocation: String },
    #[error("adapter invocations do not cover every and only adapter-bound requirement")]
    InvocationCoverage,
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use lab_capability::{
        AbsoluteIri, CapabilityKind, ConstraintRelation, ControlMode, ExactInteger, MethodId,
        OperationId, PropertyKind, PropertyValue, QualificationLevel, ScalarValue,
    };

    use super::*;
    use crate::allocation::{AllocatedProgram, AllocatedRequirementBinding, InvocationAdapter};
    use crate::method::{IntentOperationId, PortType, ProcedureValue};
    use crate::planning::{
        PlanningMethodYield, PlanningPort, PlanningProcedureParameter, PlanningTaskInput,
        PlanningTaskOutput, PlanningValueSource, SelectedCapabilityParameter,
        SelectedMaterialBinding, SelectedMaterialSource,
    };

    fn id(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn adapter() -> InvocationAdapter {
        InvocationAdapter {
            driver: "example.driver".to_owned(),
            profile_path: PathBuf::from("profiles/example.toml"),
            profile_sha256: "d".repeat(64),
            features: BTreeSet::from(["temperature-control".to_owned()]),
            accepted_run_formats: BTreeSet::from(["application/json".to_owned()]),
            emitted_run_formats: BTreeSet::from(["text/plain".to_owned()]),
        }
    }

    fn requirement(name: &str, adapter: Option<InvocationAdapter>) -> AllocatedRequirementBinding {
        AllocatedRequirementBinding {
            id: id(name),
            capability_kind: CapabilityKind::new("https://example.org/capability").unwrap(),
            minimum_qualification: QualificationLevel::Executable,
            accepted_control_modes: BTreeSet::from([ControlMode::Manual]),
            offering: format!("https://example.org/offering/{name}"),
            asset: "https://example.org/asset/instrument".to_owned(),
            observed_qualification: QualificationLevel::Executable.to_string(),
            control_mode: ControlMode::Manual.to_string(),
            parameters: Vec::new(),
            procedure_implementation: None,
            adapter,
        }
    }

    fn allocated_program() -> AllocatedProgram {
        let choice = id("choice");
        let task = id("choice::task");
        let input = id("input");
        let output = id("output");
        let task_output = id("task-output");
        AllocatedProgram {
            problem_sha256: "a".repeat(64),
            inventory_sha256: "b".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            methods: vec![AllocatedMethod {
                choice,
                source_operation: IntentOperationId::new("example.operation").unwrap(),
                method: MethodId::new("https://example.org/method").unwrap(),
                after: Vec::new(),
                inputs: vec![PlanningPort {
                    name: input.clone(),
                    port_type: PortType::Design,
                    source: None,
                }],
                outputs: vec![PlanningPort {
                    name: output.clone(),
                    port_type: PortType::Design,
                    source: None,
                }],
                yields: vec![PlanningMethodYield {
                    output,
                    source: PlanningValueSource::TaskOutput {
                        task: task.clone(),
                        output: task_output.clone(),
                    },
                }],
                tasks: vec![AllocatedProcedureTask {
                    id: task,
                    operation: OperationId::new("https://example.org/operation").unwrap(),
                    program: None,
                    inputs: vec![PlanningTaskInput {
                        source: PlanningValueSource::ChoiceInput { input },
                        port_type: PortType::Design,
                    }],
                    outputs: vec![PlanningTaskOutput {
                        name: task_output,
                        port_type: PortType::Design,
                    }],
                    parameters: Vec::new(),
                    materials: Vec::new(),
                    requirements: vec![requirement("choice::requirement", Some(adapter()))],
                }],
            }],
        }
    }

    fn inventory() -> MaterialLotInventory {
        MaterialLotInventory::new(
            "b".repeat(64),
            "https://example.org/facility",
            BTreeMap::new(),
            BTreeMap::new(),
        )
    }

    fn valid_plan() -> AdapterInvocationPlan {
        AdapterInvocationPlan::from_allocated(allocated_program(), "c".repeat(64), inventory())
            .unwrap()
    }

    #[test]
    fn adapter_invocations_revalidate_registered_task_program_provenance() {
        let mut plan = valid_plan();
        plan.validate().unwrap();

        plan.methods[0].tasks[0].operation =
            OperationId::new(crate::procedure::vocabulary::PLATE_DILUTED_CULTURE).unwrap();
        assert!(matches!(
            plan.validate(),
            Err(
                AdapterInvocationValidationError::InvalidProcedureTaskProgram(
                    ProcedureTaskProgramValidationError::CannotNormalize(_)
                )
            )
        ));
    }

    fn selected_parameter(observed: i64) -> SelectedCapabilityParameter {
        SelectedCapabilityParameter {
            property_kind: PropertyKind::new("https://example.org/property/count").unwrap(),
            relation: ConstraintRelation::AtLeast,
            required: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse("2").unwrap(),
            )),
            offering_parameter: "https://example.org/offering-parameter/count".to_owned(),
            observed: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse(observed.to_string()).unwrap(),
            )),
        }
    }

    #[test]
    fn task_values_are_available_only_incrementally_and_never_directly_from_other_choices() {
        let mut self_reference = valid_plan();
        let task = self_reference.methods[0].tasks[0].id.clone();
        let output = self_reference.methods[0].tasks[0].outputs[0].name.clone();
        self_reference.methods[0].tasks[0].inputs[0].source =
            PlanningValueSource::TaskOutput { task, output };
        assert!(matches!(
            self_reference.validate(),
            Err(AdapterInvocationValidationError::InvalidMethodGraph { .. })
        ));

        let mut direct_choice = valid_plan();
        direct_choice.methods[0].tasks[0].inputs[0].source = PlanningValueSource::ChoiceOutput {
            choice: id("choice"),
            output: id("output"),
        };
        assert!(matches!(
            direct_choice.validate(),
            Err(AdapterInvocationValidationError::InvalidMethodGraph { .. })
        ));
    }

    #[test]
    fn task_outputs_are_unique_and_method_yields_use_only_local_values() {
        let mut duplicate_output = valid_plan();
        let duplicate = duplicate_output.methods[0].tasks[0].outputs[0].clone();
        duplicate_output.methods[0].tasks[0].outputs.push(duplicate);
        assert!(matches!(
            duplicate_output.validate(),
            Err(AdapterInvocationValidationError::InvalidMethodGraph { .. })
        ));

        let mut direct_choice = valid_plan();
        direct_choice.methods[0].yields[0].source = PlanningValueSource::ChoiceOutput {
            choice: id("choice"),
            output: id("output"),
        };
        assert!(matches!(
            direct_choice.validate(),
            Err(AdapterInvocationValidationError::InvalidMethodGraph { .. })
        ));
    }

    #[test]
    fn selected_capability_parameters_and_control_policies_are_revalidated() {
        let mut policy = valid_plan();
        policy.methods[0].tasks[0].requirements[0].accepted_control_modes =
            BTreeSet::from([ControlMode::Unspecified]);
        assert!(matches!(
            policy.validate(),
            Err(AdapterInvocationValidationError::InvalidControlPolicy { .. })
        ));

        let mut observation = valid_plan();
        observation.methods[0].tasks[0].requirements[0].parameters = vec![selected_parameter(1)];
        assert!(matches!(
            observation.validate(),
            Err(AdapterInvocationValidationError::InvalidParameterBinding { .. })
        ));

        let mut duplicate = valid_plan();
        let parameter = selected_parameter(2);
        duplicate.methods[0].tasks[0].requirements[0].parameters =
            vec![parameter.clone(), parameter];
        assert!(matches!(
            duplicate.validate(),
            Err(AdapterInvocationValidationError::InvalidParameterBinding { .. })
        ));
    }

    #[test]
    fn procedure_parameter_ids_are_global() {
        let mut plan = valid_plan();
        let parameter = PlanningProcedureParameter {
            id: id("choice::parameter"),
            property_kind: PropertyKind::new("https://example.org/property/count").unwrap(),
            value: ProcedureValue::Scalar {
                value: PropertyValue::unitless(ScalarValue::Integer(
                    ExactInteger::parse("1").unwrap(),
                )),
            },
        };
        plan.methods[0].tasks[0].parameters = vec![parameter.clone(), parameter];
        assert!(matches!(
            plan.validate(),
            Err(AdapterInvocationValidationError::DuplicateProcedureParameter { .. })
        ));
    }

    #[test]
    fn material_alternatives_are_exact_inventory_lots() {
        let mut plan = valid_plan();
        let selected = "https://example.org/lot/a".to_owned();
        let alternative = "https://example.org/lot/b".to_owned();
        let mut materials = plan.material_inventory.materials().clone();
        materials.insert(
            "sample".to_owned(),
            MaterialLotCandidates::Identified {
                component: "https://example.org/component/sample".to_owned(),
                material_lots: vec![selected.clone(), alternative.clone()],
            },
        );
        plan.material_inventory = MaterialLotInventory::new(
            plan.material_inventory.source_sha256(),
            plan.material_inventory.facility(),
            materials,
            plan.material_inventory.artifacts().clone(),
        );
        plan.methods[0].tasks[0]
            .materials
            .push(SelectedMaterialBinding {
                input: id("choice::material"),
                symbol: "sample".to_owned(),
                source: SelectedMaterialSource::MaterialLot {
                    component: "https://example.org/component/sample".to_owned(),
                    material_lot: selected.clone(),
                },
                interchangeable_alternatives: vec![alternative],
            });
        plan.validate().unwrap();

        plan.methods[0].tasks[0].materials[0].interchangeable_alternatives = vec![selected];
        assert!(matches!(
            plan.validate(),
            Err(AdapterInvocationValidationError::InvalidMaterialBinding { .. })
        ));
    }

    #[test]
    fn invocation_tasks_are_exactly_the_requirement_owners() {
        let mut plan = valid_plan();
        let extra = id("choice::manual-task");
        plan.methods[0].tasks.push(AllocatedProcedureTask {
            id: extra.clone(),
            operation: OperationId::new("https://example.org/manual-operation").unwrap(),
            program: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            parameters: Vec::new(),
            materials: Vec::new(),
            requirements: vec![requirement("choice::manual-requirement", None)],
        });
        plan.invocations[0].tasks.push(extra);
        assert!(matches!(
            plan.validate(),
            Err(AdapterInvocationValidationError::InvocationTaskOwnershipMismatch { .. })
        ));
    }

    #[test]
    fn material_values_are_affine_in_the_semantic_graph() {
        let mut plan = valid_plan();
        let material = PortType::Material {
            state: AbsoluteIri::new("https://example.org/material/sample").unwrap(),
        };
        plan.methods[0].inputs[0].port_type = material.clone();
        plan.methods[0].tasks[0].inputs[0].port_type = material;
        let duplicate = plan.methods[0].tasks[0].inputs[0].clone();
        plan.methods[0].tasks[0].inputs.push(duplicate);
        assert!(matches!(
            plan.validate(),
            Err(AdapterInvocationValidationError::InvalidMethodGraph { .. })
        ));
    }

    #[test]
    fn material_method_outputs_have_at_most_one_downstream_consumer() {
        let mut allocated = allocated_program();
        let material = PortType::Material {
            state: AbsoluteIri::new("https://example.org/material/sample").unwrap(),
        };
        allocated.methods[0].outputs[0].port_type = material.clone();
        allocated.methods[0].tasks[0].outputs[0].port_type = material.clone();

        for suffix in ["first", "second"] {
            let choice = id(&format!("consumer-{suffix}"));
            let task = id(&format!("consumer-{suffix}::task"));
            let input = id("input");
            allocated.methods.push(AllocatedMethod {
                choice,
                source_operation: IntentOperationId::new(format!("example.consume.{suffix}"))
                    .unwrap(),
                method: MethodId::new(format!("https://example.org/method/consume/{suffix}"))
                    .unwrap(),
                after: Vec::new(),
                inputs: vec![PlanningPort {
                    name: input.clone(),
                    port_type: material.clone(),
                    source: Some(PlanningValueSource::ChoiceOutput {
                        choice: id("choice"),
                        output: id("output"),
                    }),
                }],
                outputs: Vec::new(),
                yields: Vec::new(),
                tasks: vec![AllocatedProcedureTask {
                    id: task,
                    operation: OperationId::new(format!(
                        "https://example.org/operation/consume/{suffix}"
                    ))
                    .unwrap(),
                    program: None,
                    inputs: vec![PlanningTaskInput {
                        source: PlanningValueSource::ChoiceInput { input },
                        port_type: material.clone(),
                    }],
                    outputs: Vec::new(),
                    parameters: Vec::new(),
                    materials: Vec::new(),
                    requirements: vec![requirement(
                        &format!("consumer-{suffix}::requirement"),
                        None,
                    )],
                }],
            });
        }

        assert!(matches!(
            AdapterInvocationPlan::from_allocated(allocated, "c".repeat(64), inventory()),
            Err(AdapterInvocationValidationError::MaterialLinearity { uses: 2, .. })
        ));
    }

    #[test]
    fn invocation_identity_covers_every_adapter_grouping_field() {
        let original = adapter();
        let original_id = adapter_invocation_id("https://example.org/asset", &original);
        for changed in [
            {
                let mut changed = original.clone();
                changed.features.insert("new-feature".to_owned());
                changed
            },
            {
                let mut changed = original.clone();
                changed
                    .accepted_run_formats
                    .insert("application/xml".to_owned());
                changed
            },
            {
                let mut changed = original.clone();
                changed
                    .emitted_run_formats
                    .insert("application/yaml".to_owned());
                changed
            },
        ] {
            assert_ne!(
                adapter_invocation_id("https://example.org/asset", &changed),
                original_id
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_adapter_profile_paths_are_rejected_without_panicking() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let mut plan = valid_plan();
        plan.invocations[0].adapter.profile_path =
            PathBuf::from(OsString::from_vec(b"profiles/\xff.toml".to_vec()));
        plan.invocations[0].id =
            adapter_invocation_id(&plan.invocations[0].asset, &plan.invocations[0].adapter);
        assert!(matches!(
            plan.validate(),
            Err(AdapterInvocationValidationError::InvalidInvocation { .. })
        ));
    }
}

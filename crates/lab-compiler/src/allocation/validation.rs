//! Validation of facility-bound semantic allocations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use lab_capability::{AbsoluteIri, ControlMode, PropertyConstraint, QualificationLevel};
use thiserror::Error;

use super::{AllocatedMethod, AllocatedProcedureTask, AllocatedProgram, InvocationAdapter};
use crate::method::{LocalId, PortType};
use crate::planning::{PlanningValueSource, SelectedMaterialBinding, SelectedMaterialSource};
use crate::procedure::binding::{
    ProcedureBindingError, ProcedureCapabilityRequirement, ProcedureTaskInterface,
};
use crate::procedure::{
    BindingScope, ProcedureTaskProgramValidationError, ValidatedProcedureProgram,
    validate_task_program,
};

impl AllocatedProgram {
    /// Validate the complete facility-bound semantic aggregate without external inventory evidence.
    pub fn validate(&self) -> Result<(), AllocatedProgramValidationError> {
        for (label, digest) in [
            ("planning problem", &self.problem_sha256),
            ("inventory", &self.inventory_sha256),
        ] {
            if !is_sha256(digest) {
                return Err(AllocatedProgramValidationError::InvalidDigest { label });
            }
        }
        if AbsoluteIri::new(&self.facility).is_err() {
            return Err(AllocatedProgramValidationError::InvalidFacility);
        }
        if self.methods.is_empty() {
            return Err(AllocatedProgramValidationError::EmptyMethods);
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
        let mut requirements = BTreeSet::new();
        for method in &self.methods {
            if !choices.insert(method.choice.clone()) {
                return Err(AllocatedProgramValidationError::DuplicateChoice {
                    choice: method.choice.clone(),
                });
            }
            if method.tasks.is_empty() {
                return Err(AllocatedProgramValidationError::EmptyMethod {
                    choice: method.choice.clone(),
                });
            }
            validate_allocated_method_graph(method, &known_methods)?;
            for task in &method.tasks {
                if !tasks.insert(task.id.clone()) {
                    return Err(AllocatedProgramValidationError::DuplicateTask {
                        task: task.id.clone(),
                    });
                }
                if task.requirements.is_empty() {
                    return Err(AllocatedProgramValidationError::EmptyTask {
                        task: task.id.clone(),
                    });
                }
                for parameter in &task.parameters {
                    if !parameters.insert(parameter.id.clone()) {
                        return Err(
                            AllocatedProgramValidationError::DuplicateProcedureParameter {
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
                        return Err(AllocatedProgramValidationError::DuplicateMaterialInput {
                            input: material.input.clone(),
                        });
                    }
                    if !valid_material_binding(material, &known_choices) {
                        return Err(AllocatedProgramValidationError::InvalidMaterialBinding {
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
                        return Err(AllocatedProgramValidationError::InvalidControlPolicy {
                            requirement: requirement.id.clone(),
                        });
                    }
                    if AbsoluteIri::new(&requirement.offering).is_err()
                        || AbsoluteIri::new(&requirement.asset).is_err()
                    {
                        return Err(AllocatedProgramValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        });
                    }
                    let observed_qualification =
                        QualificationLevel::try_from(requirement.observed_qualification.as_str())
                            .map_err(|_| AllocatedProgramValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        })?;
                    let observed_control = ControlMode::try_from(requirement.control_mode.as_str())
                        .map_err(|_| AllocatedProgramValidationError::InvalidBinding {
                            requirement: requirement.id.clone(),
                        })?;
                    if !requirement
                        .minimum_qualification
                        .is_satisfied_by(observed_qualification)
                        || !requirement
                            .accepted_control_modes
                            .contains(&observed_control)
                    {
                        return Err(AllocatedProgramValidationError::InvalidBinding {
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
                            return Err(AllocatedProgramValidationError::InvalidParameterBinding {
                                requirement: requirement.id.clone(),
                                offering_parameter: parameter.offering_parameter.clone(),
                            });
                        }
                    }
                    let needs_implementation =
                        task.program.is_some() && requirement.adapter.is_some();
                    if requirement.procedure_implementation.is_some() != needs_implementation {
                        return Err(
                            AllocatedProgramValidationError::ProcedureImplementationBinding {
                                task: task.id.clone(),
                                requirement: requirement.id.clone(),
                            },
                        );
                    }
                    if requirement
                        .adapter
                        .as_ref()
                        .is_some_and(|adapter| !valid_invocation_adapter(adapter))
                    {
                        return Err(AllocatedProgramValidationError::InvalidAdapterBinding {
                            requirement: requirement.id.clone(),
                        });
                    }
                    if !requirements.insert(requirement.id.clone()) {
                        return Err(AllocatedProgramValidationError::DuplicateRequirement {
                            requirement: requirement.id.clone(),
                        });
                    }
                }
            }
        }
        validate_allocated_method_dependencies(&self.methods)?;
        validate_allocated_material_linearity(&self.methods)?;
        Ok(())
    }
}

fn validate_program_contract(
    task: &AllocatedProcedureTask,
    program: &ValidatedProcedureProgram,
) -> Result<(), AllocatedProgramValidationError> {
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
) -> Result<(), AllocatedProgramValidationError> {
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
                AllocatedProgramValidationError::ProcedureCapabilityBindings {
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
) -> AllocatedProgramValidationError {
    match error {
        ProcedureBindingError::UnavailableInput { .. } => {
            AllocatedProgramValidationError::ProcedureInputBindings { task: task.clone() }
        }
        ProcedureBindingError::DuplicateTaskOutput
        | ProcedureBindingError::OutputMismatch { .. } => {
            AllocatedProgramValidationError::ProcedureOutputBindings { task: task.clone() }
        }
        ProcedureBindingError::DuplicateTaskMaterial
        | ProcedureBindingError::UndeclaredMaterial { .. }
        | ProcedureBindingError::UnexpectedThermalMaterials { .. } => {
            AllocatedProgramValidationError::ProcedureMaterialBindings { task: task.clone() }
        }
        ProcedureBindingError::BindingScopeMismatch { .. }
        | ProcedureBindingError::DuplicateRequirement { .. }
        | ProcedureBindingError::RequirementCount { .. }
        | ProcedureBindingError::MissingRequirement { .. }
        | ProcedureBindingError::RequirementKindMismatch { .. }
        | ProcedureBindingError::RequirementConstraintsMismatch { .. }
        | ProcedureBindingError::RequirementPolicyMismatch { .. } => {
            AllocatedProgramValidationError::ProcedureCapabilityBindings { task: task.clone() }
        }
    }
}

fn validate_allocated_method_graph(
    method: &AllocatedMethod,
    known_methods: &BTreeMap<LocalId, &AllocatedMethod>,
) -> Result<(), AllocatedProgramValidationError> {
    let invalid = |message: String| AllocatedProgramValidationError::InvalidMethodGraph {
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
            let PlanningValueSource::ChoiceOutput { choice, output } = source else {
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
                PlanningValueSource::ChoiceInput {
                    input: input.name.clone(),
                },
                input.port_type.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut task_ids = BTreeSet::new();
    let mut material_uses = BTreeMap::<PlanningValueSource, usize>::new();
    for task in &method.tasks {
        if !task_ids.insert(task.id.clone()) {
            return Err(invalid(format!("task '{}' is repeated", task.id)));
        }
        for task_input in &task.inputs {
            match &task_input.source {
                PlanningValueSource::ChoiceInput { .. }
                | PlanningValueSource::TaskOutput { .. } => {}
                PlanningValueSource::ChoiceOutput { choice, output } => {
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
                PlanningValueSource::TaskOutput {
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
            PlanningValueSource::ChoiceInput { .. } | PlanningValueSource::TaskOutput { .. } => {}
            PlanningValueSource::ChoiceOutput { choice, output } => {
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
    uses: &mut BTreeMap<PlanningValueSource, usize>,
    source: &PlanningValueSource,
    port_type: &PortType,
) {
    if matches!(port_type, PortType::Material { .. }) {
        *uses.entry(source.clone()).or_default() += 1;
    }
}

fn validate_allocated_method_dependencies(
    methods: &[AllocatedMethod],
) -> Result<(), AllocatedProgramValidationError> {
    let mut dependencies = methods
        .iter()
        .map(|method| {
            let mut dependencies = method.after.iter().collect::<BTreeSet<_>>();
            dependencies.extend(
                method
                    .inputs
                    .iter()
                    .filter_map(|input| match &input.source {
                        Some(PlanningValueSource::ChoiceOutput { choice, .. }) => Some(choice),
                        _ => None,
                    }),
            );
            dependencies.extend(method.yields.iter().filter_map(|method_yield| {
                match &method_yield.source {
                    PlanningValueSource::ChoiceOutput { choice, .. } => Some(choice),
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
        return Err(AllocatedProgramValidationError::MethodDependencyCycle);
    }
    Ok(())
}

fn validate_allocated_material_linearity(
    methods: &[AllocatedMethod],
) -> Result<(), AllocatedProgramValidationError> {
    let mut uses = BTreeMap::<(LocalId, LocalId), usize>::new();
    for method in methods {
        for input in &method.inputs {
            let Some(PlanningValueSource::ChoiceOutput { choice, output }) = &input.source else {
                continue;
            };
            if matches!(input.port_type, PortType::Material { .. }) {
                *uses.entry((choice.clone(), output.clone())).or_default() += 1;
            }
        }
    }
    if let Some(((choice, output), uses)) = uses.into_iter().find(|(_, uses)| *uses > 1) {
        return Err(AllocatedProgramValidationError::MaterialLinearity {
            value: format!("{choice}::{output}"),
            uses,
        });
    }
    Ok(())
}

fn valid_material_binding(binding: &SelectedMaterialBinding, choices: &BTreeSet<LocalId>) -> bool {
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
            AbsoluteIri::new(component).is_ok()
                && AbsoluteIri::new(material_lot).is_ok()
                && !alternatives.contains(material_lot.as_str())
        }
        SelectedMaterialSource::ChoiceOutput { choice } => {
            choices.contains(choice) && alternatives.is_empty()
        }
    }
}

fn valid_invocation_adapter(adapter: &InvocationAdapter) -> bool {
    !adapter.driver.is_empty()
        && is_relative_path(&adapter.profile_path)
        && adapter.profile_path.to_str().is_some()
        && is_sha256(&adapter.profile_sha256)
        && adapter
            .features
            .iter()
            .chain(&adapter.accepted_run_formats)
            .chain(&adapter.emitted_run_formats)
            .all(|value| !value.is_empty())
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

/// A violation of the facility-bound semantic allocation contract.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AllocatedProgramValidationError {
    #[error("allocated program contains an invalid {label} SHA-256 digest")]
    InvalidDigest { label: &'static str },
    #[error("allocated program names a facility that is not an absolute IRI")]
    InvalidFacility,
    #[error("allocated program contains no selected Methods")]
    EmptyMethods,
    #[error("allocated program repeats Method choice `{choice}`")]
    DuplicateChoice { choice: LocalId },
    #[error("selected Method choice `{choice}` contains no Procedure tasks")]
    EmptyMethod { choice: LocalId },
    #[error("selected Method choice `{choice}` has an invalid value graph: {message}")]
    InvalidMethodGraph { choice: LocalId, message: String },
    #[error("selected Method choices contain a cyclic value or completion dependency")]
    MethodDependencyCycle,
    #[error("physical material value `{value}` has {uses} semantic consumers")]
    MaterialLinearity { value: String, uses: usize },
    #[error("allocated program repeats Procedure task `{task}`")]
    DuplicateTask { task: LocalId },
    #[error("Procedure task `{task}` contains no capability requirements")]
    EmptyTask { task: LocalId },
    #[error("allocated program repeats Procedure parameter `{parameter}`")]
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
    #[error("capability requirement `{requirement}` has invalid adapter profile data")]
    InvalidAdapterBinding { requirement: LocalId },
    #[error("allocated program repeats Procedure material input `{input}`")]
    DuplicateMaterialInput { input: LocalId },
    #[error("Procedure material input `{input}` has an invalid physical source")]
    InvalidMaterialBinding { input: LocalId },
    #[error("allocated program repeats capability requirement `{requirement}`")]
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
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use lab_capability::{
        AbsoluteIri, CapabilityKind, ConstraintRelation, ControlMode, ExactInteger, MethodId,
        OperationId, PropertyKind, PropertyValue, QualificationLevel, ScalarValue,
    };

    use super::*;
    use crate::allocation::{AllocatedRequirementBinding, InvocationAdapter};
    use crate::method::{IntentOperationId, ProcedureValue};
    use crate::planning::{
        PlanningMethodYield, PlanningPort, PlanningProcedureParameter, PlanningTaskInput,
        PlanningTaskOutput, SelectedCapabilityParameter,
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
                inputs: vec![crate::planning::PlanningPort {
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
    fn revalidates_registered_task_program_provenance() {
        let mut allocated = allocated_program();
        allocated.validate().unwrap();

        allocated.methods[0].tasks[0].operation =
            OperationId::new(crate::procedure::vocabulary::PLATE_DILUTED_CULTURE).unwrap();
        assert!(matches!(
            allocated.validate(),
            Err(
                AllocatedProgramValidationError::InvalidProcedureTaskProgram(
                    ProcedureTaskProgramValidationError::CannotNormalize(_)
                )
            )
        ));
    }

    #[test]
    fn task_values_are_available_only_incrementally_and_never_directly_from_other_choices() {
        let mut self_reference = allocated_program();
        let task = self_reference.methods[0].tasks[0].id.clone();
        let output = self_reference.methods[0].tasks[0].outputs[0].name.clone();
        self_reference.methods[0].tasks[0].inputs[0].source =
            PlanningValueSource::TaskOutput { task, output };
        assert!(matches!(
            self_reference.validate(),
            Err(AllocatedProgramValidationError::InvalidMethodGraph { .. })
        ));

        let mut direct_choice = allocated_program();
        direct_choice.methods[0].tasks[0].inputs[0].source = PlanningValueSource::ChoiceOutput {
            choice: id("choice"),
            output: id("output"),
        };
        assert!(matches!(
            direct_choice.validate(),
            Err(AllocatedProgramValidationError::InvalidMethodGraph { .. })
        ));
    }

    #[test]
    fn task_outputs_are_unique_and_method_yields_use_only_local_values() {
        let mut duplicate_output = allocated_program();
        let duplicate = duplicate_output.methods[0].tasks[0].outputs[0].clone();
        duplicate_output.methods[0].tasks[0].outputs.push(duplicate);
        assert!(matches!(
            duplicate_output.validate(),
            Err(AllocatedProgramValidationError::InvalidMethodGraph { .. })
        ));

        let mut direct_choice = allocated_program();
        direct_choice.methods[0].yields[0].source = PlanningValueSource::ChoiceOutput {
            choice: id("choice"),
            output: id("output"),
        };
        assert!(matches!(
            direct_choice.validate(),
            Err(AllocatedProgramValidationError::InvalidMethodGraph { .. })
        ));
    }

    #[test]
    fn selected_capability_parameters_and_control_policies_are_revalidated() {
        let mut policy = allocated_program();
        policy.methods[0].tasks[0].requirements[0].accepted_control_modes =
            BTreeSet::from([ControlMode::Unspecified]);
        assert!(matches!(
            policy.validate(),
            Err(AllocatedProgramValidationError::InvalidControlPolicy { .. })
        ));

        let mut observation = allocated_program();
        observation.methods[0].tasks[0].requirements[0].parameters = vec![selected_parameter(1)];
        assert!(matches!(
            observation.validate(),
            Err(AllocatedProgramValidationError::InvalidParameterBinding { .. })
        ));

        let mut duplicate = allocated_program();
        let parameter = selected_parameter(2);
        duplicate.methods[0].tasks[0].requirements[0].parameters =
            vec![parameter.clone(), parameter];
        assert!(matches!(
            duplicate.validate(),
            Err(AllocatedProgramValidationError::InvalidParameterBinding { .. })
        ));
    }

    #[test]
    fn procedure_parameter_ids_are_global() {
        let mut allocated = allocated_program();
        let parameter = PlanningProcedureParameter {
            id: id("choice::parameter"),
            property_kind: PropertyKind::new("https://example.org/property/count").unwrap(),
            value: ProcedureValue::Scalar {
                value: PropertyValue::unitless(ScalarValue::Integer(
                    ExactInteger::parse("1").unwrap(),
                )),
            },
        };
        allocated.methods[0].tasks[0].parameters = vec![parameter.clone(), parameter];
        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::DuplicateProcedureParameter { .. })
        ));
    }

    #[test]
    fn material_binding_alternatives_exclude_the_selected_lot() {
        let mut allocated = allocated_program();
        let selected = "https://example.org/lot/a".to_owned();
        allocated.methods[0].tasks[0]
            .materials
            .push(SelectedMaterialBinding {
                input: id("choice::material"),
                symbol: "sample".to_owned(),
                source: SelectedMaterialSource::MaterialLot {
                    component: "https://example.org/component/sample".to_owned(),
                    material_lot: selected.clone(),
                },
                interchangeable_alternatives: vec![selected],
            });

        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::InvalidMaterialBinding { .. })
        ));
    }

    #[test]
    fn material_values_are_affine_in_the_semantic_graph() {
        let mut allocated = allocated_program();
        let material = PortType::Material {
            state: AbsoluteIri::new("https://example.org/material/sample").unwrap(),
        };
        allocated.methods[0].inputs[0].port_type = material.clone();
        allocated.methods[0].tasks[0].inputs[0].port_type = material;
        let duplicate = allocated.methods[0].tasks[0].inputs[0].clone();
        allocated.methods[0].tasks[0].inputs.push(duplicate);
        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::InvalidMethodGraph { .. })
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
            allocated.validate(),
            Err(AllocatedProgramValidationError::MaterialLinearity { uses: 2, .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn adapter_profile_paths_must_be_relative_utf8() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let mut allocated = allocated_program();
        allocated.methods[0].tasks[0].requirements[0]
            .adapter
            .as_mut()
            .unwrap()
            .profile_path = PathBuf::from(OsString::from_vec(b"profiles/\xff.toml".to_vec()));
        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::InvalidAdapterBinding { .. })
        ));

        allocated.methods[0].tasks[0].requirements[0]
            .adapter
            .as_mut()
            .unwrap()
            .profile_path = PathBuf::from("../profiles/example.toml");
        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::InvalidAdapterBinding { .. })
        ));
    }

    #[test]
    fn adapter_bindings_validate_the_complete_profile_shape() {
        let mut allocated = allocated_program();
        allocated.methods[0].tasks[0].requirements[0]
            .adapter
            .as_mut()
            .unwrap()
            .driver
            .clear();
        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::InvalidAdapterBinding { .. })
        ));

        let mut allocated = allocated_program();
        allocated.methods[0].tasks[0].requirements[0]
            .adapter
            .as_mut()
            .unwrap()
            .profile_sha256 = "not-a-digest".to_owned();
        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::InvalidAdapterBinding { .. })
        ));

        let mut allocated = allocated_program();
        allocated.methods[0].tasks[0].requirements[0]
            .adapter
            .as_mut()
            .unwrap()
            .accepted_run_formats
            .insert(String::new());
        assert!(matches!(
            allocated.validate(),
            Err(AllocatedProgramValidationError::InvalidAdapterBinding { .. })
        ));
    }
}

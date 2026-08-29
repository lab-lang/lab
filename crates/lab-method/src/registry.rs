use std::collections::{BTreeMap, BTreeSet};

use lab_capability::{ConstraintRelation, ControlMode, MethodId};
use thiserror::Error;

use crate::{
    ConstraintValue, IntentOperationId, LocalId, MethodDefinition, MethodSignature, ScalarType,
    TaskOutput, ValueReference,
};

/// A malformed portable method definition.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum MethodDefinitionError {
    #[error("method input `{id}` occurs more than once")]
    DuplicateInput { id: LocalId },
    #[error("method parameter `{id}` occurs more than once")]
    DuplicateParameter { id: LocalId },
    #[error("Procedure task `{id}` occurs more than once")]
    DuplicateTask { id: LocalId },
    #[error("Procedure task `{task}` output `{output}` occurs more than once")]
    DuplicateTaskOutput { task: LocalId, output: LocalId },
    #[error("Capability requirement `{id}` occurs more than once")]
    DuplicateRequirement { id: LocalId },
    #[error("Procedure task `{task}` has no Capability requirements")]
    MissingRequirement { task: LocalId },
    #[error("Capability requirement `{requirement}` has no concrete accepted control mode")]
    MissingControlMode { requirement: LocalId },
    #[error("Capability requirement `{requirement}` accepts descriptive UnspecifiedControl")]
    UnspecifiedControlMode { requirement: LocalId },
    #[error(
        "Capability requirement `{requirement}` references unavailable Intent parameter `{parameter}`"
    )]
    UnavailableConstraintParameter {
        requirement: LocalId,
        parameter: LocalId,
    },
    #[error(
        "Capability requirement `{requirement}` applies a unit to non-numeric Intent parameter `{parameter}`"
    )]
    UnitOnNonNumericParameter {
        requirement: LocalId,
        parameter: LocalId,
    },
    #[error(
        "Capability requirement `{requirement}` uses an ordered relation with non-numeric scalar type `{scalar_type:?}`"
    )]
    NonNumericOrderedConstraint {
        requirement: LocalId,
        scalar_type: ScalarType,
    },
    #[error("Procedure task `{task}` references unavailable value `{reference:?}`")]
    UnavailableTaskInput {
        task: LocalId,
        reference: ValueReference,
    },
    #[error("method output `{id}` occurs more than once")]
    DuplicateMethodOutput { id: LocalId },
    #[error("method output `{output}` references unavailable value `{reference:?}`")]
    UnavailableMethodOutput {
        output: LocalId,
        reference: ValueReference,
    },
    #[error("method contains no Procedure tasks")]
    EmptyProcedure,
}

/// A conflict while building a deterministic method registry.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum MethodRegistryError {
    #[error("method `{method}` is invalid: {source}")]
    InvalidDefinition {
        method: MethodId,
        source: MethodDefinitionError,
    },
    #[error("method identity `{id}` occurs more than once")]
    DuplicateMethod { id: MethodId },
    #[error("method `{method}` does not implement the common signature for `{operation}`")]
    IncompatibleSignature {
        operation: IntentOperationId,
        method: MethodId,
    },
}

/// A validated, deterministically ordered set of portable method definitions.
#[derive(Clone, Debug, Default)]
pub struct MethodRegistry {
    by_operation: BTreeMap<IntentOperationId, Vec<MethodDefinition>>,
}

impl MethodDefinition {
    /// Validate graph identities, topological references, requirements, and the yielded signature.
    pub fn validate(&self) -> Result<MethodSignature, MethodDefinitionError> {
        if self.tasks.is_empty() {
            return Err(MethodDefinitionError::EmptyProcedure);
        }
        let mut available = BTreeMap::new();
        let mut input_ids = BTreeSet::new();
        for input in &self.inputs {
            if !input_ids.insert(input.name.clone()) {
                return Err(MethodDefinitionError::DuplicateInput {
                    id: input.name.clone(),
                });
            }
            available.insert(
                ValueReference::Input {
                    input: input.name.clone(),
                },
                input.port_type.clone(),
            );
        }

        let mut parameter_types = BTreeMap::new();
        for parameter in &self.parameters {
            if parameter_types
                .insert(parameter.name.clone(), parameter.scalar_type)
                .is_some()
            {
                return Err(MethodDefinitionError::DuplicateParameter {
                    id: parameter.name.clone(),
                });
            }
        }

        let mut task_ids = BTreeSet::new();
        let mut requirement_ids = BTreeSet::new();
        for task in &self.tasks {
            if !task_ids.insert(task.id.clone()) {
                return Err(MethodDefinitionError::DuplicateTask {
                    id: task.id.clone(),
                });
            }
            for reference in &task.inputs {
                if !available.contains_key(reference) {
                    return Err(MethodDefinitionError::UnavailableTaskInput {
                        task: task.id.clone(),
                        reference: reference.clone(),
                    });
                }
            }
            if task.requirements.is_empty() {
                return Err(MethodDefinitionError::MissingRequirement {
                    task: task.id.clone(),
                });
            }
            for requirement in &task.requirements {
                if !requirement_ids.insert(requirement.id.clone()) {
                    return Err(MethodDefinitionError::DuplicateRequirement {
                        id: requirement.id.clone(),
                    });
                }
                if requirement.accepted_control_modes.is_empty() {
                    return Err(MethodDefinitionError::MissingControlMode {
                        requirement: requirement.id.clone(),
                    });
                }
                if requirement
                    .accepted_control_modes
                    .contains(&ControlMode::Unspecified)
                {
                    return Err(MethodDefinitionError::UnspecifiedControlMode {
                        requirement: requirement.id.clone(),
                    });
                }
                for constraint in &requirement.constraints {
                    let scalar_type = match &constraint.required {
                        ConstraintValue::Literal { value } => ScalarType::of(&value.value),
                        ConstraintValue::IntentParameter { parameter, unit } => {
                            let Some(scalar_type) = parameter_types.get(parameter).copied() else {
                                return Err(
                                    MethodDefinitionError::UnavailableConstraintParameter {
                                        requirement: requirement.id.clone(),
                                        parameter: parameter.clone(),
                                    },
                                );
                            };
                            if unit.is_some() && !scalar_type.is_numeric() {
                                return Err(MethodDefinitionError::UnitOnNonNumericParameter {
                                    requirement: requirement.id.clone(),
                                    parameter: parameter.clone(),
                                });
                            }
                            scalar_type
                        }
                    };
                    if !matches!(constraint.relation, ConstraintRelation::Exact)
                        && !scalar_type.is_numeric()
                    {
                        return Err(MethodDefinitionError::NonNumericOrderedConstraint {
                            requirement: requirement.id.clone(),
                            scalar_type,
                        });
                    }
                }
            }
            let mut outputs = BTreeSet::new();
            for output in &task.outputs {
                if !outputs.insert(output.name.clone()) {
                    return Err(MethodDefinitionError::DuplicateTaskOutput {
                        task: task.id.clone(),
                        output: output.name.clone(),
                    });
                }
                available.insert(
                    ValueReference::TaskOutput {
                        task: task.id.clone(),
                        output: output.name.clone(),
                    },
                    output.port_type.clone(),
                );
            }
        }

        let mut output_ids = BTreeSet::new();
        let mut outputs = Vec::new();
        for output in &self.outputs {
            if !output_ids.insert(output.name.clone()) {
                return Err(MethodDefinitionError::DuplicateMethodOutput {
                    id: output.name.clone(),
                });
            }
            let Some(port_type) = available.get(&output.source) else {
                return Err(MethodDefinitionError::UnavailableMethodOutput {
                    output: output.name.clone(),
                    reference: output.source.clone(),
                });
            };
            outputs.push(TaskOutput {
                name: output.name.clone(),
                port_type: port_type.clone(),
            });
        }
        Ok(MethodSignature {
            inputs: self.inputs.clone(),
            parameters: self.parameters.clone(),
            outputs,
        })
    }
}

impl MethodRegistry {
    /// Validate definitions, reject conflicts, and index candidates by exact Intent operation.
    pub fn new(
        definitions: impl IntoIterator<Item = MethodDefinition>,
    ) -> Result<Self, MethodRegistryError> {
        let mut definitions = definitions.into_iter().collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.id.cmp(&right.id));
        let mut method_ids = BTreeSet::new();
        let mut signatures = BTreeMap::<IntentOperationId, MethodSignature>::new();
        let mut by_operation = BTreeMap::<IntentOperationId, Vec<MethodDefinition>>::new();
        for definition in definitions {
            if !method_ids.insert(definition.id.clone()) {
                return Err(MethodRegistryError::DuplicateMethod { id: definition.id });
            }
            let signature =
                definition
                    .validate()
                    .map_err(|source| MethodRegistryError::InvalidDefinition {
                        method: definition.id.clone(),
                        source,
                    })?;
            if signatures
                .get(&definition.refines)
                .is_some_and(|expected| expected != &signature)
            {
                return Err(MethodRegistryError::IncompatibleSignature {
                    operation: definition.refines,
                    method: definition.id,
                });
            }
            signatures
                .entry(definition.refines.clone())
                .or_insert(signature);
            by_operation
                .entry(definition.refines.clone())
                .or_default()
                .push(definition);
        }
        Ok(Self { by_operation })
    }

    /// Return candidates in stable Method-IRI order for one exact Intent operation.
    pub fn methods_for(&self, operation: &IntentOperationId) -> &[MethodDefinition] {
        self.by_operation
            .get(operation)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Iterate exact Intent operations in stable lexical order.
    pub fn operations(&self) -> impl Iterator<Item = &IntentOperationId> {
        self.by_operation.keys()
    }

    pub fn is_empty(&self) -> bool {
        self.by_operation.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lab_capability::{
        AbsoluteIri, CapabilityKind, ControlMode, MethodId, OperationId, QualificationLevel,
    };

    use crate::{
        CapabilityConstraintDefinition, CapabilityRequirementDefinition, ConstraintValue,
        MethodDefinition, MethodInput, MethodOutput, MethodParameter, PortType,
        ProcedureTaskDefinition, ScalarType, TaskOutput, ValueReference,
    };

    use super::*;

    fn id(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn material(value: &str) -> PortType {
        PortType::Material {
            state: AbsoluteIri::new(value).unwrap(),
        }
    }

    fn definition(method: &str, output_state: &str) -> MethodDefinition {
        MethodDefinition {
            id: MethodId::new(method).unwrap(),
            refines: IntentOperationId::new("std.lab.incubate").unwrap(),
            inputs: vec![MethodInput {
                name: id("culture"),
                port_type: material("https://example.org/state/unincubated"),
            }],
            parameters: vec![MethodParameter {
                name: id("duration"),
                scalar_type: ScalarType::Real,
            }],
            tasks: vec![ProcedureTaskDefinition {
                id: id("incubate"),
                operation: OperationId::new("https://example.org/operation/incubate").unwrap(),
                inputs: vec![ValueReference::Input {
                    input: id("culture"),
                }],
                outputs: vec![TaskOutput {
                    name: id("product"),
                    port_type: material(output_state),
                }],
                requirements: vec![CapabilityRequirementDefinition {
                    id: id("environment"),
                    capability_kind: CapabilityKind::new(
                        "https://sbol.io/ns/capability#Incubation",
                    )
                    .unwrap(),
                    minimum_qualification: QualificationLevel::Plannable,
                    accepted_control_modes: BTreeSet::from([ControlMode::Manual, ControlMode::Api]),
                    constraints: vec![CapabilityConstraintDefinition {
                        property_kind: lab_capability::PropertyKind::new(
                            "https://sbol.io/ns/capability#Duration",
                        )
                        .unwrap(),
                        relation: ConstraintRelation::Exact,
                        required: ConstraintValue::IntentParameter {
                            parameter: id("duration"),
                            unit: Some(
                                lab_capability::UnitIri::new("http://qudt.org/vocab/unit/HR")
                                    .unwrap(),
                            ),
                        },
                    }],
                }],
            }],
            outputs: vec![MethodOutput {
                name: id("product"),
                source: ValueReference::TaskOutput {
                    task: id("incubate"),
                    output: id("product"),
                },
            }],
        }
    }

    #[test]
    fn a_valid_definition_round_trips_and_indexes_by_exact_operation() {
        let definition = definition(
            "https://example.org/method/static-incubation",
            "https://example.org/state/incubated",
        );
        let json = serde_json::to_string_pretty(&definition).unwrap();
        let reparsed: MethodDefinition = serde_json::from_str(&json).unwrap();
        let registry = MethodRegistry::new([reparsed.clone()]).unwrap();

        assert_eq!(
            registry.methods_for(&IntentOperationId::new("std.lab.incubate").unwrap()),
            &[reparsed]
        );
    }

    #[test]
    fn task_order_is_topological_and_forward_references_fail_closed() {
        let mut definition = definition(
            "https://example.org/method/static-incubation",
            "https://example.org/state/incubated",
        );
        definition.tasks[0].inputs = vec![ValueReference::TaskOutput {
            task: id("later"),
            output: id("product"),
        }];

        assert!(matches!(
            definition.validate(),
            Err(MethodDefinitionError::UnavailableTaskInput { .. })
        ));
    }

    #[test]
    fn constraint_parameters_are_declared_and_type_checked() {
        let mut definition = definition(
            "https://example.org/method/static-incubation",
            "https://example.org/state/incubated",
        );
        definition.tasks[0].requirements[0].constraints[0].required =
            ConstraintValue::IntentParameter {
                parameter: id("missing"),
                unit: None,
            };
        assert!(matches!(
            definition.validate(),
            Err(MethodDefinitionError::UnavailableConstraintParameter { .. })
        ));

        definition.tasks[0].requirements[0].constraints[0].required =
            ConstraintValue::IntentParameter {
                parameter: id("duration"),
                unit: None,
            };
        definition.parameters[0].scalar_type = ScalarType::Text;
        definition.tasks[0].requirements[0].constraints[0].relation = ConstraintRelation::AtLeast;
        assert!(matches!(
            definition.validate(),
            Err(MethodDefinitionError::NonNumericOrderedConstraint { .. })
        ));
    }

    #[test]
    fn descriptive_control_cannot_become_an_operational_requirement() {
        let mut definition = definition(
            "https://example.org/method/static-incubation",
            "https://example.org/state/incubated",
        );
        definition.tasks[0].requirements[0]
            .accepted_control_modes
            .insert(ControlMode::Unspecified);

        assert!(matches!(
            definition.validate(),
            Err(MethodDefinitionError::UnspecifiedControlMode { .. })
        ));
    }

    #[test]
    fn all_candidates_for_one_intent_must_implement_one_signature() {
        let first = definition(
            "https://example.org/method/ambient-incubation",
            "https://example.org/state/incubated",
        );
        let second = definition(
            "https://example.org/method/instrument-incubation",
            "https://example.org/state/different",
        );

        assert!(matches!(
            MethodRegistry::new([first, second]),
            Err(MethodRegistryError::IncompatibleSignature { .. })
        ));
    }

    #[test]
    fn method_identities_are_globally_unique() {
        let definition = definition(
            "https://example.org/method/static-incubation",
            "https://example.org/state/incubated",
        );

        assert!(matches!(
            MethodRegistry::new([definition.clone(), definition]),
            Err(MethodRegistryError::DuplicateMethod { .. })
        ));
    }
}

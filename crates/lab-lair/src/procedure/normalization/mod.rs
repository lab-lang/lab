//! Compiler-owned normalization from open Method operations into canonical Procedure contracts.

mod chemical_transformation;
mod golden_gate;
mod plating;
mod recovery;
mod serial_dilution;
mod thermal_cycle;
mod view;

use crate::method::{LocalId, ProcedureValue};
use crate::procedure::vocabulary::{
    ADD_RECOVERY_MEDIUM, CYCLE_GOLDEN_GATE, HEAT_SHOCK_TRANSFORMATION, INCUBATE_RECOVERY_CULTURE,
    PLATE_DILUTED_CULTURE, PREPARE_CHEMICAL_TRANSFORMATION, SERIAL_DILUTION, SETUP_GOLDEN_GATE,
};
use crate::procedure::{
    ProcedureProgram, ProcedureProgramValidationError, ValidatedProcedureProgram,
};
use lab_capability::OperationId;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedProcedureParameter {
    pub(crate) id: LocalId,
    pub(crate) value: ProcedureValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedProcedureMaterial {
    pub(crate) id: LocalId,
    pub(crate) symbol: String,
}

pub(crate) struct ProcedureTaskInstance<'a> {
    pub(crate) id: &'a LocalId,
    pub(crate) operation: &'a OperationId,
    pub(crate) input_count: usize,
    pub(crate) outputs: &'a [LocalId],
    pub(crate) parameters: &'a [ResolvedProcedureParameter],
    pub(crate) materials: &'a [ResolvedProcedureMaterial],
}

pub(crate) fn normalize_task(
    task: &ProcedureTaskInstance<'_>,
) -> Result<Option<ProcedureProgram>, ProcedureNormalizationError> {
    let result = match task.operation.as_str() {
        SETUP_GOLDEN_GATE => Some(golden_gate::normalize(task)),
        SERIAL_DILUTION => Some(serial_dilution::normalize(task)),
        CYCLE_GOLDEN_GATE => Some(thermal_cycle::normalize(task)),
        PREPARE_CHEMICAL_TRANSFORMATION => Some(chemical_transformation::normalize_prepare(task)),
        HEAT_SHOCK_TRANSFORMATION => Some(chemical_transformation::normalize_heat_shock(task)),
        ADD_RECOVERY_MEDIUM => Some(recovery::normalize_add_medium(task)),
        INCUBATE_RECOVERY_CULTURE => Some(recovery::normalize_incubation(task)),
        PLATE_DILUTED_CULTURE => Some(plating::normalize(task)),
        _ => None,
    };
    result
        .transpose()
        .map_err(|message| ProcedureNormalizationError {
            task: task.id.clone(),
            operation: task.operation.clone(),
            message,
        })
}

/// Validate the operational program carried by one resolved Procedure task.
///
/// Operations registered by this compiler must carry the exact canonical program produced by
/// their normalizer from the resolved ports, parameters, and material symbols that operation
/// consumes. Descriptive task facts that a normalizer does not consume are intentionally outside
/// this operational provenance check. Unregistered operations remain open: they may omit a program
/// or supply any program understood by the Procedure contract registry.
pub fn validate_task_program<'parameter, 'material>(
    task: &LocalId,
    operation: &OperationId,
    input_count: usize,
    outputs: &[LocalId],
    parameters: impl IntoIterator<Item = (&'parameter LocalId, &'parameter ProcedureValue)>,
    materials: impl IntoIterator<Item = (&'material LocalId, &'material str)>,
    program: Option<&ProcedureProgram>,
) -> Result<Option<ValidatedProcedureProgram>, ProcedureTaskProgramValidationError> {
    let validated = program
        .map(ProcedureProgram::validate)
        .transpose()
        .map_err(
            |source| ProcedureTaskProgramValidationError::InvalidProgram {
                task: task.clone(),
                source: Box::new(source),
            },
        )?;
    let parameters = parameters
        .into_iter()
        .map(|(id, value)| ResolvedProcedureParameter {
            id: id.clone(),
            value: value.clone(),
        })
        .collect::<Vec<_>>();
    let materials = materials
        .into_iter()
        .map(|(id, symbol)| ResolvedProcedureMaterial {
            id: id.clone(),
            symbol: symbol.to_owned(),
        })
        .collect::<Vec<_>>();
    let resolved = ProcedureTaskInstance {
        id: task,
        operation,
        input_count,
        outputs,
        parameters: &parameters,
        materials: &materials,
    };
    let Some(expected) = normalize_task(&resolved)? else {
        return Ok(validated);
    };
    let Some(actual) = program else {
        return Err(
            ProcedureTaskProgramValidationError::MissingNormalizedProgram {
                task: task.clone(),
                operation: operation.clone(),
            },
        );
    };
    if actual != &expected {
        return Err(
            ProcedureTaskProgramValidationError::NonCanonicalNormalizedProgram {
                task: task.clone(),
                operation: operation.clone(),
            },
        );
    }
    Ok(validated)
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("Procedure task `{task}` operation `{operation}` cannot be normalized: {message}")]
pub struct ProcedureNormalizationError {
    task: LocalId,
    operation: OperationId,
    message: String,
}

/// A failure to validate the program provenance of one resolved Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureTaskProgramValidationError {
    #[error("Procedure task `{task}` has an invalid program: {source}")]
    InvalidProgram {
        task: LocalId,
        #[source]
        source: Box<ProcedureProgramValidationError>,
    },
    #[error(transparent)]
    CannotNormalize(#[from] ProcedureNormalizationError),
    #[error(
        "Procedure task `{task}` operation `{operation}` is registered but has no normalized program"
    )]
    MissingNormalizedProgram {
        task: LocalId,
        operation: OperationId,
    },
    #[error(
        "Procedure task `{task}` operation `{operation}` does not carry its canonical normalized program"
    )]
    NonCanonicalNormalizedProgram {
        task: LocalId,
        operation: OperationId,
    },
}

#[cfg(test)]
mod tests {
    use lab_capability::{ExactInteger, OperationId, PropertyValue, ScalarValue, UnitIri};

    use super::*;
    use crate::procedure::vocabulary::{MICROLITRE, PLATE_DILUTED_CULTURE};

    struct Fixture {
        task: LocalId,
        operation: OperationId,
        input_count: usize,
        outputs: Vec<LocalId>,
        parameters: Vec<ResolvedProcedureParameter>,
        materials: Vec<ResolvedProcedureMaterial>,
        program: ProcedureProgram,
    }

    impl Fixture {
        fn validate(
            &self,
            operation: &OperationId,
            program: Option<&ProcedureProgram>,
        ) -> Result<Option<ValidatedProcedureProgram>, ProcedureTaskProgramValidationError>
        {
            validate_task_program(
                &self.task,
                operation,
                self.input_count,
                &self.outputs,
                self.parameters
                    .iter()
                    .map(|parameter| (&parameter.id, &parameter.value)),
                self.materials
                    .iter()
                    .map(|material| (&material.id, material.symbol.as_str())),
                program,
            )
        }
    }

    fn integer_parameter(task: &LocalId, name: &str, value: u32) -> ResolvedProcedureParameter {
        let value = PropertyValue::new(
            ScalarValue::Integer(ExactInteger::parse(value.to_string()).unwrap()),
            (!matches!(
                name,
                "replicates" | "culture_replicates" | "serial_dilutions"
            ))
            .then(|| UnitIri::new(MICROLITRE).unwrap()),
        )
        .unwrap();
        ResolvedProcedureParameter {
            id: LocalId::new(format!("{}::parameter::{name}", task.as_str())).unwrap(),
            value: ProcedureValue::Scalar { value },
        }
    }

    fn text_parameter(task: &LocalId, name: &str, value: &str) -> ResolvedProcedureParameter {
        ResolvedProcedureParameter {
            id: LocalId::new(format!("{}::parameter::{name}", task.as_str())).unwrap(),
            value: ProcedureValue::Scalar {
                value: PropertyValue::unitless(ScalarValue::Text(value.to_owned())),
            },
        }
    }

    fn plating_fixture() -> Fixture {
        let task = LocalId::new("selection::plate").unwrap();
        let operation = OperationId::new(PLATE_DILUTED_CULTURE).unwrap();
        let outputs = vec![LocalId::new("plate").unwrap()];
        let parameters = vec![
            text_parameter(&task, "selection", "ampicillin"),
            integer_parameter(&task, "replicates", 1),
            integer_parameter(&task, "culture_replicates", 1),
            integer_parameter(&task, "serial_dilutions", 1),
            integer_parameter(&task, "medium_volume_ul", 10),
            integer_parameter(&task, "culture_volume_ul", 1),
            integer_parameter(&task, "colony_volume_ul", 1),
        ];
        let materials = vec![ResolvedProcedureMaterial {
            id: LocalId::new(format!("{}::material::selection", task.as_str())).unwrap(),
            symbol: "ampicillin".to_owned(),
        }];
        let program = normalize_task(&ProcedureTaskInstance {
            id: &task,
            operation: &operation,
            input_count: 1,
            outputs: &outputs,
            parameters: &parameters,
            materials: &materials,
        })
        .unwrap()
        .unwrap();
        Fixture {
            task,
            operation,
            input_count: 1,
            outputs,
            parameters,
            materials,
            program,
        }
    }

    #[test]
    fn registered_programs_are_exact_while_external_operations_remain_open() {
        let fixture = plating_fixture();
        assert!(
            fixture
                .validate(&fixture.operation, Some(&fixture.program))
                .unwrap()
                .is_some()
        );
        assert!(matches!(
            fixture.validate(&fixture.operation, None),
            Err(ProcedureTaskProgramValidationError::MissingNormalizedProgram { .. })
        ));

        let mut corrupted = fixture.program.clone();
        corrupted.body["steps"][0]["id"] = serde_json::json!("renamed-step");
        corrupted.validate().unwrap();
        assert!(matches!(
            fixture.validate(&fixture.operation, Some(&corrupted)),
            Err(ProcedureTaskProgramValidationError::NonCanonicalNormalizedProgram { .. })
        ));

        let external = OperationId::new("https://example.org/procedure/external").unwrap();
        assert!(
            fixture
                .validate(&external, Some(&corrupted))
                .unwrap()
                .is_some()
        );
        assert!(fixture.validate(&external, None).unwrap().is_none());
    }

    #[test]
    fn registered_programs_are_recomputed_from_parameters_and_material_symbols() {
        let mut parameters = plating_fixture();
        parameters.parameters[1] = integer_parameter(&parameters.task, "replicates", 2);
        assert!(matches!(
            parameters.validate(&parameters.operation, Some(&parameters.program)),
            Err(ProcedureTaskProgramValidationError::NonCanonicalNormalizedProgram { .. })
        ));

        let mut materials = plating_fixture();
        materials.materials[0].symbol = "kanamycin".to_owned();
        assert!(matches!(
            materials.validate(&materials.operation, Some(&materials.program)),
            Err(ProcedureTaskProgramValidationError::CannotNormalize(_))
        ));
    }
}

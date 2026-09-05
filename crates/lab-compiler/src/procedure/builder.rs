//! Explicit registrations that build complete, device-neutral Procedure programs.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::sync::OnceLock;

use lab_capability::{AbsoluteIri, IriError, MethodId, ProcedureContractId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::method::{LocalId, MethodRegistry, ProcedureTaskExecutionDefinition, ProcedureValue};
use crate::procedure::normalization;
use crate::procedure::vocabulary::{
    ADD_RECOVERY_MEDIUM_BUILDER_V1, CYCLE_GOLDEN_GATE_BUILDER_V1,
    HEAT_SHOCK_TRANSFORMATION_BUILDER_V1, INCUBATE_RECOVERY_CULTURE_BUILDER_V1,
    PIPETTING_PROGRAM_V1, PLATE_DILUTED_CULTURE_BUILDER_V1,
    PREPARE_CHEMICAL_TRANSFORMATION_BUILDER_V1, SERIAL_DILUTION_BUILDER_V1,
    SETUP_GOLDEN_GATE_BUILDER_V1, THERMAL_PROGRAM_V1,
};
use crate::procedure::{
    ProcedureContractRegistry, ProcedureProgram, ProcedureProgramTemplateError,
    ProcedureProgramValidationError, ValidatedProcedureProgram, evaluate_procedure_template,
};

/// The stable absolute IRI naming one Procedure-program construction algorithm.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct ProcedureProgramBuilderId(AbsoluteIri);

impl ProcedureProgramBuilderId {
    pub fn new(value: impl Into<String>) -> Result<Self, IriError> {
        AbsoluteIri::new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for ProcedureProgramBuilderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl AsRef<str> for ProcedureProgramBuilderId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for ProcedureProgramBuilderId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for ProcedureProgramBuilderId {
    type Error = IriError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for ProcedureProgramBuilderId {
    type Error = IriError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// One resolved scalar or list value available to a Procedure-program builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedProcedureParameter {
    pub id: LocalId,
    pub value: ProcedureValue,
}

/// One resolved external material available to a Procedure-program builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedProcedureMaterial {
    pub id: LocalId,
    pub symbol: String,
}

/// The complete facility-independent input to one Procedure-program builder.
///
/// The Method task's descriptive operation IRI is deliberately absent. Selecting a builder is an
/// explicit Method decision, never a convention inferred from an operation name.
pub struct ProcedureProgramBuildContext<'a> {
    /// Complete checked source invocation, including exact declaration identity, typed values,
    /// ownership, lineage, artifact design, and provenance.
    pub intent: &'a crate::workflow::IntentAction,
    pub input_count: usize,
    pub outputs: &'a [LocalId],
    pub parameters: &'a [ResolvedProcedureParameter],
    pub materials: &'a [ResolvedProcedureMaterial],
}

pub type ProcedureProgramBuilder =
    for<'a> fn(&ProcedureProgramBuildContext<'a>) -> Result<ProcedureProgram, String>;

/// One statically linked Procedure-program builder and the contract it promises to produce.
#[derive(Clone)]
pub struct ProcedureProgramBuilderRegistration {
    pub id: ProcedureProgramBuilderId,
    pub contract: ProcedureContractId,
    build: ProcedureProgramBuilder,
}

impl fmt::Debug for ProcedureProgramBuilderRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcedureProgramBuilderRegistration")
            .field("id", &self.id)
            .field("contract", &self.contract)
            .finish_non_exhaustive()
    }
}

impl ProcedureProgramBuilderRegistration {
    pub fn new(
        id: ProcedureProgramBuilderId,
        contract: ProcedureContractId,
        build: ProcedureProgramBuilder,
    ) -> Self {
        Self {
            id,
            contract,
            build,
        }
    }

    fn build(
        &self,
        context: &ProcedureProgramBuildContext<'_>,
    ) -> Result<ProcedureProgram, String> {
        (self.build)(context)
    }
}

/// A deterministic set of Procedure-program builders available to one compiler composition.
#[derive(Clone, Debug, Default)]
pub struct ProcedureProgramBuilderRegistry {
    registrations: BTreeMap<ProcedureProgramBuilderId, ProcedureProgramBuilderRegistration>,
}

impl ProcedureProgramBuilderRegistry {
    pub fn new(
        registrations: impl IntoIterator<Item = ProcedureProgramBuilderRegistration>,
    ) -> Result<Self, ProcedureProgramBuilderRegistryError> {
        let mut by_id = BTreeMap::new();
        for registration in registrations {
            let id = registration.id.clone();
            if by_id.insert(id.clone(), registration).is_some() {
                return Err(ProcedureProgramBuilderRegistryError::Duplicate { builder: id });
            }
        }
        Ok(Self {
            registrations: by_id,
        })
    }

    pub fn registration(
        &self,
        id: &ProcedureProgramBuilderId,
    ) -> Option<&ProcedureProgramBuilderRegistration> {
        self.registrations.get(id)
    }

    /// Return a new composition containing this registry plus one builder.
    pub fn with_registration(
        &self,
        registration: ProcedureProgramBuilderRegistration,
    ) -> Result<Self, ProcedureProgramBuilderRegistryError> {
        Self::new(
            self.registrations
                .values()
                .cloned()
                .chain(std::iter::once(registration)),
        )
    }

    pub fn builders(&self) -> impl Iterator<Item = &ProcedureProgramBuilderId> {
        self.registrations.keys()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureProgramBuilderRegistryError {
    #[error("Procedure-program builder `{builder}` is registered more than once")]
    Duplicate { builder: ProcedureProgramBuilderId },
}

/// The Procedure contracts and builders linked into one compiler composition.
#[derive(Clone, Debug)]
pub struct ProcedureCompiler {
    contracts: ProcedureContractRegistry,
    builders: ProcedureProgramBuilderRegistry,
}

impl ProcedureCompiler {
    pub fn new(
        contracts: ProcedureContractRegistry,
        builders: ProcedureProgramBuilderRegistry,
    ) -> Result<Self, ProcedureCompilerError> {
        for registration in builders.registrations.values() {
            if contracts.registration(&registration.contract).is_none() {
                return Err(ProcedureCompilerError::UnknownBuilderContract {
                    builder: registration.id.clone(),
                    contract: registration.contract.clone(),
                });
            }
        }
        Ok(Self {
            contracts,
            builders,
        })
    }

    pub fn contracts(&self) -> &ProcedureContractRegistry {
        &self.contracts
    }

    pub fn builders(&self) -> &ProcedureProgramBuilderRegistry {
        &self.builders
    }

    /// Validate every Method execution reference against this exact compiler composition.
    ///
    /// Method graph validation deliberately knows nothing about which Procedure extensions an
    /// application links. The application calls this once after composing both registries, so an
    /// unknown contract or builder fails during `lab check` rather than only when a workflow
    /// happens to reach that Method.
    pub fn validate_methods(
        &self,
        methods: &MethodRegistry,
    ) -> Result<(), ProcedureMethodRegistryError> {
        for method in methods.definitions() {
            for task in &method.tasks {
                match &task.execution {
                    ProcedureTaskExecutionDefinition::Template { contract, .. } => {
                        if self.contracts.registration(contract).is_none() {
                            return Err(ProcedureMethodRegistryError::UnknownContract {
                                method: method.id.clone(),
                                task: task.id.clone(),
                                contract: contract.clone(),
                            });
                        }
                    }
                    ProcedureTaskExecutionDefinition::Builder {
                        builder, contract, ..
                    } => {
                        let registration =
                            self.builders.registration(builder).ok_or_else(|| {
                                ProcedureMethodRegistryError::UnknownBuilder {
                                    method: method.id.clone(),
                                    task: task.id.clone(),
                                    builder: builder.clone(),
                                }
                            })?;
                        if &registration.contract != contract {
                            return Err(ProcedureMethodRegistryError::BuilderContractMismatch {
                                method: method.id.clone(),
                                task: task.id.clone(),
                                builder: builder.clone(),
                                registered: registration.contract.clone(),
                                declared: contract.clone(),
                            });
                        }
                    }
                    ProcedureTaskExecutionDefinition::Primitive { .. } => {}
                }
            }
        }
        Ok(())
    }

    /// Return a new compiler composition containing one additional Procedure contract.
    ///
    /// This is the complete extension step for Methods that use declarative templates and do not
    /// need a Rust builder.
    pub fn with_contract(
        &self,
        contract: crate::procedure::ProcedureContractRegistration,
    ) -> Result<Self, ProcedureCompilerError> {
        Self::new(
            self.contracts
                .with_registration(contract)
                .map_err(ProcedureCompilerError::Contracts)?,
            self.builders.clone(),
        )
    }

    /// Return a new compiler composition containing one additional builder for a registered
    /// Procedure contract.
    pub fn with_builder(
        &self,
        builder: ProcedureProgramBuilderRegistration,
    ) -> Result<Self, ProcedureCompilerError> {
        Self::new(
            self.contracts.clone(),
            self.builders
                .with_registration(builder)
                .map_err(ProcedureCompilerError::Builders)?,
        )
    }

    /// Return a new compiler composition containing one contract and its first builder.
    ///
    /// Additional builders for the same contract can be assembled through
    /// [`ProcedureProgramBuilderRegistry::with_registration`] and [`Self::new`].
    pub fn with_registration(
        &self,
        contract: crate::procedure::ProcedureContractRegistration,
        builder: ProcedureProgramBuilderRegistration,
    ) -> Result<Self, ProcedureCompilerError> {
        self.with_contract(contract)?.with_builder(builder)
    }

    pub fn validate(
        &self,
        program: &ProcedureProgram,
    ) -> Result<ValidatedProcedureProgram, ProcedureProgramValidationError> {
        program.validate(&self.contracts)
    }

    pub fn build(
        &self,
        builder: &ProcedureProgramBuilderId,
        declared_contract: &ProcedureContractId,
        context: &ProcedureProgramBuildContext<'_>,
    ) -> Result<ValidatedProcedureProgram, ProcedureProgramBuildError> {
        let registration = self.builders.registration(builder).ok_or_else(|| {
            ProcedureProgramBuildError::UnknownBuilder {
                builder: builder.clone(),
            }
        })?;
        if &registration.contract != declared_contract {
            return Err(ProcedureProgramBuildError::DeclaredContractMismatch {
                builder: builder.clone(),
                registered: registration.contract.clone(),
                declared: declared_contract.clone(),
            });
        }
        let program = registration.build(context).map_err(|message| {
            ProcedureProgramBuildError::BuildFailed {
                builder: builder.clone(),
                message,
            }
        })?;
        if &program.contract != declared_contract {
            return Err(ProcedureProgramBuildError::ProducedContractMismatch {
                builder: builder.clone(),
                expected: declared_contract.clone(),
                actual: program.contract,
            });
        }
        program.validate(&self.contracts).map_err(|source| {
            ProcedureProgramBuildError::InvalidProgram {
                builder: builder.clone(),
                source: Box::new(source),
            }
        })
    }

    /// Render a declarative template against one resolved Method task, then validate its contract.
    pub fn render_template(
        &self,
        contract: &ProcedureContractId,
        body: &serde_json::Value,
        context: &ProcedureProgramBuildContext<'_>,
    ) -> Result<ValidatedProcedureProgram, ProcedureProgramTemplateError> {
        let body = evaluate_procedure_template(body, context)?;
        ProcedureProgram {
            contract: contract.clone(),
            body,
        }
        .validate(&self.contracts)
        .map_err(|source| ProcedureProgramTemplateError::InvalidProgram {
            contract: contract.clone(),
            source: Box::new(source),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureCompilerError {
    #[error(transparent)]
    Contracts(#[from] crate::procedure::ProcedureContractRegistryError),
    #[error(transparent)]
    Builders(#[from] ProcedureProgramBuilderRegistryError),
    #[error("Procedure-program builder `{builder}` produces unregistered contract `{contract}`")]
    UnknownBuilderContract {
        builder: ProcedureProgramBuilderId,
        contract: ProcedureContractId,
    },
}

/// A Method catalog references Procedure semantics absent from an application composition.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureMethodRegistryError {
    #[error(
        "Method `{method}` task `{task}` declares unregistered Procedure contract `{contract}`"
    )]
    UnknownContract {
        method: MethodId,
        task: LocalId,
        contract: ProcedureContractId,
    },
    #[error("Method `{method}` task `{task}` names unregistered Procedure builder `{builder}`")]
    UnknownBuilder {
        method: MethodId,
        task: LocalId,
        builder: ProcedureProgramBuilderId,
    },
    #[error(
        "Method `{method}` task `{task}` declares contract `{declared}` for builder `{builder}`, but the application registers that builder for `{registered}`"
    )]
    BuilderContractMismatch {
        method: MethodId,
        task: LocalId,
        builder: ProcedureProgramBuilderId,
        registered: ProcedureContractId,
        declared: ProcedureContractId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureProgramBuildError {
    #[error("Procedure-program builder `{builder}` is not registered in this compiler")]
    UnknownBuilder { builder: ProcedureProgramBuilderId },
    #[error(
        "Procedure-program builder `{builder}` is registered for `{registered}`, but the Method declares `{declared}`"
    )]
    DeclaredContractMismatch {
        builder: ProcedureProgramBuilderId,
        registered: ProcedureContractId,
        declared: ProcedureContractId,
    },
    #[error("Procedure-program builder `{builder}` failed: {message}")]
    BuildFailed {
        builder: ProcedureProgramBuilderId,
        message: String,
    },
    #[error(
        "Procedure-program builder `{builder}` produced `{actual}`, but its declared contract is `{expected}`"
    )]
    ProducedContractMismatch {
        builder: ProcedureProgramBuilderId,
        expected: ProcedureContractId,
        actual: ProcedureContractId,
    },
    #[error("Procedure-program builder `{builder}` produced an invalid program: {source}")]
    InvalidProgram {
        builder: ProcedureProgramBuilderId,
        #[source]
        source: Box<ProcedureProgramValidationError>,
    },
}

/// The complete built-in Procedure composition used by the standard Method registry.
pub fn builtin_procedure_compiler() -> &'static ProcedureCompiler {
    static COMPILER: OnceLock<ProcedureCompiler> = OnceLock::new();
    COMPILER.get_or_init(|| {
        ProcedureCompiler::new(
            crate::procedure::builtin_procedure_contracts().clone(),
            ProcedureProgramBuilderRegistry::new([
                registration(
                    SETUP_GOLDEN_GATE_BUILDER_V1,
                    PIPETTING_PROGRAM_V1,
                    normalization::build_golden_gate,
                ),
                registration(
                    SERIAL_DILUTION_BUILDER_V1,
                    PIPETTING_PROGRAM_V1,
                    normalization::build_serial_dilution,
                ),
                registration(
                    CYCLE_GOLDEN_GATE_BUILDER_V1,
                    THERMAL_PROGRAM_V1,
                    normalization::build_golden_gate_cycle,
                ),
                registration(
                    PREPARE_CHEMICAL_TRANSFORMATION_BUILDER_V1,
                    PIPETTING_PROGRAM_V1,
                    normalization::build_chemical_transformation_preparation,
                ),
                registration(
                    HEAT_SHOCK_TRANSFORMATION_BUILDER_V1,
                    THERMAL_PROGRAM_V1,
                    normalization::build_chemical_transformation_heat_shock,
                ),
                registration(
                    ADD_RECOVERY_MEDIUM_BUILDER_V1,
                    PIPETTING_PROGRAM_V1,
                    normalization::build_recovery_medium_addition,
                ),
                registration(
                    INCUBATE_RECOVERY_CULTURE_BUILDER_V1,
                    THERMAL_PROGRAM_V1,
                    normalization::build_recovery_incubation,
                ),
                registration(
                    PLATE_DILUTED_CULTURE_BUILDER_V1,
                    PIPETTING_PROGRAM_V1,
                    normalization::build_selective_plating,
                ),
            ])
            .expect("built-in Procedure-program builder identities are unique"),
        )
        .expect("every built-in Procedure-program builder names a built-in contract")
    })
}

fn registration(
    builder: &str,
    contract: &str,
    build: ProcedureProgramBuilder,
) -> ProcedureProgramBuilderRegistration {
    ProcedureProgramBuilderRegistration::new(
        ProcedureProgramBuilderId::new(builder)
            .expect("built-in Procedure-program builder identity is an absolute IRI"),
        ProcedureContractId::new(contract)
            .expect("built-in Procedure contract identity is an absolute IRI"),
        build,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::method::{MethodDefinition, ProcedureTaskExecutionDefinition};
    use crate::procedure::{
        ProcedureContractAnalysis, ProcedureContractRegistration, ProcedureContractRegistry,
    };

    fn unreachable_builder(
        _: &ProcedureProgramBuildContext<'_>,
    ) -> Result<ProcedureProgram, String> {
        Err("not called".to_owned())
    }

    fn unreachable_contract(_: &serde_json::Value) -> Result<ProcedureContractAnalysis, String> {
        Err("not called".to_owned())
    }

    fn method_with_builder() -> MethodDefinition {
        crate::method::standard_method_definitions()
            .into_iter()
            .find(|method| {
                method.tasks.iter().any(|task| {
                    matches!(
                        task.execution,
                        ProcedureTaskExecutionDefinition::Builder { .. }
                    )
                })
            })
            .expect("the standard catalog contains a builder-backed Method")
    }

    #[test]
    fn duplicate_builder_registrations_fail_deterministically() {
        let id = ProcedureProgramBuilderId::new("https://example.org/builder").unwrap();
        let contract = ProcedureContractId::new("https://example.org/contract").unwrap();
        let error = ProcedureProgramBuilderRegistry::new([
            ProcedureProgramBuilderRegistration::new(
                id.clone(),
                contract.clone(),
                unreachable_builder,
            ),
            ProcedureProgramBuilderRegistration::new(id.clone(), contract, unreachable_builder),
        ])
        .unwrap_err();
        assert_eq!(
            error,
            ProcedureProgramBuilderRegistryError::Duplicate { builder: id }
        );
    }

    #[test]
    fn a_builder_can_be_composed_onto_an_existing_registry() {
        let id = ProcedureProgramBuilderId::new("https://example.org/builder").unwrap();
        let contract = ProcedureContractId::new("https://example.org/contract").unwrap();
        let registry = ProcedureProgramBuilderRegistry::default()
            .with_registration(ProcedureProgramBuilderRegistration::new(
                id.clone(),
                contract,
                unreachable_builder,
            ))
            .unwrap();

        assert!(registry.registration(&id).is_some());
    }

    #[test]
    fn a_template_only_contract_has_a_one_step_composition_path() {
        let contract = ProcedureContractId::new("https://example.org/template-contract").unwrap();
        let compiler = ProcedureCompiler::new(
            ProcedureContractRegistry::default(),
            ProcedureProgramBuilderRegistry::default(),
        )
        .unwrap()
        .with_contract(ProcedureContractRegistration::new(
            contract.clone(),
            unreachable_contract,
        ))
        .unwrap();

        assert!(compiler.contracts().registration(&contract).is_some());
        assert_eq!(compiler.builders().builders().count(), 0);
    }

    #[test]
    fn composition_rejects_a_builder_for_an_unknown_contract() {
        let builder = ProcedureProgramBuilderId::new("https://example.org/builder").unwrap();
        let contract = ProcedureContractId::new("https://example.org/contract").unwrap();
        let builders =
            ProcedureProgramBuilderRegistry::new([ProcedureProgramBuilderRegistration::new(
                builder.clone(),
                contract.clone(),
                unreachable_builder,
            )])
            .unwrap();
        assert_eq!(
            ProcedureCompiler::new(ProcedureContractRegistry::default(), builders).unwrap_err(),
            ProcedureCompilerError::UnknownBuilderContract { builder, contract }
        );
    }

    #[test]
    fn builtin_builder_identities_are_explicit_and_stable() {
        assert_eq!(
            builtin_procedure_compiler()
                .builders()
                .builders()
                .map(ProcedureProgramBuilderId::as_str)
                .collect::<Vec<_>>(),
            [
                ADD_RECOVERY_MEDIUM_BUILDER_V1,
                CYCLE_GOLDEN_GATE_BUILDER_V1,
                HEAT_SHOCK_TRANSFORMATION_BUILDER_V1,
                INCUBATE_RECOVERY_CULTURE_BUILDER_V1,
                PLATE_DILUTED_CULTURE_BUILDER_V1,
                PREPARE_CHEMICAL_TRANSFORMATION_BUILDER_V1,
                SERIAL_DILUTION_BUILDER_V1,
                SETUP_GOLDEN_GATE_BUILDER_V1,
            ]
        );
    }

    #[test]
    fn a_method_contract_must_match_the_builder_registration() {
        let builder = ProcedureProgramBuilderId::new(SETUP_GOLDEN_GATE_BUILDER_V1).unwrap();
        let declared = ProcedureContractId::new(THERMAL_PROGRAM_V1).unwrap();
        let intent = crate::workflow::ir::synthetic_intent("https://example.org/action");
        let error = builtin_procedure_compiler()
            .build(
                &builder,
                &declared,
                &ProcedureProgramBuildContext {
                    intent: &intent,
                    input_count: 0,
                    outputs: &[],
                    parameters: &[],
                    materials: &[],
                },
            )
            .unwrap_err();
        assert_eq!(
            error,
            ProcedureProgramBuildError::DeclaredContractMismatch {
                builder,
                registered: ProcedureContractId::new(PIPETTING_PROGRAM_V1).unwrap(),
                declared,
            }
        );
    }

    #[test]
    fn method_composition_rejects_an_unknown_builder_before_refinement() {
        let mut method = method_with_builder();
        let task = method
            .tasks
            .iter_mut()
            .find(|task| {
                matches!(
                    task.execution,
                    ProcedureTaskExecutionDefinition::Builder { .. }
                )
            })
            .unwrap();
        let ProcedureTaskExecutionDefinition::Builder { builder, .. } = &mut task.execution else {
            unreachable!()
        };
        *builder = ProcedureProgramBuilderId::new("https://example.org/builder#missing").unwrap();
        let registry = MethodRegistry::new([method]).unwrap();

        assert!(matches!(
            builtin_procedure_compiler().validate_methods(&registry),
            Err(ProcedureMethodRegistryError::UnknownBuilder { .. })
        ));
    }

    #[test]
    fn method_composition_rejects_an_unknown_template_contract_before_refinement() {
        let mut method = method_with_builder();
        let task = method
            .tasks
            .iter_mut()
            .find(|task| {
                matches!(
                    task.execution,
                    ProcedureTaskExecutionDefinition::Builder { .. }
                )
            })
            .unwrap();
        let policy = match &task.execution {
            ProcedureTaskExecutionDefinition::Builder { policy, .. } => policy.clone(),
            _ => unreachable!(),
        };
        task.execution = ProcedureTaskExecutionDefinition::Template {
            contract: ProcedureContractId::new("https://example.org/contract#missing").unwrap(),
            body: serde_json::json!({}),
            policy,
        };
        let registry = MethodRegistry::new([method]).unwrap();

        assert!(matches!(
            builtin_procedure_compiler().validate_methods(&registry),
            Err(ProcedureMethodRegistryError::UnknownContract { .. })
        ));
    }

    #[test]
    fn a_rendered_template_is_validated_by_its_declared_contract() {
        let contract = ProcedureContractId::new(THERMAL_PROGRAM_V1).unwrap();
        let intent = crate::workflow::ir::synthetic_intent("https://example.org/action");
        let error = builtin_procedure_compiler()
            .render_template(
                &contract,
                &serde_json::json!({}),
                &ProcedureProgramBuildContext {
                    intent: &intent,
                    input_count: 0,
                    outputs: &[],
                    parameters: &[],
                    materials: &[],
                },
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ProcedureProgramTemplateError::InvalidProgram {
                contract: actual,
                ..
            } if actual == contract
        ));
    }
}

//! Explicit registrations for device-neutral Procedure semantics.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use lab_capability::ProcedureContractId;
use serde_json::Value;
use thiserror::Error;

use crate::procedure::binding::{MaterialReferencePolicy, ProcedureProgramInterface};
use crate::procedure::vocabulary::{PIPETTING_PROGRAM_V1, THERMAL_PROGRAM_V1};
use crate::procedure::{
    CapabilityFormula, PipettingProgramV1, ProgramFeature, ThermalProgramV1, VesselRole,
    pipetting_features, thermal_features,
};

/// Contract-derived facts used by every later compiler stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcedureContractAnalysis {
    pub interface: ProcedureProgramInterface,
    pub capability_formula: CapabilityFormula,
    pub features: BTreeSet<ProgramFeature>,
}

/// One statically linked Procedure contract implementation.
#[derive(Clone)]
pub struct ProcedureContractRegistration {
    pub id: ProcedureContractId,
    analyze: fn(&Value) -> Result<ProcedureContractAnalysis, String>,
}

impl std::fmt::Debug for ProcedureContractRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcedureContractRegistration")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl ProcedureContractRegistration {
    pub fn new(
        id: ProcedureContractId,
        analyze: fn(&Value) -> Result<ProcedureContractAnalysis, String>,
    ) -> Self {
        Self { id, analyze }
    }

    pub(crate) fn analyze(&self, body: &Value) -> Result<ProcedureContractAnalysis, String> {
        (self.analyze)(body)
    }
}

/// The complete set of Procedure semantics available to one compiler composition.
#[derive(Clone, Debug, Default)]
pub struct ProcedureContractRegistry {
    registrations: BTreeMap<ProcedureContractId, ProcedureContractRegistration>,
}

impl ProcedureContractRegistry {
    pub fn new(
        registrations: impl IntoIterator<Item = ProcedureContractRegistration>,
    ) -> Result<Self, ProcedureContractRegistryError> {
        let mut by_id = BTreeMap::new();
        for registration in registrations {
            let id = registration.id.clone();
            if by_id.insert(id.clone(), registration).is_some() {
                return Err(ProcedureContractRegistryError::Duplicate { contract: id });
            }
        }
        Ok(Self {
            registrations: by_id,
        })
    }

    pub fn registration(&self, id: &ProcedureContractId) -> Option<&ProcedureContractRegistration> {
        self.registrations.get(id)
    }

    pub fn contracts(&self) -> impl Iterator<Item = &ProcedureContractId> {
        self.registrations.keys()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureContractRegistryError {
    #[error("Procedure contract `{contract}` is registered more than once")]
    Duplicate { contract: ProcedureContractId },
}

/// The built-in contracts linked into the default Lab toolchain.
pub fn builtin_procedure_contracts() -> &'static ProcedureContractRegistry {
    static REGISTRY: OnceLock<ProcedureContractRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        ProcedureContractRegistry::new([
            ProcedureContractRegistration::new(
                ProcedureContractId::new(PIPETTING_PROGRAM_V1)
                    .expect("the built-in Pipetting contract is an absolute IRI"),
                analyze_pipetting,
            ),
            ProcedureContractRegistration::new(
                ProcedureContractId::new(THERMAL_PROGRAM_V1)
                    .expect("the built-in Thermal contract is an absolute IRI"),
                analyze_thermal,
            ),
        ])
        .expect("built-in Procedure contract identities are unique")
    })
}

fn analyze_pipetting(body: &Value) -> Result<ProcedureContractAnalysis, String> {
    let validated = serde_json::from_value::<PipettingProgramV1>(body.clone())
        .map_err(|error| error.to_string())?
        .validate()
        .map_err(|error| error.to_string())?;
    let program = validated.as_program();
    Ok(ProcedureContractAnalysis {
        interface: ProcedureProgramInterface {
            inputs: program
                .vessels
                .iter()
                .filter_map(|vessel| match &vessel.role {
                    VesselRole::ProcedureInput { input }
                    | VesselRole::InputOutput { input, .. } => Some(*input),
                    _ => None,
                })
                .collect(),
            materials: program
                .materials
                .iter()
                .map(|material| material.id.clone())
                .collect(),
            outputs: program
                .outputs
                .iter()
                .map(|output| output.id.clone())
                .collect(),
            material_policy: MaterialReferencePolicy::Subset,
        },
        capability_formula: validated.capability_formula(),
        features: pipetting_features(program),
    })
}

fn analyze_thermal(body: &Value) -> Result<ProcedureContractAnalysis, String> {
    let validated = serde_json::from_value::<ThermalProgramV1>(body.clone())
        .map_err(|error| error.to_string())?
        .validate()
        .map_err(|error| error.to_string())?;
    let program = validated.as_program();
    Ok(ProcedureContractAnalysis {
        interface: ProcedureProgramInterface {
            inputs: [program.load.input].into_iter().collect(),
            materials: BTreeSet::new(),
            outputs: program.load.outputs.iter().cloned().collect(),
            material_policy: MaterialReferencePolicy::Exact,
        },
        capability_formula: validated.capability_formula(),
        features: thermal_features(program),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_analysis(_: &Value) -> Result<ProcedureContractAnalysis, String> {
        Err("not used".to_owned())
    }

    #[test]
    fn duplicate_contract_registrations_fail_deterministically() {
        let id = ProcedureContractId::new("https://example.org/procedure/TestV1").unwrap();
        let error = ProcedureContractRegistry::new([
            ProcedureContractRegistration::new(id.clone(), empty_analysis),
            ProcedureContractRegistration::new(id.clone(), empty_analysis),
        ])
        .unwrap_err();
        assert_eq!(
            error,
            ProcedureContractRegistryError::Duplicate { contract: id }
        );
    }

    #[test]
    fn the_builtin_registry_is_explicit_and_stable() {
        assert_eq!(
            builtin_procedure_contracts()
                .contracts()
                .map(ProcedureContractId::as_str)
                .collect::<Vec<_>>(),
            [PIPETTING_PROGRAM_V1, THERMAL_PROGRAM_V1]
        );
    }
}

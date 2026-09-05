use lab_capability::ProcedureContractId;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::procedure::contract::{ProcedureContractAnalysis, builtin_procedure_contracts};
use crate::procedure::vocabulary::{PIPETTING_PROGRAM_V1, THERMAL_PROGRAM_V1};
use crate::procedure::{
    PipettingProgramV1, ProcedureContractRegistry, ThermalProgramV1, ValidatedPipettingProgramV1,
    ValidatedThermalProgramV1,
};

/// An open, versioned operational Procedure payload.
///
/// The envelope remains ordinary serialized data so package and Python extensions do not depend on
/// compiler IR. A contract registry validates and projects the payload into a typed program before
/// planning or adapter code consumes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcedureProgram {
    pub contract: ProcedureContractId,
    pub body: Value,
}

impl ProcedureProgram {
    pub fn from_pipetting(program: &ValidatedPipettingProgramV1) -> Self {
        Self {
            contract: ProcedureContractId::new(PIPETTING_PROGRAM_V1)
                .expect("built-in Procedure contract is an absolute IRI"),
            body: serde_json::to_value(program.as_program())
                .expect("a typed pipetting program serializes infallibly"),
        }
    }

    pub fn from_thermal(program: &ValidatedThermalProgramV1) -> Self {
        Self {
            contract: ProcedureContractId::new(THERMAL_PROGRAM_V1)
                .expect("built-in Procedure contract is an absolute IRI"),
            body: serde_json::to_value(program.as_program())
                .expect("a typed thermal program serializes infallibly"),
        }
    }

    pub fn validate(&self) -> Result<ValidatedProcedureProgram, ProcedureProgramValidationError> {
        self.validate_with(builtin_procedure_contracts())
    }

    pub fn validate_with(
        &self,
        registry: &ProcedureContractRegistry,
    ) -> Result<ValidatedProcedureProgram, ProcedureProgramValidationError> {
        let registration = registry.registration(&self.contract).ok_or_else(|| {
            ProcedureProgramValidationError::UnknownContract {
                contract: self.contract.clone(),
            }
        })?;
        let analysis = registration.analyze(&self.body).map_err(|message| {
            ProcedureProgramValidationError::InvalidBody {
                contract: self.contract.clone(),
                message,
            }
        })?;
        Ok(ValidatedProcedureProgram {
            document: self.clone(),
            analysis,
        })
    }
}

/// One canonical program validated by the registry selected by the compiler composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedProcedureProgram {
    document: ProcedureProgram,
    analysis: ProcedureContractAnalysis,
}

impl ValidatedProcedureProgram {
    pub fn document(&self) -> &ProcedureProgram {
        &self.document
    }

    pub fn contract(&self) -> &ProcedureContractId {
        &self.document.contract
    }

    pub fn decode_body<T: DeserializeOwned>(
        &self,
        expected_contract: &str,
    ) -> Result<T, ProcedureProgramDecodeError> {
        if self.document.contract.as_str() != expected_contract {
            return Err(ProcedureProgramDecodeError::ContractMismatch {
                expected: expected_contract.to_owned(),
                actual: self.document.contract.clone(),
            });
        }
        serde_json::from_value(self.document.body.clone()).map_err(|error| {
            ProcedureProgramDecodeError::InvalidBody {
                contract: self.document.contract.clone(),
                message: error.to_string(),
            }
        })
    }

    pub fn pipetting(&self) -> Result<ValidatedPipettingProgramV1, ProcedureProgramDecodeError> {
        self.decode_body::<PipettingProgramV1>(PIPETTING_PROGRAM_V1)?
            .validate()
            .map_err(|error| ProcedureProgramDecodeError::InvalidBody {
                contract: self.document.contract.clone(),
                message: error.to_string(),
            })
    }

    pub fn thermal(&self) -> Result<ValidatedThermalProgramV1, ProcedureProgramDecodeError> {
        self.decode_body::<ThermalProgramV1>(THERMAL_PROGRAM_V1)?
            .validate()
            .map_err(|error| ProcedureProgramDecodeError::InvalidBody {
                contract: self.document.contract.clone(),
                message: error.to_string(),
            })
    }

    pub(crate) fn analysis(&self) -> &ProcedureContractAnalysis {
        &self.analysis
    }

    /// Derive the exact facility capability formula required to realize this program.
    pub fn capability_formula(&self) -> crate::procedure::CapabilityFormula {
        self.analysis.capability_formula.clone()
    }

    /// Every fine-grained feature an implementation must declare to realize this program.
    pub fn features(&self) -> std::collections::BTreeSet<crate::procedure::ProgramFeature> {
        self.analysis.features.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureProgramValidationError {
    #[error("Procedure contract `{contract}` is not registered in this build")]
    UnknownContract { contract: ProcedureContractId },
    #[error("Procedure contract `{contract}` has an invalid payload: {message}")]
    InvalidBody {
        contract: ProcedureContractId,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureProgramDecodeError {
    #[error("expected Procedure contract `{expected}`, found `{actual}`")]
    ContractMismatch {
        expected: String,
        actual: ProcedureContractId,
    },
    #[error("Procedure contract `{contract}` has a body this consumer cannot decode: {message}")]
    InvalidBody {
        contract: ProcedureContractId,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use crate::procedure::{
        Duration, FluidPathPolicy, Location, MaterialInput, MaterialOutput, PipettingConstraints,
        PipettingStep, ProcedureLocalId, Temperature, ThermalLoad, ThermalProgramV1, ThermalStage,
        ThermalStep, Vessel, VesselRole, Volume,
    };

    use super::*;

    fn id(value: &str) -> ProcedureLocalId {
        ProcedureLocalId::new(value).unwrap()
    }

    fn pipetting() -> ValidatedPipettingProgramV1 {
        PipettingProgramV1::new(
            vec![MaterialInput { id: id("source") }],
            vec![MaterialOutput { id: id("product") }],
            vec![
                Vessel {
                    id: id("source-vessel"),
                    role: VesselRole::MaterialSource {
                        material: id("source"),
                    },
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
                Vessel {
                    id: id("destination-vessel"),
                    role: VesselRole::Product {
                        output: id("product"),
                    },
                    positions: 1,
                    working_capacity_each: None,
                    dead_volume_each: None,
                    initial_volume_each: None,
                    temperature: None,
                },
            ],
            vec![PipettingStep::Transfer {
                id: id("transfer"),
                source: Location {
                    vessel: id("source-vessel"),
                    position: 0,
                },
                destination: Location {
                    vessel: id("destination-vessel"),
                    position: 0,
                },
                volume: Volume::parse_microlitres("1.25").unwrap(),
                fluid_path: FluidPathPolicy::IsolatedDestinations,
                fluid_path_group: None,
                technique: Default::default(),
            }],
            PipettingConstraints::default(),
        )
        .validate()
        .unwrap()
    }

    #[test]
    fn open_envelope_round_trips_and_reenters_the_typed_registry() {
        let document = ProcedureProgram::from_pipetting(&pipetting());
        let json = serde_json::to_string_pretty(&document).unwrap();
        let round_trip = serde_json::from_str::<ProcedureProgram>(&json).unwrap();
        assert_eq!(round_trip, document);
        let validated = round_trip.validate().unwrap();
        assert_eq!(validated.contract().as_str(), PIPETTING_PROGRAM_V1);
        validated
            .decode_body::<crate::procedure::PipettingProgramV1>(PIPETTING_PROGRAM_V1)
            .unwrap()
            .validate()
            .unwrap();
    }

    #[test]
    fn unknown_contracts_and_invalid_bodies_fail_closed() {
        let unknown = ProcedureProgram {
            contract: ProcedureContractId::new("https://example.org/Unknown").unwrap(),
            body: serde_json::json!({}),
        };
        assert!(matches!(
            unknown.validate(),
            Err(ProcedureProgramValidationError::UnknownContract { .. })
        ));

        let malformed = ProcedureProgram {
            contract: ProcedureContractId::new(PIPETTING_PROGRAM_V1).unwrap(),
            body: serde_json::json!({"steps": []}),
        };
        assert!(matches!(
            malformed.validate(),
            Err(ProcedureProgramValidationError::InvalidBody { .. })
        ));
    }

    #[test]
    fn thermal_envelope_round_trips_and_reenters_the_typed_registry() {
        let thermal = ThermalProgramV1 {
            load: ThermalLoad {
                input: 0,
                outputs: vec![id("product")],
                sample_count: 1,
                volume_each: Volume::parse_microlitres("20").unwrap(),
            },
            lid_temperature: Some(Temperature::parse_degrees_celsius("105").unwrap()),
            stages: vec![ThermalStage {
                id: id("cycle"),
                repeats: 10,
                steps: vec![ThermalStep {
                    id: id("step"),
                    temperature: Temperature::parse_degrees_celsius("37").unwrap(),
                    hold: Duration::parse_seconds("60").unwrap(),
                    ramp_rate: None,
                }],
            }],
            final_hold: Some(Temperature::parse_degrees_celsius("4").unwrap()),
        }
        .validate()
        .unwrap();
        let document = ProcedureProgram::from_thermal(&thermal);
        let json = serde_json::to_string_pretty(&document).unwrap();
        let round_trip = serde_json::from_str::<ProcedureProgram>(&json).unwrap();
        assert_eq!(round_trip, document);
        let validated = round_trip.validate().unwrap();
        assert_eq!(validated.contract().as_str(), THERMAL_PROGRAM_V1);
        validated
            .decode_body::<ThermalProgramV1>(THERMAL_PROGRAM_V1)
            .unwrap()
            .validate()
            .unwrap();
    }
}

use lab_capability::ProcedureContractId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::vocabulary::{PIPETTING_PROGRAM_V1, THERMAL_PROGRAM_V1};
use crate::{
    PipettingProgramV1, PipettingProgramValidationError, ThermalProgramV1,
    ThermalProgramValidationError, ValidatedPipettingProgramV1, ValidatedThermalProgramV1,
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
        match self.contract.as_str() {
            PIPETTING_PROGRAM_V1 => {
                let program = serde_json::from_value::<PipettingProgramV1>(self.body.clone())
                    .map_err(|error| ProcedureProgramValidationError::InvalidBody {
                        contract: self.contract.clone(),
                        message: error.to_string(),
                    })?
                    .validate()?;
                Ok(ValidatedProcedureProgram::PipettingV1(program))
            }
            THERMAL_PROGRAM_V1 => {
                let program = serde_json::from_value::<ThermalProgramV1>(self.body.clone())
                    .map_err(|error| ProcedureProgramValidationError::InvalidBody {
                        contract: self.contract.clone(),
                        message: error.to_string(),
                    })?
                    .validate()?;
                Ok(ValidatedProcedureProgram::ThermalV1(program))
            }
            _ => Err(ProcedureProgramValidationError::UnknownContract {
                contract: self.contract.clone(),
            }),
        }
    }
}

/// One typed built-in program returned by the current contract registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidatedProcedureProgram {
    PipettingV1(ValidatedPipettingProgramV1),
    ThermalV1(ValidatedThermalProgramV1),
}

impl ValidatedProcedureProgram {
    /// Derive the exact facility capability formula required to realize this program.
    pub fn capability_formula(&self) -> crate::CapabilityFormula {
        match self {
            Self::PipettingV1(program) => program.capability_formula(),
            Self::ThermalV1(program) => program.capability_formula(),
        }
    }

    /// Every fine-grained feature an implementation must declare to realize this program.
    pub fn features(&self) -> std::collections::BTreeSet<crate::ProgramFeature> {
        match self {
            Self::PipettingV1(program) => crate::pipetting_features(program.as_program()),
            Self::ThermalV1(program) => crate::thermal_features(program.as_program()),
        }
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
    #[error(transparent)]
    InvalidPipetting(#[from] PipettingProgramValidationError),
    #[error(transparent)]
    InvalidThermal(#[from] ThermalProgramValidationError),
}

#[cfg(test)]
mod tests {
    use crate::{
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
                    initial_volume_each: None,
                },
                Vessel {
                    id: id("destination-vessel"),
                    role: VesselRole::Product {
                        output: id("product"),
                    },
                    positions: 1,
                    initial_volume_each: None,
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
        assert!(matches!(
            round_trip.validate().unwrap(),
            ValidatedProcedureProgram::PipettingV1(_)
        ));
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
        assert!(matches!(
            round_trip.validate().unwrap(),
            ValidatedProcedureProgram::ThermalV1(_)
        ));
    }
}

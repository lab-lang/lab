use lab_capability::ProcedureContractId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::vocabulary::PIPETTING_PROGRAM_V1;
use crate::{PipettingProgramV1, PipettingProgramValidationError, ValidatedPipettingProgramV1};

/// An open, versioned operational Procedure payload.
///
/// The envelope remains ordinary serialized data so package and Python extensions do not depend on
/// compiler IR. A contract registry validates and projects the payload into a typed program before
/// planning or adapter code consumes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
}

#[cfg(test)]
mod tests {
    use crate::{
        FluidPathPolicy, Location, MaterialInput, MaterialOutput, PipettingConstraints,
        PipettingStep, ProcedureLocalId, Vessel, VesselRole, Volume,
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
                },
                Vessel {
                    id: id("destination-vessel"),
                    role: VesselRole::Product {
                        output: id("product"),
                    },
                    positions: 1,
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
}

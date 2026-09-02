use std::collections::BTreeSet;

use super::error::PipettingProgramValidationError;
use super::ledger::{LiquidLedger, build_liquid_ledger};
use super::operation::PipettingStep;
use super::program::PipettingProgramV1;
use super::vessel::{Location, VesselRole};
use crate::procedure::ProcedureLocalId;

impl PipettingProgramV1 {
    pub fn validate(self) -> Result<ValidatedPipettingProgramV1, PipettingProgramValidationError> {
        if self.vessels.is_empty() {
            return Err(PipettingProgramValidationError::NoVessels);
        }
        if self.steps.is_empty() {
            return Err(PipettingProgramValidationError::NoSteps);
        }

        let mut material_ids = BTreeSet::new();
        for material in &self.materials {
            if !material_ids.insert(material.id.clone()) {
                return Err(PipettingProgramValidationError::DuplicateMaterial {
                    material: material.id.clone(),
                });
            }
        }
        let mut output_ids = BTreeSet::new();
        for output in &self.outputs {
            if !output_ids.insert(output.id.clone()) {
                return Err(PipettingProgramValidationError::DuplicateOutput {
                    output: output.id.clone(),
                });
            }
        }
        let mut vessel_ids = BTreeSet::new();
        for vessel in &self.vessels {
            if !vessel_ids.insert(vessel.id.clone()) {
                return Err(PipettingProgramValidationError::DuplicateVessel {
                    vessel: vessel.id.clone(),
                });
            }
            if vessel.positions == 0 {
                return Err(PipettingProgramValidationError::EmptyVessel {
                    vessel: vessel.id.clone(),
                });
            }
            match &vessel.role {
                VesselRole::MaterialSource { material }
                | VesselRole::MaterialProduct { material, .. }
                    if !material_ids.contains(material) =>
                {
                    return Err(PipettingProgramValidationError::UnknownMaterial {
                        vessel: vessel.id.clone(),
                        material: material.clone(),
                    });
                }
                VesselRole::Product { output }
                | VesselRole::InputOutput { output, .. }
                | VesselRole::MaterialProduct { output, .. }
                    if !output_ids.contains(output) =>
                {
                    return Err(PipettingProgramValidationError::UnknownOutput {
                        vessel: vessel.id.clone(),
                        output: output.clone(),
                    });
                }
                _ => {}
            }
        }

        let vessels = self
            .vessels
            .iter()
            .map(|vessel| (&vessel.id, vessel.positions))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut step_ids = BTreeSet::new();
        for step in &self.steps {
            if !step_ids.insert(step.id().clone()) {
                return Err(PipettingProgramValidationError::DuplicateStep {
                    step: step.id().clone(),
                });
            }
            match step {
                PipettingStep::Transfer {
                    id,
                    source,
                    destination,
                    ..
                } => {
                    validate_location(id, source, &vessels)?;
                    validate_location(id, destination, &vessels)?;
                    if source == destination {
                        return Err(PipettingProgramValidationError::SelfTransfer {
                            step: id.clone(),
                        });
                    }
                }
                PipettingStep::Distribute {
                    id,
                    source,
                    destinations,
                    ..
                } => {
                    validate_location(id, source, &vessels)?;
                    if destinations.is_empty() {
                        return Err(PipettingProgramValidationError::EmptyTargets {
                            step: id.clone(),
                        });
                    }
                    require_unique_targets(id, destinations)?;
                    for destination in destinations {
                        validate_location(id, destination, &vessels)?;
                        if source == destination {
                            return Err(PipettingProgramValidationError::SelfTransfer {
                                step: id.clone(),
                            });
                        }
                    }
                }
                PipettingStep::Mix {
                    id,
                    targets,
                    cycles,
                    ..
                } => {
                    if targets.is_empty() {
                        return Err(PipettingProgramValidationError::EmptyTargets {
                            step: id.clone(),
                        });
                    }
                    require_unique_targets(id, targets)?;
                    if *cycles == 0 {
                        return Err(PipettingProgramValidationError::ZeroMixCycles {
                            step: id.clone(),
                        });
                    }
                    for target in targets {
                        validate_location(id, target, &vessels)?;
                    }
                }
                PipettingStep::Barrier { id, reason } => {
                    if reason.trim().is_empty() {
                        return Err(PipettingProgramValidationError::EmptyBarrierReason {
                            step: id.clone(),
                        });
                    }
                }
            }
        }
        if !self
            .steps
            .iter()
            .any(|step| !matches!(step, PipettingStep::Barrier { .. }))
        {
            return Err(PipettingProgramValidationError::NoLiquidOperations);
        }
        let ledger = build_liquid_ledger(&self)?;
        Ok(ValidatedPipettingProgramV1 {
            program: self,
            ledger,
        })
    }
}

/// A pipetting program whose contract, graph, references, and operation bounds are valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPipettingProgramV1 {
    program: PipettingProgramV1,
    ledger: LiquidLedger,
}

impl ValidatedPipettingProgramV1 {
    pub fn as_program(&self) -> &PipettingProgramV1 {
        &self.program
    }

    /// The deterministic liquid effects proven while validating this program.
    pub fn liquid_ledger(&self) -> &LiquidLedger {
        &self.ledger
    }
}

impl AsRef<PipettingProgramV1> for ValidatedPipettingProgramV1 {
    fn as_ref(&self) -> &PipettingProgramV1 {
        self.as_program()
    }
}

fn validate_location(
    step: &ProcedureLocalId,
    location: &Location,
    vessels: &std::collections::BTreeMap<&ProcedureLocalId, u32>,
) -> Result<(), PipettingProgramValidationError> {
    let Some(positions) = vessels.get(&location.vessel) else {
        return Err(PipettingProgramValidationError::UnknownVessel {
            step: step.clone(),
            vessel: location.vessel.clone(),
        });
    };
    if location.position >= *positions {
        return Err(PipettingProgramValidationError::PositionOutOfRange {
            step: step.clone(),
            vessel: location.vessel.clone(),
            position: location.position,
            positions: *positions,
        });
    }
    Ok(())
}

fn require_unique_targets(
    step: &ProcedureLocalId,
    targets: &[Location],
) -> Result<(), PipettingProgramValidationError> {
    let mut unique = BTreeSet::new();
    for target in targets {
        if !unique.insert(target.clone()) {
            return Err(PipettingProgramValidationError::DuplicateTarget {
                step: step.clone(),
                vessel: target.vessel.clone(),
                position: target.position,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::operation::PipettingStep;
    use super::super::program::test_support::{example, location};
    use super::PipettingProgramValidationError;

    #[test]
    fn validation_rejects_dangling_and_out_of_range_locations() {
        let mut program = example();
        let PipettingStep::Distribute { destinations, .. } = &mut program.steps[0] else {
            unreachable!()
        };
        destinations[0] = location("missing", 0);
        assert!(matches!(
            program.validate(),
            Err(PipettingProgramValidationError::UnknownVessel { .. })
        ));

        let mut program = example();
        let PipettingStep::Distribute { destinations, .. } = &mut program.steps[0] else {
            unreachable!()
        };
        destinations[0] = location("reactions", 2);
        assert!(matches!(
            program.validate(),
            Err(PipettingProgramValidationError::PositionOutOfRange { .. })
        ));
    }

    #[test]
    fn validation_rejects_duplicate_ids_and_empty_operations() {
        let mut program = example();
        program.vessels.push(program.vessels[0].clone());
        assert!(matches!(
            program.validate(),
            Err(PipettingProgramValidationError::DuplicateVessel { .. })
        ));

        let mut program = example();
        program.steps.clear();
        assert_eq!(
            program.validate().unwrap_err(),
            PipettingProgramValidationError::NoSteps
        );
    }
}

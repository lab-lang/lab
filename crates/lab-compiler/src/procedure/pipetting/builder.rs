//! Checked construction of canonical pipetting programs.

use std::collections::BTreeSet;

use super::{
    FluidPathPolicy, Location, MaterialInput, MaterialOutput, MixTechnique, PipettingConstraints,
    PipettingProgramV1, PipettingStep, TransferTechnique, ValidatedPipettingProgramV1, Vessel,
    VesselRole,
};
use crate::procedure::{ProcedureLocalId, Volume};

/// A declared logical vessel. Position lookup checks the declared extent.
#[derive(Clone, Debug)]
pub struct VesselHandle {
    id: ProcedureLocalId,
    positions: u32,
}

impl VesselHandle {
    pub fn position(&self, position: u32) -> Result<Location, String> {
        if position >= self.positions {
            return Err(format!(
                "vessel `{}` has {} positions, requested {position}",
                self.id, self.positions
            ));
        }
        Ok(Location {
            vessel: self.id.clone(),
            position,
        })
    }

    pub fn positions(&self) -> Vec<Location> {
        (0..self.positions)
            .map(|position| Location {
                vessel: self.id.clone(),
                position,
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct TransferSettings {
    pub volume: Volume,
    pub technique: TransferTechnique,
}

#[derive(Clone, Debug)]
pub struct MixSettings {
    pub cycles: u32,
    pub volume: Volume,
    pub technique: MixTechnique,
}

/// Builds the ordinary, serializable pipetting contract. `finish` performs the same
/// validation and exact liquid accounting as a program authored directly as data.
pub struct PipettingBuilder {
    program: PipettingProgramV1,
    vessel_ids: BTreeSet<ProcedureLocalId>,
    step_ids: BTreeSet<ProcedureLocalId>,
}

impl Default for PipettingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PipettingBuilder {
    pub fn new() -> Self {
        Self {
            program: PipettingProgramV1::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                PipettingConstraints::default(),
            ),
            vessel_ids: BTreeSet::new(),
            step_ids: BTreeSet::new(),
        }
    }

    /// Declare a vessel with the full canonical set of material and technique constraints.
    pub fn vessel(&mut self, vessel: Vessel) -> Result<VesselHandle, String> {
        if vessel.positions == 0 {
            return Err(format!(
                "vessel `{}` needs at least one position",
                vessel.id
            ));
        }
        if !self.vessel_ids.insert(vessel.id.clone()) {
            return Err(format!("duplicate vessel `{}`", vessel.id));
        }
        match &vessel.role {
            VesselRole::MaterialSource { material }
            | VesselRole::MaterialProduct { material, .. }
                if !self
                    .program
                    .materials
                    .iter()
                    .any(|input| input.id == *material) =>
            {
                self.program.materials.push(MaterialInput {
                    id: material.clone(),
                });
            }
            _ => {}
        }
        match &vessel.role {
            VesselRole::Product { output }
            | VesselRole::InputOutput { output, .. }
            | VesselRole::MaterialProduct { output, .. }
                if !self
                    .program
                    .outputs
                    .iter()
                    .any(|product| product.id == *output) =>
            {
                self.program
                    .outputs
                    .push(MaterialOutput { id: output.clone() });
            }
            _ => {}
        }
        let handle = VesselHandle {
            id: vessel.id.clone(),
            positions: vessel.positions,
        };
        self.program.vessels.push(vessel);
        Ok(handle)
    }

    pub fn input(
        &mut self,
        name: &str,
        input: u32,
        positions: u32,
        initial_volume_each: Volume,
    ) -> Result<VesselHandle, String> {
        self.vessel(Vessel {
            id: id(name)?,
            role: VesselRole::ProcedureInput { input },
            positions,
            initial_volume_each: Some(initial_volume_each),
            working_capacity_each: None,
            dead_volume_each: None,
            temperature: None,
        })
    }

    pub fn source(
        &mut self,
        name: &str,
        material: ProcedureLocalId,
        initial_volume: Option<Volume>,
        retained_volume: Option<Volume>,
    ) -> Result<VesselHandle, String> {
        self.vessel(Vessel {
            id: id(name)?,
            role: VesselRole::MaterialSource { material },
            positions: 1,
            initial_volume_each: initial_volume,
            working_capacity_each: None,
            dead_volume_each: retained_volume,
            temperature: None,
        })
    }

    pub fn product(
        &mut self,
        name: &str,
        output: ProcedureLocalId,
        positions: u32,
    ) -> Result<VesselHandle, String> {
        self.vessel(Vessel {
            id: id(name)?,
            role: VesselRole::Product { output },
            positions,
            initial_volume_each: None,
            working_capacity_each: None,
            dead_volume_each: None,
            temperature: None,
        })
    }

    /// Append any canonical operation; its stable ID must be unique within the program.
    pub fn step(&mut self, step: PipettingStep) -> Result<(), String> {
        if !self.step_ids.insert(step.id().clone()) {
            return Err(format!("duplicate step `{}`", step.id()));
        }
        self.program.steps.push(step);
        Ok(())
    }

    pub fn distribute(
        &mut self,
        name: &str,
        source: Location,
        destinations: Vec<Location>,
        settings: &TransferSettings,
        policy: FluidPathPolicy,
    ) -> Result<(), String> {
        self.step(PipettingStep::Distribute {
            id: id(name)?,
            source,
            destinations,
            volume_each: settings.volume.clone(),
            fluid_path: policy,
            fluid_path_group: None,
            technique: settings.technique.clone(),
        })
    }

    /// Borrow the builder for one contiguous sequence sharing an explicit fluid path.
    /// Steps keep the supplied identities, so diagnostics and reviewed artifacts remain stable.
    pub fn continuous_path(
        &mut self,
        name: &str,
        policy: FluidPathPolicy,
    ) -> Result<FluidPath<'_>, String> {
        Ok(FluidPath {
            builder: self,
            group: id(name)?,
            policy,
        })
    }

    pub fn finish(self) -> Result<ValidatedPipettingProgramV1, String> {
        self.program.validate().map_err(|error| error.to_string())
    }
}

pub struct FluidPath<'a> {
    builder: &'a mut PipettingBuilder,
    group: ProcedureLocalId,
    policy: FluidPathPolicy,
}

impl FluidPath<'_> {
    pub fn transfer(
        &mut self,
        name: &str,
        source: Location,
        destination: Location,
        settings: &TransferSettings,
    ) -> Result<(), String> {
        self.builder.step(PipettingStep::Transfer {
            id: id(name)?,
            source,
            destination,
            volume: settings.volume.clone(),
            fluid_path: self.policy,
            fluid_path_group: Some(self.group.clone()),
            technique: settings.technique.clone(),
        })
    }

    pub fn mix(
        &mut self,
        name: &str,
        target: Location,
        settings: &MixSettings,
    ) -> Result<(), String> {
        self.builder.step(PipettingStep::Mix {
            id: id(name)?,
            targets: vec![target],
            cycles: settings.cycles,
            volume: settings.volume.clone(),
            fluid_path: self.policy,
            fluid_path_group: Some(self.group.clone()),
            technique: settings.technique.clone(),
        })
    }
}

fn id(name: &str) -> Result<ProcedureLocalId, String> {
    ProcedureLocalId::new(name).map_err(|error| error.to_string())
}

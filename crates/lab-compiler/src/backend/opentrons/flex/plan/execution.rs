//! Resource-allocated execution plan owned by Flex planning.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::backend::opentrons::flex::profile::FlexAdapterProfile;

pub use crate::backend::resources::Well as FlexWell;

/// Every well, rack position, and replicate the robot will use, allocated once
/// and shared by every emitted artifact. The plan is split by the artifact kind
/// that produces it: a plasmid is assembled, and a strain is transformed,
/// recovered, diluted, and plated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexExecutionPlan {
    pub schema_version: String,
    /// The explicit adapter implementation that produced this device plan.
    pub adapter: String,
    /// Checked implementation configuration for the allocated Asset binding.
    pub deck: FlexAdapterProfile,
    pub assembly_source_wells: BTreeMap<String, String>,
    pub transformation_source_wells: BTreeMap<String, String>,
    /// DNA-plate well holding each plasmid a strain is transformed from. A
    /// plasmid assembled in the same batch is carried over from its assembly
    /// plate; one retrieved from inventory is loaded by the operator.
    pub dna_source_wells: BTreeMap<String, FlexWell>,
    pub assemblies: Vec<FlexAssemblyPlan>,
    pub strains: Vec<FlexStrainPlan>,
}

/// One plasmid artifact assembled in the thermocycler plate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexAssemblyPlan {
    pub artifact: String,
    pub sequence: String,
    pub backbone: String,
    pub components: Vec<String>,
    pub dependencies: Vec<String>,
    pub restriction_enzyme: String,
    pub assembly_replicates: u8,
    pub water_volume_ul: u16,
    pub assembly_wells: Vec<String>,
    pub chemistry: FlexAssemblyChemistry,
}

/// Golden Gate reaction parameters stated by the plasmid design.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexAssemblyChemistry {
    pub reaction_volume_ul: u16,
    pub part_volume_ul: u16,
    pub enzyme_volume_ul: u16,
    pub ligase_volume_ul: u16,
    pub buffer_volume_ul: u16,
    pub cycles: u16,
    pub digest_temperature_c: u16,
    pub digest_minutes: u16,
    pub ligate_temperature_c: u16,
    pub ligate_minutes: u16,
}

/// One strain artifact transformed from assembled plasmids and plated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexStrainPlan {
    pub artifact: String,
    pub host: String,
    pub plasmids: Vec<String>,
    pub dependencies: Vec<String>,
    pub selection: String,
    pub transformation_replicates: u8,
    pub plating_replicates: u8,
    pub serial_dilutions: u8,
    pub transformations: Vec<FlexTransformationPlan>,
    pub plating: Vec<FlexPlatingPlan>,
    pub chemistry: FlexStrainChemistry,
}

/// Heat-shock transformation and plating parameters stated by the strain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexStrainChemistry {
    pub cell_volume_ul: u16,
    pub dna_volume_ul: u16,
    pub recovery_volume_ul: u16,
    pub cold_minutes: u16,
    pub heat_shock_temperature_c: u16,
    pub heat_shock_minutes: u16,
    pub recovery_temperature_c: u16,
    pub recovery_minutes: u16,
    pub medium_volume_ul: u16,
    pub culture_volume_ul: u16,
    pub colony_volume_ul: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexTransformationPlan {
    pub culture_well: String,
    /// DNA-plate wells whose contents enter this reaction. A strain carrying
    /// several plasmids receives all of them in one well.
    pub source_wells: Vec<FlexWell>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexPlatingPlan {
    pub culture_well: String,
    pub dilution_wells: Vec<FlexWell>,
    pub agar_wells: Vec<Vec<FlexWell>>,
}

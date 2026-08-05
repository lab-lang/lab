//! Resource-allocated execution plan owned by OT-2 planning.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ot2ExecutionPlan {
    pub schema_version: String,
    pub target: String,
    pub api_level: String,
    pub assembly_source_wells: BTreeMap<String, String>,
    pub transformation_source_wells: BTreeMap<String, String>,
    pub constructs: Vec<Ot2ConstructPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ot2ConstructPlan {
    pub artifact: String,
    pub sequence: String,
    pub backbone: String,
    pub components: Vec<String>,
    pub steps: Vec<String>,
    pub restriction_enzyme: String,
    pub host: String,
    pub selection: String,
    pub assembly_replicates: u8,
    pub transformation_replicates: u8,
    pub plating_replicates: u8,
    pub serial_dilutions: u8,
    pub water_volume_ul: u16,
    pub assembly_wells: Vec<String>,
    pub transformations: Vec<Ot2TransformationPlan>,
    pub plating: Vec<Ot2PlatingPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ot2TransformationPlan {
    pub assembly_well: String,
    pub culture_well: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ot2PlatingPlan {
    pub culture_well: String,
    pub dilution_wells: Vec<String>,
    pub agar_wells: Vec<Vec<String>>,
}

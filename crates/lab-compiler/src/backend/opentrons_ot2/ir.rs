//! Build and execution IRs owned by the OT-2 backend.
//!
//! `Ot2BuildIr` is the biological build shape accepted by this specialization.
//! `Ot2ExecutionPlan` is the fully validated and resource-allocated robot plan
//! consumed by all OT-2 emitters. Keeping both representations explicit makes
//! target planning inspectable without teaching the Lab AST about this robot.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Ot2BuildIrError {
    #[error("the OT-2 build IR contains no artifacts")]
    Empty,
    #[error("artifact '{0}' appears more than once in the OT-2 build IR")]
    DuplicateArtifact(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ot2BuildIr {
    artifacts: Vec<Ot2BuildArtifact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ot2BuildArtifact {
    pub name: String,
    pub sequence: String,
    pub dependencies: Vec<String>,
    pub recipe: Ot2BuildRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ot2BuildRecipe {
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
}

impl Ot2BuildArtifact {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sequence(&self) -> &str {
        &self.sequence
    }

    pub fn build_recipe(&self) -> &Ot2BuildRecipe {
        &self.recipe
    }
}

impl Ot2BuildRecipe {
    pub fn backbone(&self) -> &str {
        &self.backbone
    }

    pub fn components(&self) -> &[String] {
        &self.components
    }

    pub fn steps(&self) -> &[String] {
        &self.steps
    }

    pub fn restriction_enzyme(&self) -> &str {
        &self.restriction_enzyme
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn selection(&self) -> &str {
        &self.selection
    }

    pub fn assembly_replicates(&self) -> u8 {
        self.assembly_replicates
    }

    pub fn transformation_replicates(&self) -> u8 {
        self.transformation_replicates
    }

    pub fn plating_replicates(&self) -> u8 {
        self.plating_replicates
    }

    pub fn serial_dilutions(&self) -> u8 {
        self.serial_dilutions
    }
}

impl Ot2BuildIr {
    pub fn new(artifacts: Vec<Ot2BuildArtifact>) -> Result<Self, Ot2BuildIrError> {
        if artifacts.is_empty() {
            return Err(Ot2BuildIrError::Empty);
        }
        let mut names = BTreeSet::new();
        for artifact in &artifacts {
            if !names.insert(artifact.name.clone()) {
                return Err(Ot2BuildIrError::DuplicateArtifact(artifact.name.clone()));
            }
        }
        Ok(Self { artifacts })
    }

    pub fn artifacts(&self) -> &[Ot2BuildArtifact] {
        &self.artifacts
    }
}

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

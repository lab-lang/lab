use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use thiserror::Error;

use crate::{ArtifactBundle, ArtifactError};

use super::emit::{
    render_assembly_protocol, render_manual_protocol, render_plating_protocol,
    render_transformation_protocol,
};
use super::{
    Ot2BuildArtifact, Ot2BuildIr, Ot2BuildRecipe, Ot2ConstructPlan, Ot2ExecutionPlan,
    Ot2PlatingPlan, Ot2TransformationPlan, TargetConstraintError,
};

const API_LEVEL: &str = "2.21";
const TARGET: &str = "opentrons_ot2";
const PLATE_CAPACITY: usize = 96;
const SOURCE_RACK_CAPACITY: usize = 24;
const TIP_RACK_CAPACITY: usize = 96;
const REACTION_VOLUME_UL: u16 = 20;
const SUPPORTED_STEPS: [&str; 5] = ["assemble", "transform", "recover", "dilute", "plate"];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Ot2PlanningError {
    #[error(transparent)]
    Constraint(Box<TargetConstraintError>),
}

impl From<TargetConstraintError> for Ot2PlanningError {
    fn from(error: TargetConstraintError) -> Self {
        Self::Constraint(Box::new(error))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Ot2EmissionError {
    #[error("failed to serialize the generated automation plan: {0}")]
    Serialization(String),
    #[error("invalid OT-2 Python template '{template}': {message}")]
    Template {
        template: &'static str,
        message: String,
    },
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Ot2BuildError {
    #[error(transparent)]
    Planning(#[from] Ot2PlanningError),
    #[error(transparent)]
    Emission(#[from] Ot2EmissionError),
}

/// Planned OT-2 program together with its emitted artifact package.
#[derive(Clone, Debug)]
pub struct Ot2Bundle {
    manifest: Ot2ExecutionPlan,
    artifacts: ArtifactBundle,
}

impl Ot2Bundle {
    pub(super) fn from_plan(manifest: Ot2ExecutionPlan) -> Result<Self, Ot2EmissionError> {
        let mut artifacts = ArtifactBundle::new();
        artifacts.insert_text(
            "automation_manifest.json",
            "application/json",
            pretty_json(&manifest)?,
        )?;
        artifacts.insert_text(
            "manual_protocol.md",
            "text/markdown",
            render_manual_protocol(&manifest),
        )?;
        artifacts.insert_text(
            "assembly_protocol.py",
            "text/x-python",
            render_assembly_protocol(&manifest)?,
        )?;
        artifacts.insert_text(
            "transformation_protocol.py",
            "text/x-python",
            render_transformation_protocol(&manifest)?,
        )?;
        artifacts.insert_text(
            "plating_protocol.py",
            "text/x-python",
            render_plating_protocol(&manifest)?,
        )?;
        Ok(Self {
            manifest,
            artifacts,
        })
    }

    pub fn manifest(&self) -> &Ot2ExecutionPlan {
        &self.manifest
    }

    pub fn manifest_json(&self) -> Result<String, Ot2EmissionError> {
        Ok(self.artifact_text("automation_manifest.json").to_owned())
    }

    pub fn manual_protocol(&self) -> &str {
        self.artifact_text("manual_protocol.md")
    }

    pub fn assembly_protocol(&self) -> &str {
        self.artifact_text("assembly_protocol.py")
    }

    pub fn transformation_protocol(&self) -> &str {
        self.artifact_text("transformation_protocol.py")
    }

    pub fn plating_protocol(&self) -> &str {
        self.artifact_text("plating_protocol.py")
    }

    pub fn artifacts(&self) -> &ArtifactBundle {
        &self.artifacts
    }

    fn artifact_text(&self, path: &str) -> &str {
        self.artifacts
            .get(path)
            .expect("OT-2 bundle contains every declared artifact")
            .text_contents()
            .expect("OT-2 source artifacts are UTF-8")
    }
}

pub fn compile_build(build: &Ot2BuildIr) -> Result<Ot2Bundle, Ot2BuildError> {
    Ok(Ot2Bundle::from_plan(plan_build(build)?)?)
}

pub fn emit_program(program: &Ot2ExecutionPlan) -> Result<Ot2Bundle, Ot2EmissionError> {
    Ot2Bundle::from_plan(program.clone())
}

/// Validate and allocate an OT-2 build without rendering any output files.
///
/// The returned execution IR is the single input shared by the Python,
/// Markdown, and JSON emitters.
pub fn plan_build(build: &Ot2BuildIr) -> Result<Ot2ExecutionPlan, Ot2PlanningError> {
    let mut recipes = Vec::new();
    for artifact in build.artifacts() {
        let recipe = artifact.build_recipe();
        validate_target_constraints(artifact.name(), recipe)?;
        recipes.push((artifact, recipe));
    }
    validate_uniform_batch_settings(&recipes)?;

    let assembly_well_count = recipes
        .iter()
        .map(|(_, recipe)| usize::from(recipe.assembly_replicates()))
        .sum::<usize>();
    let transformation_count = recipes
        .iter()
        .map(|(_, recipe)| {
            usize::from(recipe.assembly_replicates())
                * usize::from(recipe.transformation_replicates())
        })
        .sum::<usize>();
    require_plate_capacity(
        "assembly and transformation",
        assembly_well_count + transformation_count,
    )?;
    let assembly_tip_count = recipes
        .iter()
        .map(|(_, recipe)| {
            usize::from(recipe.assembly_replicates()) * (recipe.components().len() + 6)
        })
        .sum();
    require_tip_capacity("assembly", "p20", assembly_tip_count)?;
    require_tip_capacity("transformation", "p20", transformation_count * 2)?;
    require_tip_capacity("transformation", "p300", transformation_count)?;

    let dilution_count = recipes
        .iter()
        .map(|(_, recipe)| {
            usize::from(recipe.assembly_replicates())
                * usize::from(recipe.transformation_replicates())
                * usize::from(recipe.serial_dilutions())
        })
        .sum::<usize>();
    require_plate_capacity("serial dilution", dilution_count)?;
    let agar_count = recipes
        .iter()
        .map(|(_, recipe)| {
            usize::from(recipe.assembly_replicates())
                * usize::from(recipe.transformation_replicates())
                * usize::from(recipe.serial_dilutions())
                * usize::from(recipe.plating_replicates())
        })
        .sum::<usize>();
    require_plate_capacity("plating", agar_count)?;
    let plating_tip_count = recipes
        .iter()
        .map(|(_, recipe)| {
            usize::from(recipe.assembly_replicates())
                * usize::from(recipe.transformation_replicates())
                * usize::from(recipe.serial_dilutions())
                * (1 + usize::from(recipe.plating_replicates()))
        })
        .sum();
    require_tip_capacity("plating", "p20", plating_tip_count)?;

    let assembly_sources = assembly_source_keys(&recipes);
    let transformation_sources = transformation_source_keys(&recipes);
    let assembly_source_wells = assign_source_wells("assembly", assembly_sources)?;
    let transformation_source_wells =
        assign_source_wells("transformation", transformation_sources)?;

    let wells = plate_wells();
    let mut assembly_cursor = 0;
    let mut transformation_cursor = assembly_well_count;
    let mut dilution_cursor = 0;
    let mut agar_cursor = 0;
    let mut constructs = Vec::new();
    for (artifact, recipe) in recipes {
        let assembly_wells = wells
            [assembly_cursor..assembly_cursor + usize::from(recipe.assembly_replicates())]
            .to_vec();
        assembly_cursor += assembly_wells.len();

        let mut transformations = Vec::new();
        let mut plating = Vec::new();
        for assembly_well in &assembly_wells {
            for _ in 0..recipe.transformation_replicates() {
                let culture_well = wells[transformation_cursor].clone();
                transformation_cursor += 1;
                transformations.push(Ot2TransformationPlan {
                    assembly_well: assembly_well.clone(),
                    culture_well: culture_well.clone(),
                });

                let dilution_end = dilution_cursor + usize::from(recipe.serial_dilutions());
                let dilution_wells = wells[dilution_cursor..dilution_end].to_vec();
                dilution_cursor = dilution_end;
                let mut agar_wells = Vec::new();
                for _ in 0..recipe.serial_dilutions() {
                    let agar_end = agar_cursor + usize::from(recipe.plating_replicates());
                    agar_wells.push(wells[agar_cursor..agar_end].to_vec());
                    agar_cursor = agar_end;
                }
                plating.push(Ot2PlatingPlan {
                    culture_well,
                    dilution_wells,
                    agar_wells,
                });
            }
        }

        let dna_components = 1 + recipe.components().len();
        let required_ul = dna_components as u16 * 2 + 8;
        constructs.push(Ot2ConstructPlan {
            artifact: artifact.name().to_owned(),
            sequence: artifact.sequence().to_owned(),
            backbone: recipe.backbone().to_owned(),
            components: recipe.components().to_vec(),
            steps: recipe.steps().to_vec(),
            restriction_enzyme: recipe.restriction_enzyme().to_owned(),
            host: recipe.host().to_owned(),
            selection: recipe.selection().to_owned(),
            assembly_replicates: recipe.assembly_replicates(),
            transformation_replicates: recipe.transformation_replicates(),
            plating_replicates: recipe.plating_replicates(),
            serial_dilutions: recipe.serial_dilutions(),
            water_volume_ul: 20 - required_ul,
            assembly_wells,
            transformations,
            plating,
        });
    }

    Ok(Ot2ExecutionPlan {
        schema_version: "lab.automation.v0".into(),
        target: TARGET.into(),
        api_level: API_LEVEL.into(),
        assembly_source_wells,
        transformation_source_wells,
        constructs,
    })
}

fn validate_target_constraints(
    artifact: &str,
    recipe: &Ot2BuildRecipe,
) -> Result<(), Ot2PlanningError> {
    if recipe.steps() != SUPPORTED_STEPS {
        return Err(TargetConstraintError::UnsupportedOperationSequence {
            target: TARGET.into(),
            subject: artifact.into(),
            expected: SUPPORTED_STEPS.into_iter().map(str::to_owned).collect(),
            found: recipe.steps().to_vec(),
        }
        .into());
    }
    for (parameter, value, maximum) in [
        ("assembly_replicates", recipe.assembly_replicates(), u8::MAX),
        (
            "transformation_replicates",
            recipe.transformation_replicates(),
            u8::MAX,
        ),
        ("plating_replicates", recipe.plating_replicates(), 8),
        ("serial_dilutions", recipe.serial_dilutions(), 2),
    ] {
        if !(1..=maximum).contains(&value) {
            return Err(TargetConstraintError::ParameterOutOfRange {
                target: TARGET.into(),
                subject: artifact.into(),
                parameter: parameter.into(),
                minimum: 1,
                maximum: u64::from(maximum),
                found: u64::from(value),
            }
            .into());
        }
    }
    let required_ul = (1 + recipe.components().len()) as u16 * 2 + 8;
    if required_ul > REACTION_VOLUME_UL {
        return Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: "assembly".into(),
            subject: artifact.into(),
            resource: "reaction_volume".into(),
            required: u64::from(required_ul),
            capacity: u64::from(REACTION_VOLUME_UL),
            unit: "uL".into(),
        }
        .into());
    }
    Ok(())
}

fn validate_uniform_batch_settings(
    recipes: &[(&Ot2BuildArtifact, &Ot2BuildRecipe)],
) -> Result<(), Ot2PlanningError> {
    let Some((_, first)) = recipes.first() else {
        return Ok(());
    };
    let expected = (
        first.assembly_replicates(),
        first.transformation_replicates(),
        first.plating_replicates(),
        first.serial_dilutions(),
    );
    if recipes.iter().skip(1).any(|(_, recipe)| {
        (
            recipe.assembly_replicates(),
            recipe.transformation_replicates(),
            recipe.plating_replicates(),
            recipe.serial_dilutions(),
        ) != expected
    }) {
        Err(TargetConstraintError::NonUniformParameters {
            target: TARGET.into(),
            subject: "automation_batch".into(),
            parameters: vec![
                "assembly_replicates".into(),
                "transformation_replicates".into(),
                "plating_replicates".into(),
                "serial_dilutions".into(),
            ],
        }
        .into())
    } else {
        Ok(())
    }
}

fn require_plate_capacity(stage: &'static str, required: usize) -> Result<(), Ot2PlanningError> {
    if required > PLATE_CAPACITY {
        Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: "destination_plate".into(),
            required: required as u64,
            capacity: PLATE_CAPACITY as u64,
            unit: "wells".into(),
        }
        .into())
    } else {
        Ok(())
    }
}

fn require_tip_capacity(
    stage: &'static str,
    pipette: &'static str,
    required: usize,
) -> Result<(), Ot2PlanningError> {
    if required > TIP_RACK_CAPACITY {
        Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: format!("{pipette}_tip_rack"),
            required: required as u64,
            capacity: TIP_RACK_CAPACITY as u64,
            unit: "tips".into(),
        }
        .into())
    } else {
        Ok(())
    }
}

fn assembly_source_keys(recipes: &[(&Ot2BuildArtifact, &Ot2BuildRecipe)]) -> BTreeSet<String> {
    let mut keys = BTreeSet::from([
        "reagent:nuclease_free_water".into(),
        "reagent:T4_DNA_ligase".into(),
        "reagent:T4_DNA_ligase_buffer".into(),
    ]);
    for (_, recipe) in recipes {
        keys.insert(format!("dna:{}", recipe.backbone()));
        keys.extend(
            recipe
                .components()
                .iter()
                .map(|component| format!("dna:{component}")),
        );
        keys.insert(format!("enzyme:{}", recipe.restriction_enzyme()));
    }
    keys
}

fn transformation_source_keys(
    recipes: &[(&Ot2BuildArtifact, &Ot2BuildRecipe)],
) -> BTreeSet<String> {
    let mut keys = BTreeSet::from(["reagent:recovery_medium".into()]);
    keys.extend(
        recipes
            .iter()
            .map(|(_, recipe)| format!("cells:{}", recipe.host())),
    );
    keys
}

fn assign_source_wells(
    stage: &'static str,
    keys: BTreeSet<String>,
) -> Result<BTreeMap<String, String>, Ot2PlanningError> {
    if keys.len() > SOURCE_RACK_CAPACITY {
        return Err(TargetConstraintError::CapacityExceeded {
            target: TARGET.into(),
            operation: stage.into(),
            subject: "automation_batch".into(),
            resource: "source_rack".into(),
            required: keys.len() as u64,
            capacity: SOURCE_RACK_CAPACITY as u64,
            unit: "wells".into(),
        }
        .into());
    }
    let wells = source_rack_wells();
    Ok(keys.into_iter().zip(wells).collect::<BTreeMap<_, _>>())
}

fn plate_wells() -> Vec<String> {
    (1..=12)
        .flat_map(|column| (b'A'..=b'H').map(move |row| format!("{}{column}", char::from(row))))
        .collect()
}

fn source_rack_wells() -> Vec<String> {
    (1..=6)
        .flat_map(|column| (b'A'..=b'D').map(move |row| format!("{}{column}", char::from(row))))
        .collect()
}

fn pretty_json(value: &impl Serialize) -> Result<String, Ot2EmissionError> {
    serde_json::to_string_pretty(value)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| Ot2EmissionError::Serialization(error.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::backend::opentrons_ot2::{Ot2LoweringError, lower_build};
    use lab_language::compile_module;

    use super::*;

    const SOURCE: &str = r#"
use std.bio.build
use std.bio.inventory
use std.lab.plasmid_actions

record BuiltArtifact:
  product: Material<Plasmid>
  plate: Material<Plate>

J23101 = part("J23101")
B0034 = part("B0034")
GFP = part("GFP")
B0015 = part("B0015")
pSB1C3 = backbone("pSB1C3")
BsaI = restriction_enzyme("BsaI")
DH5alpha = strain("DH5alpha")
chloramphenicol = antibiotic("chloramphenicol")

plasmid p_gfp:
  sequence: dna("ACGT")
  backbone: pSB1C3
  components: [J23101, B0034, GFP, B0015]
  restriction_enzyme: BsaI
  host: DH5alpha
  selection: chloramphenicol
  assembly_replicates: 1
  transformation_replicates: 2
  plating_replicates: 2
  serial_dilutions: 2
  require topology == circular
  accept sequence == design.sequence

workflow realize_p_gfp() -> BuiltArtifact:
  dependencies = []
  product, construct <- realize p_gfp from dependencies
  cells <- provision DH5alpha
  culture <- transform construct into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return BuiltArtifact{product: product, plate: plate}
"#;

    #[test]
    fn compiles_all_three_protocol_stages() {
        let build = lower_build(&compile_module(SOURCE).unwrap()).unwrap();
        let plan = plan_build(&build).unwrap();
        let bundle = compile_build(&build).unwrap();

        assert_eq!(bundle.manifest(), &plan);
        assert_eq!(bundle.manifest.constructs[0].assembly_wells, ["A1"]);
        assert_eq!(
            bundle.manifest.constructs[0]
                .transformations
                .iter()
                .map(|reaction| reaction.culture_well.as_str())
                .collect::<Vec<_>>(),
            ["B1", "C1"]
        );
        assert!(
            bundle
                .manual_protocol()
                .contains("Stage 3 — Serial dilution and plating")
        );
        assert!(bundle.assembly_protocol().contains("repetitions=75"));
        assert!(
            bundle
                .transformation_protocol()
                .contains("hold_time_minutes=30")
        );
        assert!(bundle.plating_protocol().contains("p300.distribute("));
        assert_eq!(bundle.artifacts().len(), 5);
        assert!(bundle.artifacts().get("automation_manifest.json").is_some());
        assert!(bundle.artifacts().get("manual_protocol.md").is_some());
    }

    #[test]
    fn rejects_a_step_sequence_this_target_does_not_lower() {
        let build = lower_build(
            &compile_module(&SOURCE.replace(
                "  culture <- recover culture for 1 h\n  culture <- dilute culture\n",
                "",
            ))
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            compile_build(&build),
            Err(Ot2BuildError::Planning(Ot2PlanningError::Constraint(error)))
                if matches!(*error, TargetConstraintError::UnsupportedOperationSequence { .. })
        ));
    }

    #[test]
    fn target_metadata_requires_resolved_symbols_of_the_right_kind() {
        for source in [
            SOURCE.replace("  backbone: pSB1C3\n", "  backbone: \"pSB1C3\"\n"),
            SOURCE.replace("  backbone: pSB1C3\n", "  backbone: J23101\n"),
        ] {
            assert!(matches!(
                lower_build(&compile_module(&source).unwrap()),
                Err(Ot2LoweringError::InvalidField {
                    artifact,
                    field: "backbone"
                }) if artifact == "p_gfp"
            ));
        }
    }
}

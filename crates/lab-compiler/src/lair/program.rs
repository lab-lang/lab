//! Owned, verified LAIR produced from checked source modules.

use std::collections::BTreeMap;

use lab_language::CheckedModule;
use pliron::builtin::op_interfaces::SingleBlockRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::identifier::Identifier;
use pliron::op::Op;
use pliron::operation::verify_operation;
use pliron::pass::{Analysis, AnalysisManager};
use pliron::printable::Printable;
use thiserror::Error;

use crate::lair::dialect::attributes::quantity_dict;
use crate::lair::dialect::design::{DesignPlasmidOp, DesignStrainOp};
use crate::lair::dialect::workflow::{
    DiluteOp, PlateOp, ProvisionOp, RealizeOp, RecoverOp, TransformOp,
};
use crate::lair::source_lowering::{
    BuildArtifactIntent, SourceLoweringError, WorkflowActionIntent, lower_build_intent,
};
use crate::lair::stage::{IrStage, detect_stage};

#[derive(Debug, Error)]
pub enum PortableLairError {
    #[error(transparent)]
    Source(#[from] SourceLoweringError),
    #[error("generated LAIR failed verification: {0}")]
    Verification(String),
    #[error("generated LAIR does not satisfy the Design/Workflow stage contract: {0}")]
    Stage(String),
}

#[derive(Debug, Error)]
pub enum ProtocolLairError {
    #[error("Workflow-to-Protocol dialect conversion failed: {0}")]
    Conversion(String),
    #[error("generated Protocol LAIR failed verification: {0}")]
    Verification(String),
    #[error("generated Protocol LAIR failed material-linearity analysis: {0}")]
    MaterialLinearity(String),
    #[error("generated LAIR does not satisfy the target-selected Protocol contract: {0}")]
    Stage(String),
}

/// A Pliron context and its root module, owned together for the complete
/// lifetime of all IR handles.
pub struct PortableLairProgram {
    context: Context,
    module: ModuleOp,
}

impl PortableLairProgram {
    /// Lower one checked module into verified Design and Workflow LAIR.
    pub fn lower(module: &CheckedModule) -> Result<Self, PortableLairError> {
        Self::lower_program(&[module])
    }

    /// Lower checked, backend-neutral frontend IR into verified Design and
    /// Workflow LAIR. Protocol selection consumes this type; neither selection
    /// nor a robot backend can accept checked modules directly.
    ///
    /// The modules form one program. An artifact declared in one module may be
    /// realized by a workflow in another, so a package can separate designs,
    /// policies, and workflows into their own modules. The caller supplies the
    /// modules in its own deterministic compilation order.
    pub fn lower_program(modules: &[&CheckedModule]) -> Result<Self, PortableLairError> {
        let artifacts = lower_build_intent(modules)?;
        let mut context = Context::new();
        let root = ModuleOp::new(
            &mut context,
            Identifier::try_from("lab_build").expect("static module name is valid"),
        );
        let mut designs = BTreeMap::new();
        for artifact in &artifacts {
            let design = match artifact {
                BuildArtifactIntent::Plasmid(intent) => {
                    let operation = DesignPlasmidOp::new(
                        &mut context,
                        intent.name.clone(),
                        intent.sequence.clone(),
                        1,
                        true,
                        None,
                        None,
                    );
                    let design = operation.get_result_design(&context);
                    root.append_operation(&mut context, operation.get_operation(), 0);
                    design
                }
                BuildArtifactIntent::Strain(intent) => {
                    let operation = DesignStrainOp::new(
                        &mut context,
                        intent.name.clone(),
                        intent.chassis.clone(),
                        intent.plasmids.clone(),
                        intent.selection.clone(),
                    );
                    let design = operation.get_result_design(&context);
                    root.append_operation(&mut context, operation.get_operation(), 0);
                    design
                }
            };
            designs.insert(artifact.name().to_owned(), design);
        }
        for artifact in artifacts {
            append_workflow(&mut context, root, &designs, artifact)?;
        }
        verify_operation(root.get_operation(), &context)
            .map_err(|error| PortableLairError::Verification(error.disp(&context).to_string()))?;
        let stage = detect_stage(&context, root).map_err(PortableLairError::Stage)?;
        if stage != IrStage::DesignWorkflow {
            return Err(PortableLairError::Stage(format!(
                "expected design-workflow, found {stage}"
            )));
        }
        Ok(Self {
            context,
            module: root,
        })
    }

    pub fn ir(&self) -> String {
        self.module.get_operation().disp(&self.context).to_string()
    }

    /// Consume target-neutral Workflow LAIR and select the supported concrete
    /// plasmid-build Protocol. No backend planning occurs at this boundary.
    pub fn select_protocol(mut self) -> Result<ProtocolLairProgram, ProtocolLairError> {
        crate::lair::protocol_selection::select_plasmid_build_protocol(
            &mut self.context,
            self.module.get_operation(),
        )
        .map_err(|error| ProtocolLairError::Conversion(error.disp(&self.context).to_string()))?;
        verify_operation(self.module.get_operation(), &self.context).map_err(|error| {
            ProtocolLairError::Verification(error.disp(&self.context).to_string())
        })?;
        crate::lair::analysis::MaterialLinearityAnalysis::compute(
            self.module.get_operation(),
            &self.context,
            &mut AnalysisManager::default(),
        )
        .map_err(|error| {
            ProtocolLairError::MaterialLinearity(error.disp(&self.context).to_string())
        })?;
        let stage = detect_stage(&self.context, self.module).map_err(ProtocolLairError::Stage)?;
        if stage != IrStage::TargetSelectedProtocol {
            return Err(ProtocolLairError::Stage(format!(
                "expected target-selected-protocol, found {stage}"
            )));
        }
        Ok(ProtocolLairProgram {
            context: self.context,
            module: self.module,
        })
    }
}

/// Owned, verifier-valid Protocol LAIR. Robot planners consume this boundary
/// directly; it cannot be constructed from unchecked source or target IR.
pub struct ProtocolLairProgram {
    context: Context,
    module: ModuleOp,
}

impl ProtocolLairProgram {
    pub fn ir(&self) -> String {
        self.module.get_operation().disp(&self.context).to_string()
    }

    pub(crate) fn context(&self) -> &Context {
        &self.context
    }

    pub(crate) fn module(&self) -> ModuleOp {
        self.module
    }
}

fn append_workflow(
    context: &mut Context,
    root: ModuleOp,
    designs: &BTreeMap<String, pliron::value::Value>,
    artifact: BuildArtifactIntent,
) -> Result<(), PortableLairError> {
    let name = artifact.name().to_owned();
    let design = designs[&name];
    let dependencies = artifact.dependencies().to_vec();
    let mut values = BTreeMap::new();

    for action in artifact.actions() {
        match action {
            WorkflowActionIntent::Realize { product } => {
                let BuildArtifactIntent::Plasmid(intent) = &artifact else {
                    return Err(unsupported_realization(&name, "realize", "plasmid"));
                };
                let operation = RealizeOp::new(
                    context,
                    design,
                    name.clone(),
                    intent.recipe.backbone.clone(),
                    intent.recipe.components.clone(),
                    dependencies.clone(),
                    intent.recipe.restriction_enzyme.clone(),
                    intent.recipe.assembly_replicates,
                    assembly_chemistry(&intent.recipe.chemistry, context),
                );
                values.insert(product.clone(), operation.get_result_product(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Provision { cells, item } => {
                let operation = ProvisionOp::competent_cells(context, item.clone());
                values.insert(cells.clone(), operation.get_result_material(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Transform {
                strain,
                culture,
                cells,
            } => {
                let BuildArtifactIntent::Strain(intent) = &artifact else {
                    return Err(unsupported_realization(&name, "transform", "strain"));
                };
                let operation = TransformOp::new(
                    context,
                    design,
                    workflow_value(&values, cells, &name)?,
                    name.clone(),
                    intent.chassis.clone(),
                    intent.plasmids.clone(),
                    dependencies.clone(),
                    intent.transformation_replicates,
                    strain_chemistry(&intent.chemistry, context),
                );
                values.insert(strain.clone(), operation.get_result_strain(context));
                values.insert(culture.clone(), operation.get_result_culture(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Recover {
                culture,
                input,
                duration_magnitude,
                duration_unit,
            } => {
                let operation = RecoverOp::new(
                    context,
                    workflow_value(&values, input, &name)?,
                    duration_magnitude.clone(),
                    duration_unit.clone(),
                );
                values.insert(culture.clone(), operation.get_result_recovered(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Dilute { culture, input } => {
                let BuildArtifactIntent::Strain(intent) = &artifact else {
                    return Err(unsupported_realization(&name, "dilute", "strain"));
                };
                let operation = DiluteOp::new(
                    context,
                    workflow_value(&values, input, &name)?,
                    intent.serial_dilutions,
                );
                values.insert(culture.clone(), operation.get_result_diluted(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Plate {
                plate,
                culture,
                selection,
            } => {
                let BuildArtifactIntent::Strain(intent) = &artifact else {
                    return Err(unsupported_realization(&name, "plate", "strain"));
                };
                let operation = PlateOp::new(
                    context,
                    workflow_value(&values, culture, &name)?,
                    selection.clone(),
                    intent.plating_replicates,
                );
                values.insert(plate.clone(), operation.get_result_plate(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
        }
    }
    Ok(())
}

fn assembly_chemistry(
    chemistry: &crate::lair::source_lowering::AssemblyChemistryIntent,
    context: &Context,
) -> pliron::builtin::attributes::DictAttr {
    quantity_dict(
        &[
            ("reaction_volume_ul", chemistry.reaction_volume_ul.into()),
            ("part_volume_ul", chemistry.part_volume_ul.into()),
            ("enzyme_volume_ul", chemistry.enzyme_volume_ul.into()),
            ("ligase_volume_ul", chemistry.ligase_volume_ul.into()),
            ("buffer_volume_ul", chemistry.buffer_volume_ul.into()),
            ("cycles", chemistry.cycles.into()),
            (
                "digest_temperature_c",
                chemistry.digest_temperature_c.into(),
            ),
            ("digest_minutes", chemistry.digest_minutes.into()),
            (
                "ligate_temperature_c",
                chemistry.ligate_temperature_c.into(),
            ),
            ("ligate_minutes", chemistry.ligate_minutes.into()),
        ],
        context,
    )
}

fn strain_chemistry(
    chemistry: &crate::lair::source_lowering::StrainChemistryIntent,
    context: &Context,
) -> pliron::builtin::attributes::DictAttr {
    quantity_dict(
        &[
            ("cell_volume_ul", chemistry.cell_volume_ul.into()),
            ("dna_volume_ul", chemistry.dna_volume_ul.into()),
            ("recovery_volume_ul", chemistry.recovery_volume_ul.into()),
            ("cold_minutes", chemistry.cold_minutes.into()),
            (
                "heat_shock_temperature_c",
                chemistry.heat_shock_temperature_c.into(),
            ),
            ("heat_shock_minutes", chemistry.heat_shock_minutes.into()),
            (
                "recovery_temperature_c",
                chemistry.recovery_temperature_c.into(),
            ),
            ("recovery_minutes", chemistry.recovery_minutes.into()),
            ("medium_volume_ul", chemistry.medium_volume_ul.into()),
            ("culture_volume_ul", chemistry.culture_volume_ul.into()),
            ("colony_volume_ul", chemistry.colony_volume_ul.into()),
        ],
        context,
    )
}

fn unsupported_realization(artifact: &str, operation: &str, expected: &str) -> PortableLairError {
    PortableLairError::Stage(format!(
        "workflow for artifact '{artifact}' uses '{operation}', which realizes a {expected}"
    ))
}

fn workflow_value(
    values: &BTreeMap<String, pliron::value::Value>,
    name: &str,
    artifact: &str,
) -> Result<pliron::value::Value, PortableLairError> {
    values.get(name).copied().ok_or_else(|| {
        PortableLairError::Stage(format!(
            "workflow for artifact '{artifact}' uses material '{name}' before it is defined"
        ))
    })
}

#[cfg(test)]
mod tests {
    use lab_language::{
        ModuleId, SemanticEnvironment, compile_module, compile_module_in_environment,
    };

    use super::PortableLairProgram;

    const DESIGNS: &str = r#"use std.bio.designs
use std.bio.golden_gate

buy part J23101
buy part B0034
buy part GFP
buy part B0015
buy backbone pSB1C3
buy restriction_enzyme BsaI
buy chassis DH5alpha
buy antibiotic chloramphenicol

plasmid p_gfp:
  sequence = dna("ACGT")
  backbone = pSB1C3
  components = [J23101, B0034, GFP, B0015]
  restriction_enzyme = BsaI
  assembly_replicates = 1
  require topology == circular
  accept sequence == design.sequence

strain reporter_host:
  chassis = DH5alpha
  plasmids = [p_gfp]
  selection = chloramphenicol
  transformation_replicates = 2
  plating_replicates = 2
  serial_dilutions = 2
"#;

    const WORKFLOWS: &str = r#"
use std.bio.build
use std.bio.designs
use std.bio.golden_gate
use std.lab.plasmid
use demo.designs

workflow assemble_p_gfp() -> Material<Plasmid>:
  dependencies = []
  product <- realize p_gfp from dependencies
  return product

workflow build_reporter_host(
  p_gfp: Material<Plasmid>,
) -> (
  strain: Material<Strain>,
  plate: Material<Plate>,
):
  dependencies = [p_gfp]
  cells <- provision DH5alpha
  strain, culture <- transform reporter_host from dependencies into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return strain, plate
"#;

    #[test]
    fn lowers_an_artifact_and_its_workflow_from_separate_modules() {
        let designs = compile_module_in_environment(
            ModuleId::new("demo.designs"),
            DESIGNS,
            &SemanticEnvironment::default(),
        )
        .expect("design module checks");
        let mut environment = SemanticEnvironment::default();
        environment.insert("demo.designs", designs.interface.clone());
        let workflows =
            compile_module_in_environment(ModuleId::new("demo.workflows"), WORKFLOWS, &environment)
                .expect("workflow module checks");

        let program =
            PortableLairProgram::lower_program(&[&designs, &workflows]).expect("program lowers");
        let split = program.ir();

        let combined = compile_module(
            &format!("{DESIGNS}{WORKFLOWS}")
                .replace("use demo.designs\n", "")
                // Concatenating two modules would import the kinds twice.
                .replace("use std.bio.designs\nuse std.bio.golden_gate\n", "")
                .replacen(
                    "use std.bio.build",
                    "use std.bio.designs\nuse std.bio.golden_gate\nuse std.bio.build",
                    1,
                ),
        )
        .expect("single module checks");
        let single = PortableLairProgram::lower(&combined)
            .expect("single module lowers")
            .ir();

        assert_eq!(split, single);
    }

    #[test]
    fn a_module_without_a_realizing_workflow_is_not_a_program_on_its_own() {
        let designs = compile_module_in_environment(
            ModuleId::new("demo.designs"),
            DESIGNS,
            &SemanticEnvironment::default(),
        )
        .expect("design module checks");
        let error = PortableLairProgram::lower_program(&[&designs])
            .err()
            .expect("an artifact with no realization cannot lower");
        assert!(
            error.to_string().contains("std.bio.build.realize"),
            "{error}"
        );
    }
}

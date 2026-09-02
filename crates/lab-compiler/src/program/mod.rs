//! Owned, verified LAIR produced from checked source modules.

mod lowering;

use std::collections::BTreeMap;

use crate::method::MethodRegistry;
use lab_language::CheckedModule;
use pliron::builtin::op_interfaces::SingleBlockRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::combine::{Parser, eof};
use pliron::context::Context;
use pliron::identifier::Identifier;
use pliron::irfmt::parsers::spaced;
use pliron::op::Op;
use pliron::operation::{Operation, verify_operation};
use pliron::parsable::parse_from_str;
use pliron::pass::{Analysis, AnalysisManager};
use pliron::printable::Printable;
use sha2::{Digest, Sha256};
use thiserror::Error;

use self::lowering::{BuildArtifactIntent, WorkflowActionIntent, lower_build_intent};
use crate::design::ir::{DesignDnaSequenceOp, DesignPlasmidOp, DesignStrainOp};
use crate::ir::attributes::quantity_dict;
use crate::stage::{IrStage, detect_stage, initialize_stage, set_stage};
use crate::workflow::ir::{DiluteOp, PlateOp, ProvisionOp, RealizeOp, RecoverOp, TransformOp};

pub use self::lowering::SourceLoweringError;
use crate::planning::PlanningProblemExtractionError;

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
pub enum RefinedLairError {
    #[error("Intent-to-Method refinement failed: {0}")]
    Conversion(String),
    #[error("generated refined-alternatives LAIR failed verification: {0}")]
    Verification(String),
    #[error("generated LAIR does not satisfy the refined-alternatives contract: {0}")]
    Stage(String),
}

#[derive(Debug, Error)]
pub enum AllocatedLairError {
    #[error("failed to parse allocated LAIR: {0}")]
    Parse(String),
    #[error("expected a builtin.module root operation, found '{0}'")]
    ExpectedModule(String),
    #[error(transparent)]
    Problem(#[from] PlanningProblemExtractionError),
    #[error(transparent)]
    Application(#[from] crate::allocation::AllocationApplicationError),
    #[error("generated allocated LAIR failed verification: {0}")]
    Verification(String),
    #[error("generated allocated LAIR failed material-linearity analysis: {0}")]
    MaterialLinearity(String),
    #[error("generated LAIR does not satisfy the allocated-procedure contract: {0}")]
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
    /// Workflow LAIR. Method refinement consumes this type; facility planning
    /// and adapters cannot accept checked modules directly.
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
        initialize_stage(&mut context, root, IrStage::DesignIntent);
        let mut sequences = BTreeMap::new();
        for artifact in &artifacts {
            let BuildArtifactIntent::Plasmid(intent) = artifact else {
                continue;
            };
            if sequences.contains_key(&intent.sequence.key) {
                continue;
            }
            let operation = DesignDnaSequenceOp::new(
                &mut context,
                intent.sequence.name.clone(),
                intent.sequence.elements.clone(),
            );
            let sequence = operation.get_result_sequence(&context);
            root.append_operation(&mut context, operation.get_operation(), 0);
            sequences.insert(intent.sequence.key.clone(), sequence);
        }
        let mut designs = BTreeMap::new();
        for artifact in &artifacts {
            let design = match artifact {
                BuildArtifactIntent::Plasmid(intent) => {
                    let sequence = sequences
                        .get(&intent.sequence.key)
                        .copied()
                        .expect("every plasmid sequence was lowered before its design");
                    let operation = DesignPlasmidOp::new(
                        &mut context,
                        intent.name.clone(),
                        sequence,
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
        if stage != IrStage::DesignIntent {
            return Err(PortableLairError::Stage(format!(
                "expected design-intent, found {stage}"
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

    /// Enumerate every applicable portable method without selecting a facility or candidate.
    pub fn refine_methods(
        mut self,
        registry: &MethodRegistry,
    ) -> Result<RefinedLairProgram, RefinedLairError> {
        crate::method::refinement::refine_method_alternatives(
            &mut self.context,
            self.module.get_operation(),
            registry,
        )
        .map_err(|error| RefinedLairError::Conversion(error.disp(&self.context).to_string()))?;
        set_stage(&mut self.context, self.module, IrStage::RefinedAlternatives)
            .map_err(RefinedLairError::Stage)?;
        verify_operation(self.module.get_operation(), &self.context).map_err(|error| {
            RefinedLairError::Verification(error.disp(&self.context).to_string())
        })?;
        let stage = detect_stage(&self.context, self.module).map_err(RefinedLairError::Stage)?;
        if stage != IrStage::RefinedAlternatives {
            return Err(RefinedLairError::Stage(format!(
                "expected refined-alternatives, found {stage}"
            )));
        }
        Ok(RefinedLairProgram {
            context: self.context,
            module: self.module,
        })
    }

    /// Refine with the validated methods bundled into this compiler build.
    pub fn refine_standard_methods(self) -> Result<RefinedLairProgram, RefinedLairError> {
        self.refine_methods(crate::method::standard_method_registry())
    }
}

/// Owned, verifier-valid Method alternatives with no facility allocation or selected candidate.
pub struct RefinedLairProgram {
    context: Context,
    module: ModuleOp,
}

impl RefinedLairProgram {
    pub fn ir(&self) -> String {
        self.module.get_operation().disp(&self.context).to_string()
    }

    /// Project immutable, facility-independent constraints for the global planner.
    pub fn planning_problem(
        &self,
    ) -> Result<crate::planning::PlanningProblem, PlanningProblemExtractionError> {
        crate::planning::extract_planning_problem(&self.context, self.module)
    }

    /// Apply one complete solution to this exact refined module and eliminate every alternative.
    pub fn allocate(
        mut self,
        solution: &crate::planning::FacilityPlanningSolution,
    ) -> Result<AllocatedLairProgram, AllocatedLairError> {
        let problem = self.planning_problem()?;
        crate::allocation::apply_facility_solution(
            &mut self.context,
            self.module,
            &problem,
            solution,
        )?;
        set_stage(&mut self.context, self.module, IrStage::AllocatedProcedure)
            .map_err(AllocatedLairError::Stage)?;
        verify_allocated_program(&self.context, self.module)?;
        let source = self.module.get_operation().disp(&self.context).to_string();
        Ok(AllocatedLairProgram {
            context: self.context,
            module: self.module,
            source,
        })
    }
}

/// Owned, verifier-valid Procedure LAIR with all method and facility decisions frozen.
pub struct AllocatedLairProgram {
    context: Context,
    module: ModuleOp,
    source: String,
}

impl AllocatedLairProgram {
    /// Parse and verify a complete textual Allocated LAIR program.
    ///
    /// The resulting program has no dependency on the planning problem or solution that
    /// originally produced the text; every backend-facing semantic fact is reconstructed from
    /// the allocated IR itself.
    pub fn parse_ir(source: &str) -> Result<Self, AllocatedLairError> {
        let mut context = Context::new();
        let root = parse_from_str(
            spaced(Operation::top_level_parser()).skip(eof()),
            &mut context,
            source,
        )
        .map_err(|error| AllocatedLairError::Parse(error.disp(&context).to_string()))?;
        let module = Operation::get_op::<ModuleOp>(root, &context).ok_or_else(|| {
            AllocatedLairError::ExpectedModule(Operation::get_opid(root, &context).to_string())
        })?;
        verify_allocated_program(&context, module)?;
        Ok(Self {
            context,
            module,
            source: source.to_owned(),
        })
    }

    pub fn ir(&self) -> String {
        self.source.clone()
    }

    /// Digest the exact verified textual artifact retained by this program.
    pub fn sha256(&self) -> String {
        hex_sha256(self.source.as_bytes())
    }

    /// Reconstruct the complete facility-bound semantic aggregate from Allocated LAIR.
    pub fn allocated_program(
        &self,
    ) -> Result<
        crate::allocation::AllocatedProgram,
        crate::allocation::AllocatedProgramExtractionError,
    > {
        crate::allocation::extract_allocated_program(&self.context, self.module)
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn verify_allocated_program(context: &Context, module: ModuleOp) -> Result<(), AllocatedLairError> {
    verify_operation(module.get_operation(), context)
        .map_err(|error| AllocatedLairError::Verification(error.disp(context).to_string()))?;
    crate::procedure::analysis::MaterialLinearityAnalysis::compute(
        module.get_operation(),
        context,
        &mut AnalysisManager::default(),
    )
    .map_err(|error| AllocatedLairError::MaterialLinearity(error.disp(context).to_string()))?;
    let stage = detect_stage(context, module).map_err(AllocatedLairError::Stage)?;
    if stage != IrStage::AllocatedProcedure {
        return Err(AllocatedLairError::Stage(format!(
            "expected allocated-procedure, found {stage}"
        )));
    }
    Ok(())
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
                let operation = if let Some(recipe) = &intent.recipe {
                    RealizeOp::golden_gate(
                        context,
                        design,
                        name.clone(),
                        recipe.backbone.clone(),
                        recipe.components.clone(),
                        dependencies.clone(),
                        recipe.restriction_enzyme.clone(),
                        recipe.assembly_replicates,
                        assembly_chemistry(&recipe.chemistry, context),
                    )
                } else {
                    RealizeOp::new(context, design, name.clone(), dependencies.clone())
                };
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
                let BuildArtifactIntent::Strain(intent) = &artifact else {
                    return Err(unsupported_realization(&name, "recover", "strain"));
                };
                let transformed_volume = transformed_volume_ul(intent)?;
                let operation = RecoverOp::new(
                    context,
                    workflow_value(&values, input, &name)?,
                    name.clone(),
                    duration_magnitude.clone(),
                    duration_unit.clone(),
                    intent.transformation_replicates,
                    transformed_volume,
                    intent.chemistry.recovery_volume_ul,
                    intent.chemistry.recovery_temperature_c,
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
                    name.clone(),
                    intent.serial_dilutions,
                    intent.transformation_replicates,
                    recovered_volume_ul(intent)?,
                    intent.chemistry.medium_volume_ul,
                    intent.chemistry.culture_volume_ul,
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
                    name.clone(),
                    selection.clone(),
                    intent.plating_replicates,
                    intent.transformation_replicates,
                    intent.serial_dilutions,
                    intent.chemistry.medium_volume_ul,
                    intent.chemistry.culture_volume_ul,
                    intent.chemistry.colony_volume_ul,
                );
                values.insert(plate.clone(), operation.get_result_plate(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
        }
    }
    Ok(())
}

fn transformed_volume_ul(
    intent: &lowering::StrainArtifactIntent,
) -> Result<u32, PortableLairError> {
    let dna_count = u32::try_from(intent.plasmids.len()).map_err(|_| {
        PortableLairError::Stage(format!(
            "strain '{}' has too many plasmids to represent its transformation volume",
            intent.name
        ))
    })?;
    u32::from(intent.chemistry.dna_volume_ul)
        .checked_mul(dna_count)
        .and_then(|dna| dna.checked_add(u32::from(intent.chemistry.cell_volume_ul)))
        .ok_or_else(|| {
            PortableLairError::Stage(format!(
                "strain '{}' transformation volume overflows",
                intent.name
            ))
        })
}

fn recovered_volume_ul(intent: &lowering::StrainArtifactIntent) -> Result<u32, PortableLairError> {
    transformed_volume_ul(intent)?
        .checked_add(u32::from(intent.chemistry.recovery_volume_ul))
        .ok_or_else(|| {
            PortableLairError::Stage(format!(
                "strain '{}' recovery volume overflows",
                intent.name
            ))
        })
}

fn assembly_chemistry(
    chemistry: &lowering::AssemblyChemistryIntent,
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
            ("lid_temperature_c", chemistry.lid_temperature_c.into()),
            (
                "final_digest_temperature_c",
                chemistry.final_digest_temperature_c.into(),
            ),
            (
                "final_digest_minutes",
                chemistry.final_digest_minutes.into(),
            ),
            (
                "heat_inactivation_temperature_c",
                chemistry.heat_inactivation_temperature_c.into(),
            ),
            (
                "heat_inactivation_minutes",
                chemistry.heat_inactivation_minutes.into(),
            ),
            ("hold_temperature_c", chemistry.hold_temperature_c.into()),
        ],
        context,
    )
}

fn strain_chemistry(
    chemistry: &lowering::StrainChemistryIntent,
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
    use crate::method::{ProcedureValue, ScalarType};
    use crate::procedure::{
        AspirationStrategy, DispenseStrategy, PipettingStep, ValidatedProcedureProgram, vocabulary,
    };
    use lab_capability::ScalarValue;
    use lab_language::{
        ModuleId, SemanticEnvironment, compile_module, compile_module_in_environment,
    };

    use crate::planning::{PlanningProblem, PlanningValueSource};
    use crate::session::CompilerSession;
    use crate::stage::IrStage;

    use super::PortableLairProgram;

    const DESIGNS: &str = r#"use std.bio.designs
use std.bio.golden_gate

buy part J23101:
  sbol_identity = "https://sbolcanvas.org/J23101"
buy part B0034:
  sbol_identity = "https://sbolcanvas.org/B0034"
buy part GFP:
  sbol_identity = "https://sbolcanvas.org/GFP"
buy part B0015:
  sbol_identity = "https://sbolcanvas.org/B0015"
buy backbone pSB1C3:
  sbol_identity = "https://sbolcanvas.org/pSB1C3"
buy restriction_enzyme BsaI:
  sbol_identity = "https://SBOL2Build.org/BsaI"
buy chassis DH5alpha:
  sbol_identity = "https://sbolcanvas.org/DH5alpha"
buy antibiotic chloramphenicol:
  sbol_identity = "https://example.org/golden-gate/materials/chloramphenicol"
buy part T4_DNA_ligase:
  sbol_identity = "https://example.org/golden-gate/materials/T4_DNA_ligase"
buy part T4_DNA_ligase_buffer:
  sbol_identity = "https://example.org/golden-gate/materials/T4_DNA_ligase_buffer"
buy part nuclease_free_water:
  sbol_identity = "https://example.org/golden-gate/materials/nuclease_free_water"
buy part recovery_medium:
  sbol_identity = "https://example.org/golden-gate/materials/recovery_medium"

gfp_sequence: DNA = dna("ACGT")

plasmid p_gfp:
  sequence = gfp_sequence
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

    const SHARED_SEQUENCE_PROGRAM: &str = r#"use std.bio.build
use std.bio.designs
use std.bio.golden_gate

buy part insert
buy backbone pSB1C3
buy restriction_enzyme BsaI

shared_sequence: DNA = dna("ACGT")

plasmid first:
  sequence = shared_sequence
  backbone = pSB1C3
  components = [insert]
  restriction_enzyme = BsaI
  assembly_replicates = 1

plasmid second:
  sequence = shared_sequence
  backbone = pSB1C3
  components = [insert]
  restriction_enzyme = BsaI
  assembly_replicates = 1

workflow build_first() -> Material<Plasmid>:
  dependencies = []
  product <- realize first from dependencies
  return product

workflow build_second() -> Material<Plasmid>:
  dependencies = []
  product <- realize second from dependencies
  return product
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

        assert_eq!(split.matches(" = design.dna_sequence ").count(), 1);
        assert!(split.contains("sequence_name: builtin.string \"gfp_sequence\""));
        assert!(split.contains("elements: builtin.string \"ACGT\""));
        assert!(split.contains("<(design.dna_sequence ) -> (design.artifact )>"));
        assert!(!split.contains("design.plasmid ()"));

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

    #[test]
    fn several_designs_share_one_named_sequence_value() {
        let checked = compile_module(SHARED_SEQUENCE_PROGRAM).expect("shared sequence checks");
        let ir = PortableLairProgram::lower(&checked)
            .expect("shared sequence lowers")
            .ir();

        assert_eq!(ir.matches(" = design.dna_sequence ").count(), 1);
        assert_eq!(ir.matches(" = design.plasmid ").count(), 2);
        assert_eq!(
            ir.matches("sequence_name: builtin.string \"shared_sequence\"")
                .count(),
            1
        );
    }

    #[test]
    fn sequence_defined_plasmids_only_offer_applicable_realization_methods() {
        let checked = compile_module(
            r#"use std.bio.build
use std.bio.designs

plasmid starter:
  sequence = dna("ATGC")
  require topology == circular
  accept sequence == design.sequence

workflow main() -> Material<Plasmid>:
  product <- realize starter
  return product
"#,
        )
        .expect("generic realization checks");
        let portable = PortableLairProgram::lower(&checked).expect("generic realization lowers");
        let portable_ir = portable.ir();
        assert!(portable_ir.contains("workflow.realize"), "{portable_ir}");
        assert!(
            !portable_ir.contains("realize_restriction_enzyme"),
            "{portable_ir}"
        );

        let refined = portable
            .refine_standard_methods()
            .expect("an applicable manual realization method exists");
        let problem = refined.planning_problem().expect("problem projects");
        let realization = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.bio.build.realize")
            .expect("realization choice exists");
        assert_eq!(realization.candidates.len(), 1);
        assert_eq!(
            realization.candidates[0].method.as_str(),
            "https://www.lab-compiler.org/ns/method#manual-artifact-realization"
        );
    }

    #[test]
    fn standard_methods_replace_every_workflow_op_with_verified_alternatives() {
        let checked = compile_module(
            &format!("{DESIGNS}{WORKFLOWS}")
                .replace("use demo.designs\n", "")
                .replace("use std.bio.designs\nuse std.bio.golden_gate\n", "")
                .replacen(
                    "use std.bio.build",
                    "use std.bio.designs\nuse std.bio.golden_gate\nuse std.bio.build",
                    1,
                ),
        )
        .expect("program checks");
        let refined = PortableLairProgram::lower(&checked)
            .expect("portable LAIR lowers")
            .refine_standard_methods()
            .expect("standard methods refine");
        let ir = refined.ir();

        assert!(ir.contains("lair.stage") && ir.contains("refined-alternatives"));
        assert!(!ir.contains("workflow."), "{ir}");
        assert!(ir.contains("https://www.lab-compiler.org/ns/method#automated-golden-gate"));
        assert!(ir.contains("https://www.lab-compiler.org/ns/method#manual-artifact-realization"));
        assert!(ir.contains("procedure.parameter"));
        assert!(
            ir.contains("normalized_program: procedure.program <"),
            "{ir}"
        );
        assert!(ir.contains("capability.requirement"));
        assert!(ir.contains("capability.constraint"));
        assert!(ir.contains("http://qudt.org/vocab/unit/HR"));

        let mut session = CompilerSession::default();
        session.parse_ir(&ir).unwrap();
        session.verify_stage(IrStage::RefinedAlternatives).unwrap();
    }

    #[test]
    fn refinement_fails_closed_when_the_registry_has_no_method() {
        let checked = compile_module(SHARED_SEQUENCE_PROGRAM).expect("program checks");
        let error = PortableLairProgram::lower(&checked)
            .expect("portable LAIR lowers")
            .refine_methods(&crate::method::MethodRegistry::default())
            .err()
            .expect("an empty method registry cannot refine reachable Intent");

        assert!(
            error.to_string().contains("no method definition"),
            "{error}"
        );
    }

    #[test]
    fn method_refinement_preserves_the_source_quantity_unit() {
        let source = format!("{DESIGNS}{WORKFLOWS}")
            .replace("use demo.designs\n", "")
            .replace("use std.bio.designs\nuse std.bio.golden_gate\n", "")
            .replacen(
                "use std.bio.build",
                "use std.bio.designs\nuse std.bio.golden_gate\nuse std.bio.build",
                1,
            )
            .replace("recover culture for 1 h", "recover culture for 30 min");
        let checked = compile_module(&source).expect("minute-scale recovery checks");
        let ir = PortableLairProgram::lower(&checked)
            .expect("portable LAIR lowers")
            .refine_standard_methods()
            .expect("standard methods refine")
            .ir();

        assert!(ir.contains("http://qudt.org/vocab/unit/MIN"), "{ir}");
        assert!(ir.contains("builtin.string \"30\""), "{ir}");
        assert!(!ir.contains("http://qudt.org/vocab/unit/HR"), "{ir}");
    }

    #[test]
    fn refined_lair_projects_a_stable_facility_independent_planning_problem() {
        let source = format!("{DESIGNS}{WORKFLOWS}")
            .replace("use demo.designs\n", "")
            .replace("use std.bio.designs\nuse std.bio.golden_gate\n", "")
            .replacen(
                "use std.bio.build",
                "use std.bio.designs\nuse std.bio.golden_gate\nuse std.bio.build",
                1,
            )
            .replace("recover culture for 1 h", "recover culture for 30 min");
        let checked = compile_module(&source).expect("minute-scale recovery checks");
        let refined = PortableLairProgram::lower(&checked)
            .expect("portable LAIR lowers")
            .refine_standard_methods()
            .expect("standard methods refine");
        let problem = refined.planning_problem().expect("problem projects");

        let realization = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.bio.build.realize")
            .expect("realization is a global method choice");
        assert_eq!(realization.inputs[0].name.as_str(), "design");
        assert_eq!(realization.outputs[0].name.as_str(), "product");
        assert_eq!(realization.candidates.len(), 3);
        assert!(realization.candidates.iter().any(|candidate| {
            candidate.method.as_str()
                == "https://www.lab-compiler.org/ns/method#manual-artifact-realization"
        }));
        let automated = realization
            .candidates
            .iter()
            .find(|candidate| {
                candidate
                    .method
                    .as_str()
                    .ends_with("#automated-golden-gate")
            })
            .expect("automated Golden Gate remains selectable");
        assert_eq!(automated.tasks.len(), 2);
        let program = automated.tasks[0]
            .program
            .as_ref()
            .expect("Golden Gate setup is normalized before facility planning")
            .validate()
            .expect("normalized program validates");
        let ValidatedProcedureProgram::PipettingV1(program) = program else {
            panic!("Golden Gate setup must normalize to the pipetting contract")
        };
        assert_eq!(program.as_program().materials.len(), 9);
        assert_eq!(program.as_program().steps.len(), 10);
        let capabilities = program
            .capability_formula()
            .all_of
            .into_iter()
            .map(|clause| clause.capability_kind)
            .collect::<Vec<_>>();
        assert!(
            capabilities
                .iter()
                .any(|kind| kind.as_str() == vocabulary::METERED_LIQUID_TRANSFER)
        );
        assert!(
            capabilities
                .iter()
                .any(|kind| kind.as_str() == vocabulary::IN_WELL_MIXING)
        );
        let setup_parameters = &automated.tasks[0].parameters;
        let artifact = setup_parameters
            .iter()
            .find(|parameter| parameter.id.as_str().ends_with("::parameter::artifact"))
            .expect("selected Procedure carries its artifact identity");
        assert!(matches!(
            &artifact.value,
            ProcedureValue::Scalar { value }
                if matches!(&value.value, ScalarValue::Text(value) if value == "p_gfp")
        ));
        let components = setup_parameters
            .iter()
            .find(|parameter| parameter.id.as_str().ends_with("::parameter::components"))
            .expect("selected Procedure carries its ordered components");
        assert!(matches!(
            &components.value,
            ProcedureValue::List { element_type: ScalarType::Text, values }
                if values.len() == 4
                    && matches!(&values[0].value, ScalarValue::Text(value) if value == "J23101")
        ));
        let dependencies = setup_parameters
            .iter()
            .find(|parameter| parameter.id.as_str().ends_with("::parameter::dependencies"))
            .expect("selected Procedure carries its dependency list");
        assert!(matches!(
            &dependencies.value,
            ProcedureValue::List { element_type: ScalarType::Text, values } if values.is_empty()
        ));
        assert_eq!(automated.tasks[0].materials.len(), 9);
        assert!(automated.tasks[0].materials.iter().all(|material| {
            matches!(
                material.source,
                crate::planning::PlanningMaterialSource::Inventory
            )
        }));
        assert!(matches!(
            automated.tasks[1].inputs[0].source,
            PlanningValueSource::TaskOutput { ref task, ref output }
                if task.as_str().ends_with("::setup-reaction") && output.as_str() == "reaction"
        ));
        let thermal = automated.tasks[1]
            .program
            .as_ref()
            .expect("Golden Gate cycling is normalized before facility planning")
            .validate()
            .expect("normalized thermal program validates");
        let ValidatedProcedureProgram::ThermalV1(thermal) = thermal else {
            panic!("Golden Gate cycling must normalize to the thermal contract")
        };
        let thermal = thermal.as_program();
        assert_eq!(thermal.load.input, 0);
        assert_eq!(thermal.load.outputs.len(), 1);
        assert_eq!(thermal.load.outputs[0].as_str(), "product");
        assert_eq!(thermal.load.sample_count, 1);
        assert_eq!(thermal.load.volume_each.value().to_string(), "20");
        assert_eq!(thermal.stages.len(), 2);
        assert_eq!(thermal.stages[0].repeats, 75);
        assert_eq!(thermal.stages[0].steps[0].id.as_str(), "digest");
        assert_eq!(thermal.stages[0].steps[0].hold.value().to_string(), "120");
        assert_eq!(thermal.stages[0].steps[1].id.as_str(), "ligate");
        assert_eq!(thermal.stages[0].steps[1].hold.value().to_string(), "300");
        assert_eq!(
            thermal
                .final_hold
                .as_ref()
                .expect("Golden Gate has a final hold")
                .value()
                .to_string(),
            "4"
        );
        assert_eq!(automated.tasks[1].requirements.len(), 2);
        assert_eq!(
            automated.tasks[1]
                .requirements
                .iter()
                .map(|requirement| requirement.capability_kind.as_str())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                vocabulary::HEATED_LID_TEMPERATURE_CONTROL,
                vocabulary::PROGRAMMED_BLOCK_TEMPERATURE_CONTROL,
            ])
        );

        let temperature_staged = realization
            .candidates
            .iter()
            .find(|candidate| {
                candidate
                    .method
                    .as_str()
                    .ends_with("#temperature-staged-golden-gate")
            })
            .expect("temperature-staged Golden Gate is a portable Method alternative");
        let staged_program = temperature_staged.tasks[0]
            .program
            .as_ref()
            .expect("temperature-staged setup is normalized before facility planning")
            .validate()
            .expect("temperature-staged setup validates");
        let ValidatedProcedureProgram::PipettingV1(staged_program) = staged_program else {
            panic!("temperature-staged setup must normalize to the pipetting contract")
        };
        let staged_program = staged_program.as_program();
        assert_eq!(staged_program.materials.len(), 9);
        assert_eq!(staged_program.steps.len(), 18);
        let source_temperature =
            crate::procedure::staged_temperature_envelope(&staged_program.vessels)
                .expect("the Method requires controlled source staging");
        assert_eq!(source_temperature.minimum, source_temperature.maximum);
        assert_eq!(source_temperature.minimum.value().to_string(), "4");
        assert!(
            staged_program
                .vessels
                .iter()
                .filter(|vessel| vessel.temperature.is_some())
                .count()
                > 1,
            "every staged reagent source carries the requirement, not the program as a whole"
        );
        let staged_capabilities = temperature_staged.tasks[0]
            .requirements
            .iter()
            .map(|requirement| requirement.capability_kind.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            staged_capabilities,
            std::collections::BTreeSet::from([
                vocabulary::METERED_LIQUID_TRANSFER,
                vocabulary::IN_WELL_MIXING,
                vocabulary::TEMPERATURE_CONTROLLED_STAGING,
                vocabulary::VESSEL_RELATIVE_LIQUID_ACCESS,
                vocabulary::POST_DISPENSE_BLOWOUT,
                vocabulary::TOUCH_TIP,
            ])
        );
        let PipettingStep::Mix {
            cycles: source_mix_cycles,
            volume: source_mix_volume,
            fluid_path_group: source_mix_path,
            ..
        } = &staged_program.steps[1]
        else {
            panic!("the first non-water reagent must be mixed before transfer")
        };
        let PipettingStep::Transfer {
            fluid_path_group: source_transfer_path,
            technique: source_transfer_technique,
            ..
        } = &staged_program.steps[2]
        else {
            panic!("source mixing must be followed by its transfer")
        };
        assert_eq!(*source_mix_cycles, 3);
        assert_eq!(source_mix_volume.value().to_string(), "2");
        assert_eq!(source_mix_path, source_transfer_path);
        assert!(source_transfer_technique.blow_out);
        assert!(source_transfer_technique.touch_tip);
        let PipettingStep::Transfer {
            fluid_path_group: final_transfer_path,
            ..
        } = &staged_program.steps[16]
        else {
            panic!("the final reagent must be transferred before bubble clearing")
        };
        let PipettingStep::Mix {
            cycles,
            volume,
            fluid_path_group: final_mix_path,
            technique,
            ..
        } = &staged_program.steps[17]
        else {
            panic!("the final operation must clear bubbles")
        };
        assert_eq!(*cycles, 2);
        assert_eq!(volume.value().to_string(), "20");
        assert_eq!(final_transfer_path, final_mix_path);
        assert!(technique.blow_out && technique.touch_tip);
        assert!(matches!(
            &technique.aspiration,
            AspirationStrategy::VesselBottom { offset } if offset.value().to_string() == "0"
        ));
        assert!(matches!(
            &technique.dispense,
            DispenseStrategy::VesselBottom { offset } if offset.value().to_string() == "8"
        ));
        assert_eq!(
            temperature_staged.tasks[1].program, automated.tasks[1].program,
            "preparation technique must not rewrite authored thermal intent"
        );

        let dilution = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.lab.plasmid.dilute")
            .expect("serial dilution is a global method choice");
        let dilution_task = &dilution.candidates[0].tasks[0];
        let dilution_program = dilution_task
            .program
            .as_ref()
            .expect("serial dilution is normalized before facility planning")
            .validate()
            .expect("normalized serial-dilution program validates");
        let ValidatedProcedureProgram::PipettingV1(dilution_program) = dilution_program else {
            panic!("serial dilution must normalize to the pipetting contract")
        };
        assert!(dilution_program.as_program().vessels.iter().any(|vessel| {
            matches!(
                &vessel.role,
                crate::procedure::VesselRole::ProcedureInput { input: 0 }
            )
        }));
        assert_eq!(dilution_program.as_program().steps.len(), 9);
        assert_eq!(dilution_task.requirements.len(), 3);
        assert!(dilution_task.requirements.iter().all(|requirement| {
            matches!(
                requirement.capability_kind.as_str(),
                vocabulary::METERED_LIQUID_TRANSFER
                    | vocabulary::IN_WELL_MIXING
                    | vocabulary::LIQUID_LEVEL_AWARE_ASPIRATION
            )
        }));

        let recovery = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.lab.plasmid.recover")
            .expect("recovery is a global method choice");
        assert_eq!(recovery.candidates.len(), 3);
        for candidate in recovery
            .candidates
            .iter()
            .filter(|candidate| !candidate.method.as_str().ends_with("#automated-recovery"))
        {
            let constraint = &candidate.tasks[0].requirements[0].constraints[0];
            assert_eq!(
                constraint.required.unit.as_ref().unwrap().as_str(),
                "http://qudt.org/vocab/unit/MIN"
            );
            assert!(matches!(
                &constraint.required.value,
                ScalarValue::Real(value) if value.to_string() == "30"
            ));
        }
        let automated_recovery = recovery
            .candidates
            .iter()
            .find(|candidate| candidate.method.as_str().ends_with("#automated-recovery"))
            .expect("automated recovery is a real method alternative");
        assert_eq!(automated_recovery.tasks.len(), 2);
        let add_medium = automated_recovery.tasks[0]
            .program
            .as_ref()
            .expect("recovery medium addition is normalized")
            .validate()
            .expect("recovery medium program validates");
        let ValidatedProcedureProgram::PipettingV1(add_medium) = add_medium else {
            panic!("recovery medium addition must be pipetting")
        };
        let recovered_location = crate::procedure::Location {
            vessel: crate::procedure::ProcedureLocalId::new("recovery-cultures").unwrap(),
            position: 0,
        };
        assert_eq!(
            add_medium
                .liquid_ledger()
                .final_volume(&recovered_location)
                .expect("recovered culture volume is exact")
                .to_string(),
            "82"
        );
        let incubation = automated_recovery.tasks[1]
            .program
            .as_ref()
            .expect("recovery incubation is normalized")
            .validate()
            .expect("recovery incubation program validates");
        let ValidatedProcedureProgram::ThermalV1(incubation) = incubation else {
            panic!("recovery incubation must be thermal")
        };
        assert_eq!(
            incubation.as_program().load.volume_each.value().to_string(),
            "82"
        );
        assert_eq!(
            incubation.as_program().stages[0].steps[0]
                .hold
                .value()
                .to_string(),
            "1800"
        );
        let transformation = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.lab.plasmid.transform")
            .expect("transformation is a global method choice");
        assert_eq!(transformation.candidates.len(), 2);
        let automated_transformation = transformation
            .candidates
            .iter()
            .find(|candidate| {
                candidate
                    .method
                    .as_str()
                    .ends_with("#automated-chemical-transformation")
            })
            .expect("automated transformation is a real method alternative");
        assert_eq!(automated_transformation.tasks.len(), 2);
        let preparation = automated_transformation.tasks[0]
            .program
            .as_ref()
            .expect("transformation preparation is normalized")
            .validate()
            .expect("transformation preparation validates");
        let ValidatedProcedureProgram::PipettingV1(preparation) = preparation else {
            panic!("transformation preparation must be pipetting")
        };
        assert_eq!(preparation.as_program().steps.len(), 8);
        assert_eq!(automated_transformation.tasks[0].requirements.len(), 6);
        assert!(
            preparation
                .as_program()
                .vessels
                .iter()
                .any(|vessel| vessel.temperature.is_some()),
            "the competent-cell aliquot states the temperature it must be staged at"
        );
        let heat_shock = automated_transformation.tasks[1]
            .program
            .as_ref()
            .expect("heat shock is normalized")
            .validate()
            .expect("heat shock validates");
        let ValidatedProcedureProgram::ThermalV1(heat_shock) = heat_shock else {
            panic!("heat shock must be thermal")
        };
        assert_eq!(heat_shock.as_program().load.outputs.len(), 2);
        assert_eq!(
            heat_shock.as_program().load.volume_each.value().to_string(),
            "22"
        );
        assert!(matches!(
            transformation.candidates[0].tasks[0].materials[0].source,
            crate::planning::PlanningMaterialSource::ChoiceOutput { .. }
        ));

        let plating = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.lab.plasmid.plate")
            .expect("plating is a global method choice");
        let automated_plating = plating
            .candidates
            .iter()
            .find(|candidate| {
                candidate
                    .method
                    .as_str()
                    .ends_with("#automated-antibiotic-selection")
            })
            .expect("automated selective plating is a real method alternative");
        let plate_program = automated_plating.tasks[0]
            .program
            .as_ref()
            .expect("selective plating is normalized")
            .validate()
            .expect("selective plating validates");
        let ValidatedProcedureProgram::PipettingV1(plate_program) = plate_program else {
            panic!("selective plating must be pipetting")
        };
        assert_eq!(plate_program.as_program().steps.len(), 4);
        assert_eq!(plate_program.as_program().vessels.len(), 3);
        assert_eq!(automated_plating.tasks[0].requirements.len(), 3);

        let json = serde_json::to_string_pretty(&problem).expect("problem serializes");
        let decoded: PlanningProblem = serde_json::from_str(&json).expect("problem deserializes");
        decoded.validate().expect("decoded problem revalidates");
        assert_eq!(decoded, problem);
    }
}

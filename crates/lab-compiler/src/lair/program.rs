//! Owned, verified LAIR produced from checked source modules.

use std::collections::BTreeMap;

use lab_language::CheckedModule;
use lab_method::MethodRegistry;
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
use crate::lair::dialect::design::{DesignDnaSequenceOp, DesignPlasmidOp, DesignStrainOp};
use crate::lair::dialect::workflow::{
    DiluteOp, PlateOp, ProvisionOp, RealizeOp, RecoverOp, TransformOp,
};
use crate::lair::source_lowering::{
    BuildArtifactIntent, SourceLoweringError, WorkflowActionIntent, lower_build_intent,
};
use crate::lair::stage::{IrStage, detect_stage, initialize_stage, set_stage};

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
    #[error("Procedure-to-Protocol adapter projection failed: {0}")]
    Conversion(String),
    #[error("generated Protocol LAIR failed verification: {0}")]
    Verification(String),
    #[error("generated Protocol LAIR failed material-linearity analysis: {0}")]
    MaterialLinearity(String),
    #[error("generated LAIR does not satisfy the method-selected Protocol contract: {0}")]
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

pub use crate::lair::planning_problem::PlanningProblemExtractionError;

#[derive(Debug, Error)]
pub enum AllocatedLairError {
    #[error(transparent)]
    Problem(#[from] PlanningProblemExtractionError),
    #[error(transparent)]
    Application(#[from] crate::lair::allocation::AllocationApplicationError),
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
        crate::lair::method_refinement::refine_method_alternatives(
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
        self.refine_methods(crate::lair::methods::standard_method_registry())
    }

    /// Consume method-neutral Workflow LAIR and select the supported concrete
    /// plasmid-build Protocol. No backend planning occurs at this boundary.
    pub fn select_protocol(mut self) -> Result<ProtocolLairProgram, ProtocolLairError> {
        crate::lair::protocol_selection::select_plasmid_build_protocol(
            &mut self.context,
            self.module.get_operation(),
        )
        .map_err(|error| ProtocolLairError::Conversion(error.disp(&self.context).to_string()))?;
        set_stage(
            &mut self.context,
            self.module,
            IrStage::MethodSelectedProtocol,
        )
        .map_err(ProtocolLairError::Stage)?;
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
        if stage != IrStage::MethodSelectedProtocol {
            return Err(ProtocolLairError::Stage(format!(
                "expected method-selected-protocol, found {stage}"
            )));
        }
        Ok(ProtocolLairProgram {
            context: self.context,
            module: self.module,
        })
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
        crate::lair::planning_problem::extract_planning_problem(&self.context, self.module)
    }

    /// Apply one complete solution to this exact refined module and eliminate every alternative.
    pub fn allocate(
        mut self,
        solution: crate::planning::FacilityPlanningSolution,
    ) -> Result<AllocatedLairProgram, AllocatedLairError> {
        let problem = self.planning_problem()?;
        crate::lair::allocation::apply_facility_solution(
            &mut self.context,
            self.module,
            &problem,
            &solution,
        )?;
        set_stage(&mut self.context, self.module, IrStage::AllocatedProcedure)
            .map_err(AllocatedLairError::Stage)?;
        verify_operation(self.module.get_operation(), &self.context).map_err(|error| {
            AllocatedLairError::Verification(error.disp(&self.context).to_string())
        })?;
        crate::lair::analysis::MaterialLinearityAnalysis::compute(
            self.module.get_operation(),
            &self.context,
            &mut AnalysisManager::default(),
        )
        .map_err(|error| {
            AllocatedLairError::MaterialLinearity(error.disp(&self.context).to_string())
        })?;
        let stage = detect_stage(&self.context, self.module).map_err(AllocatedLairError::Stage)?;
        if stage != IrStage::AllocatedProcedure {
            return Err(AllocatedLairError::Stage(format!(
                "expected allocated-procedure, found {stage}"
            )));
        }
        Ok(AllocatedLairProgram {
            context: self.context,
            module: self.module,
            problem,
            solution,
        })
    }
}

/// Owned, verifier-valid Procedure LAIR with all method and facility decisions frozen.
pub struct AllocatedLairProgram {
    context: Context,
    module: ModuleOp,
    problem: crate::planning::PlanningProblem,
    solution: crate::planning::FacilityPlanningSolution,
}

impl AllocatedLairProgram {
    pub fn ir(&self) -> String {
        self.module.get_operation().disp(&self.context).to_string()
    }

    pub fn solution(&self) -> &crate::planning::FacilityPlanningSolution {
        &self.solution
    }

    pub fn planning_problem(&self) -> &crate::planning::PlanningProblem {
        &self.problem
    }

    /// Project the exact backend-facing ABI from this verifier-valid allocated program.
    pub fn adapter_invocations(
        &self,
        material_inventory: crate::planning::MaterialLotBuildInventory,
    ) -> Result<crate::planning::AdapterInvocationPlan, crate::planning::AdapterInvocationError>
    {
        let ir = self.ir();
        let allocated_lair_sha256 = crate::planning::hex_sha256(ir.as_bytes());
        crate::planning::AdapterInvocationPlan::project(
            &self.problem,
            &self.solution,
            allocated_lair_sha256,
            material_inventory,
        )
    }

    /// Project this exact selected Procedure graph into the mature dependency-build adapter IR.
    ///
    /// This compatibility IR is downstream of allocation: it cannot select a Method, Asset,
    /// offering, or adapter, and it never consults checked source or unresolved Workflow Intent.
    pub(crate) fn dependency_build_protocol(
        &self,
    ) -> Result<ProtocolLairProgram, ProtocolLairError> {
        let (context, module) = crate::lair::allocated_protocol::project_dependency_build_protocol(
            &self.context,
            self.module,
        )
        .map_err(ProtocolLairError::Conversion)?;
        verify_operation(module.get_operation(), &context)
            .map_err(|error| ProtocolLairError::Verification(error.disp(&context).to_string()))?;
        crate::lair::analysis::MaterialLinearityAnalysis::compute(
            module.get_operation(),
            &context,
            &mut AnalysisManager::default(),
        )
        .map_err(|error| ProtocolLairError::MaterialLinearity(error.disp(&context).to_string()))?;
        let stage = detect_stage(&context, module).map_err(ProtocolLairError::Stage)?;
        if stage != IrStage::MethodSelectedProtocol {
            return Err(ProtocolLairError::Stage(format!(
                "expected method-selected-protocol, found {stage}"
            )));
        }
        Ok(ProtocolLairProgram { context, module })
    }
}

/// Owned, verifier-valid Protocol LAIR. Robot planners consume this boundary
/// directly; it cannot be constructed from unchecked source or device IR.
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
    use lab_capability::ScalarValue;
    use lab_inventory::InventorySnapshot;
    use lab_language::{
        ModuleId, SemanticEnvironment, compile_module, compile_module_in_environment,
    };
    use lab_method::{IntentOperationId, ProcedureValue, ScalarType};

    use crate::backend::default_adapter_profile;
    use crate::lair::session::CompilerSession;
    use crate::lair::stage::IrStage;
    use crate::planning::{
        AdapterBindingRequest, AdapterBindingSnapshot, AdapterInvocationPlan, AdapterRequirement,
        BuildInventory, FacilityPlanningPolicy, FacilityPlanningSolution, MethodPin,
        MethodPinSelector, PlanningProblem, PlanningValueSource,
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
            .refine_methods(&lab_method::MethodRegistry::default())
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
        assert_eq!(realization.candidates.len(), 2);
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
        assert!(matches!(
            automated.tasks[1].inputs[0].source,
            PlanningValueSource::TaskOutput { ref task, ref output }
                if task.as_str().ends_with("::setup-reaction") && output.as_str() == "reaction"
        ));

        let recovery = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.lab.plasmid.recover")
            .expect("recovery is a global method choice");
        assert_eq!(recovery.candidates.len(), 2);
        for candidate in &recovery.candidates {
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

        let json = serde_json::to_string_pretty(&problem).expect("problem serializes");
        let decoded: PlanningProblem = serde_json::from_str(&json).expect("problem deserializes");
        decoded.validate().expect("decoded problem revalidates");
        assert_eq!(decoded, problem);
    }

    #[test]
    fn a_complete_solution_produces_verifier_valid_allocated_procedure_lair() {
        let source = format!("{DESIGNS}{WORKFLOWS}")
            .replace("use demo.designs\n", "")
            .replace("use std.bio.designs\nuse std.bio.golden_gate\n", "")
            .replacen(
                "use std.bio.build",
                "use std.bio.designs\nuse std.bio.golden_gate\nuse std.bio.build",
                1,
            );
        let checked = compile_module(&source).expect("program checks");
        let refined = PortableLairProgram::lower(&checked)
            .expect("portable LAIR lowers")
            .refine_standard_methods()
            .expect("standard methods refine");
        let problem = refined.planning_problem().expect("problem projects");
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let inventory = InventorySnapshot::load(
            workspace.join("examples/golden-gate"),
            "inventory/facility.ttl",
            None,
        )
        .expect("Golden Gate inventory validates");
        let policy = FacilityPlanningPolicy {
            method_pins: vec![MethodPin {
                selector: MethodPinSelector::SourceOperation {
                    source_operation: IntentOperationId::new("std.bio.build.realize").unwrap(),
                },
                method: lab_capability::MethodId::new(
                    "https://www.lab-compiler.org/ns/method#automated-golden-gate",
                )
                .unwrap(),
            }],
            adapter_requirement: AdapterRequirement::Optional,
        };
        let adapters = AdapterBindingSnapshot::resolve(
            &inventory,
            vec![AdapterBindingRequest {
                asset: "https://example.org/golden-gate/opentrons_ot2".to_owned(),
                driver: "opentrons.ot2".to_owned(),
                profile_path: std::path::PathBuf::from("adapters/opentrons-ot2.toml"),
                profile: default_adapter_profile("opentrons.ot2", "opentrons-ot2").unwrap(),
            }],
        )
        .unwrap();
        let solution =
            FacilityPlanningSolution::solve(&problem, &inventory, Some(&adapters), policy).unwrap();
        let allocated = refined.allocate(solution).expect("solution applies");
        let ir = allocated.ir();

        assert!(ir.contains("allocated-procedure"), "{ir}");
        assert!(ir.contains("allocation.context"), "{ir}");
        assert!(ir.contains("allocation.method"), "{ir}");
        assert!(ir.contains("allocation.binding"), "{ir}");
        assert!(
            ir.contains("https://example.org/golden-gate/opentrons_ot2"),
            "{ir}"
        );
        assert!(ir.contains("#manual-recovery"), "{ir}");
        assert!(!ir.contains("method.choice"), "{ir}");
        assert!(!ir.contains("method.yield"), "{ir}");

        let protocol = allocated
            .dependency_build_protocol()
            .expect("allocated Procedure projects into the mature adapter IR");
        let protocol_ir = protocol.ir();
        assert!(
            protocol_ir.contains("method-selected-protocol"),
            "{protocol_ir}"
        );
        assert!(protocol_ir.contains("protocol.assemble"), "{protocol_ir}");
        assert!(protocol_ir.contains("protocol.transform"), "{protocol_ir}");
        assert!(!protocol_ir.contains("workflow."), "{protocol_ir}");
        assert!(!protocol_ir.contains("procedure."), "{protocol_ir}");
        assert!(!protocol_ir.contains("allocation."), "{protocol_ir}");

        let active_lots = inventory.active_material_lots().unwrap();
        let lots_by_component = active_lots
            .components()
            .map(|(component, lots)| {
                (
                    component.as_str().to_owned(),
                    lots.iter().map(|lot| lot.as_str().to_owned()).collect(),
                )
            })
            .collect();
        let BuildInventory::MaterialLots(material_inventory) = BuildInventory::from_material_lots(
            &[&checked],
            inventory.source_sha256(),
            inventory.facility().as_str(),
            &lots_by_component,
        )
        .unwrap() else {
            unreachable!()
        };
        let invocations = allocated.adapter_invocations(material_inventory).unwrap();
        assert_eq!(invocations.invocations.len(), 1);
        assert_eq!(
            invocations.invocations[0].asset,
            "https://example.org/golden-gate/opentrons_ot2"
        );
        assert_eq!(invocations.invocations[0].adapter.driver, "opentrons.ot2");
        assert!(
            invocations.invocations[0]
                .requirements
                .iter()
                .any(|requirement| requirement.as_str().ends_with("::liquid-handling"))
        );
        assert!(
            invocations
                .methods
                .iter()
                .any(|method| { method.method.as_str().ends_with("#automated-golden-gate") })
        );
        assert!(invocations.methods.iter().any(|method| {
            method.method.as_str().ends_with("#manual-recovery")
                && method.tasks[0].requirements[0].adapter.is_none()
        }));
        let json = serde_json::to_string_pretty(&invocations).unwrap();
        let decoded: AdapterInvocationPlan = serde_json::from_str(&json).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded, invocations);
        let mut mismatched_inventory = decoded.clone();
        mismatched_inventory.material_inventory.source_sha256 = "0".repeat(64);
        assert!(matches!(
            mismatched_inventory.validate(),
            Err(crate::planning::AdapterInvocationValidationError::MaterialInventoryMismatch)
        ));

        let mut tampered = decoded;
        tampered.invocations[0]
            .tasks
            .push(lab_method::LocalId::new("task-that-was-never-allocated").unwrap());
        assert!(matches!(
            tampered.validate(),
            Err(crate::planning::AdapterInvocationValidationError::UnknownTask { .. })
        ));

        let mut session = CompilerSession::default();
        session.parse_ir(&ir).unwrap();
        session.verify_stage(IrStage::AllocatedProcedure).unwrap();
    }
}

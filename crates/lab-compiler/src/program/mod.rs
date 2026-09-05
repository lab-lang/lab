//! Owned, verified LAIR produced from checked source modules.

mod lowering;

use std::collections::BTreeMap;

use crate::method::MethodRegistry;
use crate::procedure::{ProcedureCompiler, ProcedureContractRegistry};
use lab_language::{CheckedModule, CheckedType};
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

use self::lowering::lower_rooted_program_intent;
use crate::design::ir::DesignArtifactOp;
use crate::stage::{
    IrStage, detect_stage, initialize_stage, set_stage, verify_refined_procedure_programs,
};
use crate::workflow::ir::{DataType as WorkflowDataType, MaterialType, PerformOp};

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
    #[error(transparent)]
    Semantic(#[from] crate::allocation::AllocatedProgramExtractionError),
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
    /// Lower the exact program reached from one entry module's `main` workflow.
    ///
    /// Workflow calls are expanded in statement order and every reachable
    /// action becomes Intent. Artifacts are included only when this execution
    /// reaches their realization workflows.
    pub fn lower_entry_program(
        modules: &[&CheckedModule],
        entry_module: &str,
    ) -> Result<Self, PortableLairError> {
        let rooted = lower_rooted_program_intent(modules, entry_module)?;
        Self::from_rooted_intent(rooted.designs, rooted.actions)
    }

    fn from_rooted_intent(
        artifacts: Vec<crate::design::ArtifactDesign>,
        actions: Vec<crate::workflow::ir::IntentAction>,
    ) -> Result<Self, PortableLairError> {
        let mut context = Context::new();
        let root = ModuleOp::new(
            &mut context,
            Identifier::try_from("lab_build").expect("static module name is valid"),
        );
        initialize_stage(&mut context, root, IrStage::DesignIntent);
        let mut designs = BTreeMap::new();
        for artifact in &artifacts {
            let operation = DesignArtifactOp::new(&mut context, artifact);
            let design = operation.get_result_design(&context);
            root.append_operation(&mut context, operation.get_operation(), 0);
            designs.insert(artifact.definition.clone(), design);
        }
        append_actions(&mut context, root, &designs, &actions)?;
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
        procedures: &ProcedureCompiler,
    ) -> Result<RefinedLairProgram, RefinedLairError> {
        crate::method::refinement::refine_method_alternatives(
            &mut self.context,
            self.module.get_operation(),
            registry,
            procedures,
        )
        .map_err(|error| RefinedLairError::Conversion(error.disp(&self.context).to_string()))?;
        set_stage(&mut self.context, self.module, IrStage::RefinedAlternatives)
            .map_err(RefinedLairError::Stage)?;
        verify_operation(self.module.get_operation(), &self.context).map_err(|error| {
            RefinedLairError::Verification(error.disp(&self.context).to_string())
        })?;
        verify_refined_procedure_programs(&self.context, self.module, procedures.contracts())
            .map_err(RefinedLairError::Stage)?;
        let stage = detect_stage(&self.context, self.module).map_err(RefinedLairError::Stage)?;
        if stage != IrStage::RefinedAlternatives {
            return Err(RefinedLairError::Stage(format!(
                "expected refined-alternatives, found {stage}"
            )));
        }
        Ok(RefinedLairProgram {
            context: self.context,
            module: self.module,
            contracts: procedures.contracts().clone(),
        })
    }
}

/// Owned, verifier-valid Method alternatives with no facility allocation or selected candidate.
pub struct RefinedLairProgram {
    context: Context,
    module: ModuleOp,
    contracts: ProcedureContractRegistry,
}

impl RefinedLairProgram {
    pub fn ir(&self) -> String {
        self.module.get_operation().disp(&self.context).to_string()
    }

    /// The exact Procedure semantics retained from refinement for every later revalidation.
    pub fn procedure_contracts(&self) -> &ProcedureContractRegistry {
        &self.contracts
    }

    /// Project immutable, facility-independent constraints for the global planner.
    pub fn planning_problem(
        &self,
    ) -> Result<crate::planning::PlanningProblem, PlanningProblemExtractionError> {
        crate::planning::extract_planning_problem(&self.context, self.module, &self.contracts)
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
        let allocated = AllocatedLairProgram {
            context: self.context,
            module: self.module,
            source,
            contracts: self.contracts,
        };
        allocated.allocated_program()?;
        Ok(allocated)
    }
}

/// Owned, verifier-valid Procedure LAIR with all method and facility decisions frozen.
pub struct AllocatedLairProgram {
    context: Context,
    module: ModuleOp,
    source: String,
    contracts: ProcedureContractRegistry,
}

impl AllocatedLairProgram {
    /// Parse and verify a complete textual Allocated LAIR program.
    ///
    /// The resulting program has no dependency on the planning problem or solution that
    /// originally produced the text; every backend-facing semantic fact is reconstructed from
    /// the allocated IR itself.
    pub fn parse_ir(
        source: &str,
        contracts: &ProcedureContractRegistry,
    ) -> Result<Self, AllocatedLairError> {
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
        let allocated = Self {
            context,
            module,
            source: source.to_owned(),
            contracts: contracts.clone(),
        };
        allocated.allocated_program()?;
        Ok(allocated)
    }

    /// The exact Procedure semantics retained from refinement or explicit parsing.
    pub fn procedure_contracts(&self) -> &ProcedureContractRegistry {
        &self.contracts
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
        crate::allocation::extract_allocated_program(&self.context, self.module, &self.contracts)
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

fn append_actions(
    context: &mut Context,
    root: ModuleOp,
    designs: &BTreeMap<lab_language::DefinitionId, pliron::value::Value>,
    actions: &[crate::workflow::IntentAction],
) -> Result<(), PortableLairError> {
    let mut values = BTreeMap::new();
    for intent in actions {
        let mut operand_names = Vec::new();
        let mut operands = Vec::new();
        for name in &intent.ssa_operands {
            let argument = intent
                .action
                .arguments
                .iter()
                .find(|argument| argument.name == *name)
                .expect("validated Intent SSA operands name action arguments");
            let lab_language::CheckedExpression::Reference { definition, path } =
                &argument.value.value
            else {
                return Err(SourceLoweringError::UnsupportedProjectedOperand {
                    operation: intent.action.display_name().to_owned(),
                    argument: argument.name.clone(),
                    path: Vec::new(),
                }
                .into());
            };
            let [binding] = path.as_slice() else {
                return Err(SourceLoweringError::UnsupportedProjectedOperand {
                    operation: intent.action.display_name().to_owned(),
                    argument: argument.name.clone(),
                    path: path.clone(),
                }
                .into());
            };
            let value = values
                .get(definition)
                .or_else(|| designs.get(definition))
                .copied()
                .ok_or_else(|| SourceLoweringError::UnboundSsaOperand {
                    operation: intent.action.display_name().to_owned(),
                    argument: argument.name.clone(),
                    binding: format!("{}::{}", definition.module, binding),
                })?;
            operand_names.push(argument.name.clone());
            operands.push(value);
        }
        let result_types = intent
            .result_bindings
            .iter()
            .map(|result| workflow_result_type(context, &result.r#type))
            .collect::<Vec<_>>();
        let performed = PerformOp::new(context, intent, operand_names, operands, result_types);
        for (binding, result) in intent
            .result_bindings
            .iter()
            .zip(performed.results(context))
        {
            values.insert(
                lab_language::DefinitionId::exported(&intent.source.module, &binding.name),
                result,
            );
        }
        root.append_operation(context, performed.get_operation(), 0);
    }
    Ok(())
}

fn workflow_result_type(context: &Context, ty: &CheckedType) -> pliron::r#type::TypeHandle {
    let CheckedType::Named { name, arguments } = ty else {
        return WorkflowDataType::kind(context, &type_kind_name(ty));
    };
    if name != "Material" {
        return WorkflowDataType::kind(context, &type_kind_name(ty));
    }
    let state = match arguments.first() {
        Some(CheckedType::InState { state, .. }) => state.clone(),
        Some(subject) => format!("{}Product", subject.subject().display_name()),
        None => "MaterialProduct".to_owned(),
    };
    MaterialType::state(context, &state)
}

fn type_kind_name(ty: &CheckedType) -> String {
    let name = ty.subject().display_name();
    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return name;
    }
    format!(
        "type-{}",
        name.as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use crate::method::{
        CapabilityRequirementDefinition, IntentOperationId, LocalId, MethodCatalogDocument,
        MethodDefinition, MethodInput, MethodOutput, MethodParameter, MethodRegistry,
        ParameterType, PortType, ProcedureParameterDefinition, ProcedureTaskDefinition,
        ProcedureTaskExecutionDefinition, ProcedureValue, ProcedureValueExpression, ScalarType,
        TaskOutput, ValueReference,
    };
    use crate::procedure::{
        AspirationStrategy, DispenseStrategy, PipettingStep, ProcedureCompiler,
        ProcedureProgramBuilderRegistry, builtin_procedure_contracts, vocabulary,
    };
    use lab_capability::{
        AbsoluteIri, CapabilityKind, ControlMode, MethodId, OperationId, PropertyKind,
        QualificationLevel, ScalarValue,
    };
    use lab_language::{
        ModuleId, SemanticEnvironment, compile_module, compile_module_in_environment,
    };

    use crate::planning::{
        FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION, FacilityPlanningPolicy,
        FacilityPlanningSolution, PlanningProblem, PlanningValueSource, SelectedMethod,
        SelectedProcedureTask, SelectedRequirementBinding,
    };
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
  competence = competent
  efficiency = 1e9 cfu/ug
buy antibiotic chloramphenicol:
  sbol_identity = "https://example.org/golden-gate/materials/chloramphenicol"
buy medium LB_chloramphenicol_agar:
  sbol_identity = "https://example.org/golden-gate/materials/LB_chloramphenicol_agar"
  pouring = poured
  selection = chloramphenicol
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
  plate: Material<Medium is inoculated>,
):
  dependencies = [p_gfp]
  cells <- provision DH5alpha
  strain, culture <- transform reporter_host from dependencies into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  agar <- provision LB_chloramphenicol_agar
  plate <- plate culture on agar
  return strain, plate

workflow main() -> Material<Strain>:
  plasmid <- assemble_p_gfp
  strain, plate <- build_reporter_host plasmid
  <- dispose plate
  return strain
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

workflow main() -> (
  first_product: Material<Plasmid>,
  second_product: Material<Plasmid>,
):
  first_product <- build_first
  second_product <- build_second
  return first_product, second_product
"#;

    /// Fetching something off a shelf yields what was asked for.
    ///
    /// Provisioning minted competent cells for every item, so an antibiotic
    /// arrived in LAIR as a value the IR believed was a tube of cells and no
    /// later check disagreed. One provisioning signature still serves every
    /// kind, because the state is the one the Intent asked for.
    /// The reason the whole state machinery exists: making LB media.
    ///
    /// A medium is realized from its declaration exactly as a plasmid is, and
    /// arrives as a `MediumProduct` rather than being refused for not being a
    /// plasmid. The Golden Gate methods do not apply, because the Intent
    /// carries no assembly recipe, so the one candidate is manual realization.
    #[test]
    fn a_medium_realizes_without_being_a_plasmid() {
        const SOURCE: &str = r#"use std.bio.designs
use std.bio.build

build medium LB_broth:
  sbol_identity = "https://example.org/media/LB_broth"
  ph = 7.0
  components = [
    Ingredient { substance: "tryptone", concentration: 10 g/L },
    Ingredient { substance: "yeast extract", concentration: 5 g/L },
    Ingredient { substance: "sodium chloride", concentration: 10 g/L },
  ]

workflow make_LB() -> Material<Medium>:
  product <- realize LB_broth
  return product

workflow main() -> Material<Medium>:
  product <- make_LB
  return product
"#;
        let module = lab_language::compile_module(SOURCE).expect("module checks");
        let program = PortableLairProgram::lower_entry_program(&[&module], module.module.as_str())
            .expect("program lowers");
        let intent = program.ir();
        assert!(
            intent.contains("design.define"),
            "a medium has a design of its own: {intent}"
        );
        assert!(
            intent.contains("\\\"artifact\\\":\\\"medium\\\""),
            "the package-defined artifact kind survives generically: {intent}"
        );
        assert!(
            intent.contains("material-state#MediumProduct"),
            "a realized medium is a MediumProduct: {intent}"
        );

        let refined = program
            .refine_methods(
                crate::method::standard_method_registry(),
                crate::procedure::builtin_procedure_compiler(),
            )
            .expect("the manual realization method refines it")
            .ir();
        assert!(
            refined.contains("method#manual-artifact-realization"),
            "manual realization applies: {refined}"
        );
        assert!(
            !refined.contains("golden-gate"),
            "an Intent with no assembly recipe is not a Golden Gate candidate: {refined}"
        );
    }

    /// Declaring scientific vocabulary does not silently invent execution semantics. A Method
    /// must explicitly refine every action that reaches Procedure lowering.
    #[test]
    fn an_action_without_a_registered_method_fails_closed() {
        const SOURCE: &str = r#"use std.bio.designs
use std.bio.build

build medium LB_broth:
  components = [
    Ingredient { substance: "tryptone", concentration: 10 g/L },
  ]

action degas <medium> for <duration> -> degassed:
  medium: take Material<Medium>
  duration: Quantity<min>
  degassed: Material<Medium> continues from medium

workflow make_LB() -> Material<Medium>:
  broth <- realize LB_broth
  clear <- degas broth for 5 min
  return clear

workflow main() -> Material<Medium>:
  product <- make_LB
  return product
"#;
        let module = lab_language::compile_module(SOURCE).expect("module checks");
        let portable = PortableLairProgram::lower_entry_program(&[&module], module.module.as_str())
            .expect("program lowers");
        let intent = portable.ir();
        assert!(
            intent.contains("workflow.perform"),
            "the declared verb lowers to a perform Intent: {intent}"
        );

        let error = portable
            .refine_methods(
                crate::method::standard_method_registry(),
                crate::procedure::builtin_procedure_compiler(),
            )
            .err()
            .expect("an action with no Method must not acquire implicit execution semantics");
        assert!(
            error.to_string().contains("no method definition")
                && error.to_string().contains("standalone.degas"),
            "{error}"
        );
    }

    #[test]
    fn nested_workflow_effects_fail_at_the_control_boundary() {
        let checked = compile_module(
            r#"use std.bio.designs
use std.bio.build

build medium broth:
  components = [Ingredient { substance: "tryptone", concentration: 10 g/L }]

workflow prepare() -> Material<Medium>:
  product <- realize broth
  if 1 == 1:
    return product
  return product

workflow main() -> Material<Medium>:
  product <- prepare
  return product
"#,
        )
        .expect("the source control flow checks");
        let error = PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
            .err()
            .expect("Design/Intent LAIR must not flatten a branch");
        assert!(
            error
                .to_string()
                .contains("unsupported control statement 'if'")
                && error.to_string().contains("statement path [0, 1]"),
            "{error}"
        );
    }

    /// Rooted lowering expands workflow composition instead of treating only
    /// artifact realization bodies as executable. The sample is created in a
    /// twice-nested call, then consumed by an action written directly in main.
    #[test]
    fn rooted_straight_line_workflows_inline_every_action_and_preserve_ssa() {
        let checked = compile_module(
            r#"use std.bio.designs

action create <label> -> sample:
  label: String
  sample: Material<Medium> begins

action inspect <sample> -> evidence:
  sample: take Material<Medium>
  evidence: Evidence continues from sample

workflow leaf(label: String) -> Material<Medium>:
  sample <- create label
  return sample

workflow middle(label: String) -> Material<Medium>:
  sample <- leaf label
  return sample

workflow main() -> Evidence:
  label = "batch-a"
  sample <- middle label
  evidence <- inspect sample
  return evidence
"#,
        )
        .expect("the package-defined straight-line program checks");

        let portable =
            PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
                .expect("a rooted action-only program lowers without an artifact");
        let intent = portable.ir();
        assert_eq!(intent.matches("workflow.perform").count(), 2, "{intent}");
        assert!(
            intent.find("standalone.create").unwrap() < intent.find("standalone.inspect").unwrap(),
            "nested calls retain execution order: {intent}"
        );
        assert!(intent.contains("call_2_statement_0_sample"), "{intent}");
        assert!(
            intent.contains("\\\"statement_path\\\":[1,0,0]"),
            "the nested action carries its complete call-site path: {intent}"
        );
        assert!(
            intent.contains("\\\"statement_path\\\":[2]"),
            "the direct main action retains its root statement path: {intent}"
        );

        let registry = MethodRegistry::new(vec![
            straight_line_method("create", None, "sample", PortType::MaterialAsRequested),
            straight_line_method(
                "inspect",
                Some((
                    "sample",
                    PortType::Material {
                        state: AbsoluteIri::new(format!(
                            "{}MediumProduct",
                            crate::workflow::ir::STATE_NS
                        ))
                        .unwrap(),
                    },
                )),
                "evidence",
                PortType::Data {
                    data_kind: AbsoluteIri::new(format!(
                        "{}Evidence",
                        crate::workflow::ir::DATA_NS
                    ))
                    .unwrap(),
                },
            ),
        ])
        .expect("the package Methods validate");
        let problem = portable
            .refine_methods(&registry, crate::procedure::builtin_procedure_compiler())
            .expect("both package actions refine explicitly")
            .planning_problem()
            .expect("the inlined SSA edge projects to planning");
        assert_eq!(problem.choices.len(), 2);
        assert!(matches!(
            problem.choices[1].inputs[0].source,
            Some(PlanningValueSource::ChoiceOutput { ref choice, ref output })
                if choice == &problem.choices[0].id && output.as_str() == "sample"
        ));
        assert!(
            problem
                .choices
                .iter()
                .all(|choice| choice.source_intent.artifact.is_none()),
            "an action-only workflow must not invent a build artifact"
        );
    }

    #[test]
    fn repeated_workflow_calls_have_distinct_paths_and_result_bindings() {
        let checked = compile_module(
            r#"use std.bio.designs

action create <label> -> sample:
  label: String
  sample: Material<Medium> begins

workflow leaf(label: String) -> Material<Medium>:
  sample <- create label
  return sample

workflow main() -> (
  first: Material<Medium>,
  second: Material<Medium>,
):
  first_label = "first"
  first <- leaf first_label
  second_label = "second"
  second <- leaf second_label
  return first, second
"#,
        )
        .expect("the repeated straight-line calls check");

        let intent = PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
            .expect("both call instances lower")
            .ir();
        assert_eq!(intent.matches("workflow.perform").count(), 2, "{intent}");
        assert!(intent.contains("\\\"statement_path\\\":[1,0]"), "{intent}");
        assert!(intent.contains("\\\"statement_path\\\":[3,0]"), "{intent}");
        assert!(intent.contains("call_1_statement_0_sample"), "{intent}");
        assert!(intent.contains("call_2_statement_0_sample"), "{intent}");
    }

    #[test]
    fn same_local_artifact_names_remain_module_qualified() {
        let first = compile_module_in_environment(
            ModuleId::new("pkg.first"),
            r#"use std.bio.designs
use std.bio.build

build medium sample:
  components = [Ingredient { substance: "first", concentration: 1 g/L }]

workflow build_first() -> Material<Medium>:
  product <- realize sample
  return product

workflow main() -> Material<Medium>:
  product <- build_first
  return product
"#,
            &SemanticEnvironment::default(),
        )
        .expect("the first package checks");
        let second = compile_module_in_environment(
            ModuleId::new("pkg.second"),
            r#"use std.bio.designs
use std.bio.build

build medium sample:
  components = [Ingredient { substance: "second", concentration: 1 g/L }]

workflow build_second() -> Material<Medium>:
  product <- realize sample
  return product
"#,
            &SemanticEnvironment::default(),
        )
        .expect("the second package checks");
        let intent =
            PortableLairProgram::lower_entry_program(&[&first, &second], first.module.as_str())
                .expect("the unused same-local declaration cannot overwrite the rooted artifact")
                .ir();
        assert_eq!(intent.matches(" = design.define ").count(), 1, "{intent}");
        assert!(
            intent.contains("\\\"module\\\":\\\"pkg.first\\\",\\\"local\\\":\\\"sample\\\""),
            "{intent}"
        );
        assert!(
            !intent.contains("\\\"module\\\":\\\"pkg.second\\\",\\\"local\\\":\\\"sample\\\""),
            "{intent}"
        );
    }

    #[test]
    fn projected_runtime_values_fail_instead_of_losing_the_operand() {
        let material = PortType::Material {
            state: AbsoluteIri::new(format!("{}SampleProduct", crate::workflow::ir::STATE_NS))
                .unwrap(),
        };
        let producer = crate::workflow::ir::synthetic_intent_with_ports(
            "example.produce",
            &[],
            &[("sample".to_owned(), material.clone())],
        );
        let mut consumer = crate::workflow::ir::synthetic_intent_with_ports(
            "example.consume",
            &[("sample".to_owned(), material)],
            &[],
        );
        let lab_language::CheckedExpression::Reference { path, .. } =
            &mut consumer.action.arguments[0].value.value
        else {
            panic!("the synthetic input is a reference")
        };
        path.push("projection".to_owned());

        let error = PortableLairProgram::from_rooted_intent(Vec::new(), vec![producer, consumer])
            .err()
            .expect("a projected SSA value is not silently omitted");
        assert!(
            error
                .to_string()
                .contains("unsupported projected SSA operand")
                && error.to_string().contains("projection"),
            "{error}"
        );
    }

    #[test]
    fn rooted_lowering_rejects_state_instead_of_erasing_mutation() {
        let checked = compile_module(
            r#"workflow main() -> Integer:
  state count: Integer = 0
  count = count + 1
  return count
"#,
        )
        .expect("the stateful workflow checks in the source language");
        let error = PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
            .err()
            .expect("straight-line Intent lowering cannot erase durable state");
        assert!(
            error.to_string().contains("declares durable state"),
            "{error}"
        );
    }

    #[test]
    fn rooted_artifact_calls_keep_dependencies_and_direct_main_actions() {
        let source = format!(
            "{DESIGNS}{WORKFLOWS}\nworkflow main() -> Material<Strain>:\n  plasmid <- assemble_p_gfp\n  retained, aliquot <- split plasmid\n  strain, plate <- build_reporter_host retained\n  <- dispose aliquot\n  <- dispose plate\n  return strain\n"
        )
        .replace(
            "\nworkflow main() -> Material<Strain>:\n  plasmid <- assemble_p_gfp\n  strain, plate <- build_reporter_host plasmid\n  <- dispose plate\n  return strain\n",
            "",
        )
        .replace("use demo.designs\n", "")
        .replace("use std.bio.designs\nuse std.bio.golden_gate\n", "")
        .replacen(
            "use std.bio.build",
            "use std.bio.designs\nuse std.bio.golden_gate\nuse std.bio.build",
            1,
        );
        let checked = compile_module(&source).expect("the rooted build checks");
        let portable =
            PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
                .expect("the complete rooted build lowers");
        let intent = portable.ir();
        assert_eq!(
            intent.matches("workflow.perform").count(),
            10,
            "one realize, six build actions, and main's split and disposals all survive: {intent}"
        );
        assert!(intent.contains("std.lab.plasmid.split"), "{intent}");
        assert!(intent.contains("std.lab.plasmid.dispose"), "{intent}");

        let problem = portable
            .refine_methods(
                crate::method::standard_method_registry(),
                crate::procedure::builtin_procedure_compiler(),
            )
            .expect("the rooted standard actions refine")
            .planning_problem()
            .expect("the rooted dependencies project");
        let plasmid_realization = problem
            .choices
            .iter()
            .find(|choice| {
                choice.source_operation.as_str() == "std.bio.build.realize"
                    && choice
                        .source_intent
                        .artifact
                        .as_ref()
                        .is_some_and(|artifact| artifact.name == "p_gfp")
            })
            .expect("the plasmid realization is present");
        let transformation = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.lab.plasmid.transform")
            .expect("the strain transformation is present");
        assert_eq!(
            transformation.source_intent.artifact_dependencies,
            ["standalone::p_gfp"]
        );
        assert_eq!(
            transformation.after.as_slice(),
            std::slice::from_ref(&plasmid_realization.id)
        );
        assert!(matches!(
            transformation.source_intent.parameters.get("plasmids_count"),
            Some(ProcedureValue::Scalar { value })
                if matches!(&value.value, ScalarValue::Integer(value) if value.to_string() == "1")
        ));
        assert!(matches!(
            transformation.source_intent.parameters.get("plasmids"),
            Some(ProcedureValue::List { values, .. })
                if matches!(&values[..], [value]
                    if matches!(&value.value, ScalarValue::Text(value) if value == "p_gfp"))
        ));
        let dispose = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "std.lab.plasmid.dispose")
            .expect("main's direct action becomes a planning choice");
        assert!(dispose.source_intent.artifact.is_none());
        assert!(
            problem
                .choices
                .iter()
                .any(|choice| choice.source_operation.as_str() == "std.lab.plasmid.split")
        );
    }

    fn straight_line_method(
        operation: &str,
        input: Option<(&str, PortType)>,
        output: &str,
        output_type: PortType,
    ) -> MethodDefinition {
        let local = |name: &str| LocalId::new(name).unwrap();
        let task = local(operation);
        let inputs = input
            .as_ref()
            .map(|(name, port_type)| MethodInput {
                name: local(name),
                port_type: port_type.clone(),
            })
            .into_iter()
            .collect::<Vec<_>>();
        let task_inputs = input
            .as_ref()
            .map(|(name, _)| ValueReference::Input { input: local(name) })
            .into_iter()
            .collect::<Vec<_>>();
        MethodDefinition {
            id: MethodId::new(format!("https://example.org/method#{operation}")).unwrap(),
            refines: IntentOperationId::new(format!("standalone.{operation}")).unwrap(),
            inputs,
            parameters: vec![],
            tasks: vec![ProcedureTaskDefinition {
                id: task.clone(),
                operation: OperationId::new(format!("https://example.org/procedure#{operation}"))
                    .unwrap(),
                inputs: task_inputs,
                outputs: vec![TaskOutput {
                    name: local(output),
                    port_type: output_type.clone(),
                }],
                parameters: vec![],
                materials: vec![],
                execution: ProcedureTaskExecutionDefinition::Primitive {
                    requirements: vec![CapabilityRequirementDefinition {
                        id: local("execution"),
                        capability_kind: CapabilityKind::new(format!(
                            "https://example.org/capability#{operation}"
                        ))
                        .unwrap(),
                        minimum_qualification: QualificationLevel::Plannable,
                        accepted_control_modes: [ControlMode::Manual].into_iter().collect(),
                        constraints: vec![],
                    }],
                },
            }],
            outputs: vec![MethodOutput {
                name: local(output),
                source: ValueReference::TaskOutput {
                    task,
                    output: local(output),
                },
            }],
        }
    }

    /// A package adds a typed scientific verb and two alternative Methods
    /// without adding an operation class or a compiler dispatch arm.
    #[test]
    fn a_package_action_reaches_two_registered_methods_through_generic_intent() {
        let provider = compile_module_in_environment(
            ModuleId::new("pkg.conditioning"),
            r#"use std.bio.designs

action condition <sample> named <label> at <temperature> for <cycles> with <flags> -> conditioned, evidence:
  sample: take Material<Medium>
  label: String
  temperature: Quantity<C>
  cycles: Integer
  flags: List<String>
  conditioned: Material<Medium> continues from sample
  evidence: Evidence continues from sample
"#,
            &SemanticEnvironment::default(),
        )
        .expect("the package action checks");
        let mut environment = SemanticEnvironment::default();
        environment.insert("pkg.conditioning", provider.interface.clone());
        let consumer = compile_module_in_environment(
            ModuleId::new("demo.experiment"),
            r#"use std.bio.designs
use std.bio.build
use pkg.conditioning

build medium broth:
  components = [Ingredient { substance: "tryptone", concentration: 10 g/L }]

workflow prepare() -> Material<Medium>:
  label = "batch-a"
  flags = ["alpha", "beta"]
  sample <- realize broth
  conditioned, evidence <- condition sample named label at 30 C for 3 with flags
  return conditioned

workflow main() -> Material<Medium>:
  product <- prepare
  return product
"#,
            &environment,
        )
        .expect("the consumer checks against the package interface");

        let portable = PortableLairProgram::lower_entry_program(
            &[&provider, &consumer],
            consumer.module.as_str(),
        )
        .expect("the package action lowers through workflow.perform");
        let intent = portable.ir();
        assert_eq!(intent.matches("workflow.perform").count(), 2, "{intent}");
        for preserved in [
            "pkg.conditioning.condition",
            "\\\"module\\\":\\\"pkg.conditioning\\\"",
            "\\\"mode\\\":\\\"take\\\"",
            "\\\"lineage\\\":{\\\"kind\\\":\\\"continues\\\"",
            "batch-a",
            "http://qudt.org/vocab/unit/DEG_C",
            "\\\"value\\\":\\\"3\\\"",
            "alpha",
            "beta",
            "workflow.data",
        ] {
            assert!(intent.contains(preserved), "missing {preserved}: {intent}");
        }

        let mut methods = crate::method::standard_method_definitions();
        methods.push(conditioning_method("condition-by-hand"));
        methods.push(conditioning_method("condition-automatically"));
        let registry = MethodRegistry::new(methods).expect("the two package Methods validate");
        let problem = portable
            .refine_methods(&registry, crate::procedure::builtin_procedure_compiler())
            .expect("generic refinement uses the package Methods")
            .planning_problem()
            .expect("the alternatives project");
        let choice = problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "pkg.conditioning.condition")
            .expect("the package action becomes a Method choice");
        assert_eq!(choice.candidates.len(), 2);
    }

    /// A package-defined artifact and facet cross every generic compiler
    /// boundary without teaching the compiler their names.
    #[test]
    fn package_artifact_identity_and_semantics_reach_method_planning_losslessly() {
        let provider = compile_module_in_environment(
            ModuleId::new("pkg.specimens"),
            r#"record Specimen

artifact Specimen:
  label: String
  score: Integer

facet Readiness on Specimen:
  raw
  ready:
    confidence: Integer

  raw -> ready
"#,
            &SemanticEnvironment::default(),
        )
        .expect("the package artifact vocabulary checks");
        let mut environment = SemanticEnvironment::default();
        environment.insert("pkg.specimens", provider.interface.clone());
        let consumer = compile_module_in_environment(
            ModuleId::new("demo.experiment"),
            r#"use std.bio.build
use pkg.specimens

build specimen sample:
  sbol_identity = "https://example.org/design/sample"
  label = "sample-a"
  score = 7
  readiness = ready
  confidence = 9
  require score >= 5
  across 2 biological replicates
  accept confidence >= 8

workflow main() -> Material<Specimen>:
  product <- realize sample
  return product
"#,
            &environment,
        )
        .expect("the consumer checks against package vocabulary");

        let registry = MethodRegistry::new(vec![generic_artifact_realization_method()])
            .expect("the generic artifact Method validates");
        let refined = PortableLairProgram::lower_entry_program(
            &[&provider, &consumer],
            consumer.module.as_str(),
        )
        .expect("the package artifact lowers")
        .refine_methods(&registry, crate::procedure::builtin_procedure_compiler())
        .expect("the package artifact refines without a core kind branch");
        let problem = refined
            .planning_problem()
            .expect("the refined artifact projects to planning");
        let choice = &problem.choices[0];
        let design = choice
            .source_intent
            .artifact
            .as_ref()
            .expect("the full checked artifact remains on its Intent action");

        assert_eq!(design.definition.module.as_str(), "demo.experiment");
        assert_eq!(design.definition.local, "sample");
        assert_eq!(design.artifact, "specimen");
        assert_eq!(design.artifact_definitions.len(), 1);
        assert_eq!(
            design.artifact_definitions[0].module.as_str(),
            "pkg.specimens"
        );
        assert_eq!(design.artifact_definitions[0].local, "specimen");
        assert_eq!(design.type_definition.module.as_str(), "pkg.specimens");
        assert_eq!(design.type_definition.local, "Specimen");
        assert_eq!(design.facets.len(), 1);
        assert_eq!(design.facets[0].definition.module.as_str(), "pkg.specimens");
        assert_eq!(design.facets[0].definition.local, "Readiness");
        assert_eq!(design.facets[0].state, "ready");
        assert_eq!(
            design.sbol_identity.as_deref(),
            Some("https://example.org/design/sample")
        );
        assert_eq!(design.properties.len(), 3);
        assert_eq!(design.requirements.len(), 1);
        assert_eq!(design.acceptance.len(), 1);
        assert_eq!(design.acceptance[0].replicates, Some(2));

        let task = &choice.candidates[0].tasks[0];
        let projected = |name: &str| {
            task.parameters
                .iter()
                .find(|parameter| {
                    parameter
                        .id
                        .as_str()
                        .ends_with(&format!("::parameter::{name}"))
                })
                .map(|parameter| &parameter.value)
                .unwrap_or_else(|| panic!("Method parameter `{name}` was not projected"))
        };
        let artifact_document = match projected("artifact_design") {
            ProcedureValue::Scalar { value } => match &value.value {
                ScalarValue::Text(value) => value,
                other => panic!("artifact_design is not text: {other:?}"),
            },
            other => panic!("artifact_design is not scalar: {other:?}"),
        };
        let projected_design: crate::design::ArtifactDesign =
            serde_json::from_str(artifact_document).expect("the projected design is typed JSON");
        assert_eq!(&projected_design, design);
        assert!(matches!(
            projected("score"),
            ProcedureValue::Scalar { value }
                if matches!(value.value, ScalarValue::Integer(_))
        ));
        assert!(matches!(
            projected("sbol_identity"),
            ProcedureValue::Scalar { value }
                if matches!(value.value, ScalarValue::Iri(_))
        ));
        assert_eq!(choice.source_intent.action.arguments[0].name, "design");
        assert_eq!(
            choice.source_intent.action.arguments[0].mode,
            lab_language::OwnershipMode::Copy
        );
        assert_eq!(
            choice.source_intent.action.arguments[1].name,
            "dependencies"
        );
        assert_eq!(
            choice.source_intent.action.arguments[1].mode,
            lab_language::OwnershipMode::Take
        );
        assert!(matches!(
            choice.source_intent.action.results[0].lineage,
            lab_language::ResultLineage::Begins
        ));

        let candidate = &choice.candidates[0];
        let requirement = &candidate.tasks[0].requirements[0];
        let source_intent = choice.source_intent.clone();
        let solution = FacilityPlanningSolution {
            schema_version: FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION.to_owned(),
            problem_sha256: problem.sha256(),
            inventory_sha256: "a".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            policy: FacilityPlanningPolicy::default(),
            selections: vec![SelectedMethod {
                choice: choice.id.clone(),
                source_operation: choice.source_operation.clone(),
                source_intent: source_intent.clone(),
                method: candidate.method.clone(),
                tasks: vec![SelectedProcedureTask {
                    task: candidate.tasks[0].id.clone(),
                    materials: vec![],
                    requirements: vec![SelectedRequirementBinding {
                        requirement: requirement.id.clone(),
                        capability_kind: requirement.capability_kind.clone(),
                        minimum_qualification: requirement.minimum_qualification,
                        accepted_control_modes: requirement.accepted_control_modes.clone(),
                        offering: "https://example.org/offering/specimen-realization".to_owned(),
                        asset: "https://example.org/asset/operator".to_owned(),
                        observed_qualification: requirement.minimum_qualification.to_string(),
                        control_mode: ControlMode::Manual.to_string(),
                        parameters: vec![],
                        adapter: None,
                        rejected_candidates: vec![],
                    }],
                }],
            }],
        };
        let allocated = refined
            .allocate(&solution)
            .expect("the generic Intent allocates")
            .allocated_program()
            .expect("the allocated semantic program extracts");
        assert_eq!(allocated.methods[0].source_intent, source_intent);
    }

    fn generic_artifact_realization_method() -> MethodDefinition {
        let local = |name: &str| LocalId::new(name).unwrap();
        let parameter_types = [
            ("artifact_kind", ScalarType::Text),
            ("artifact_definition", ScalarType::Text),
            ("artifact_kind_definitions", ScalarType::Text),
            ("artifact_type_definition", ScalarType::Text),
            ("artifact_facets", ScalarType::Text),
            ("sbol_identity", ScalarType::Iri),
            ("label", ScalarType::Text),
            ("score", ScalarType::Integer),
            ("confidence", ScalarType::Integer),
            ("artifact_requirements", ScalarType::Text),
            ("artifact_acceptance", ScalarType::Text),
            ("artifact_design", ScalarType::Text),
        ];
        let task = local("realize");
        MethodDefinition {
            id: MethodId::new("https://example.org/method#generic-specimen-realization").unwrap(),
            refines: IntentOperationId::new("std.bio.build.realize").unwrap(),
            inputs: vec![MethodInput {
                name: local("design"),
                port_type: PortType::Design,
            }],
            parameters: parameter_types
                .iter()
                .map(|(name, scalar_type)| MethodParameter {
                    name: local(name),
                    source: None,
                    value_type: if matches!(*name, "artifact_kind_definitions" | "artifact_facets")
                    {
                        ParameterType::List {
                            element_type: *scalar_type,
                        }
                    } else {
                        ParameterType::Scalar {
                            scalar_type: *scalar_type,
                        }
                    },
                    default: None,
                })
                .collect(),
            tasks: vec![ProcedureTaskDefinition {
                id: task.clone(),
                operation: OperationId::new("https://example.org/procedure#RealizeSpecimen")
                    .unwrap(),
                inputs: vec![ValueReference::Input {
                    input: local("design"),
                }],
                outputs: vec![TaskOutput {
                    name: local("product"),
                    port_type: PortType::MaterialAsRequested,
                }],
                parameters: parameter_types
                    .iter()
                    .map(|(name, _)| ProcedureParameterDefinition {
                        id: local(name),
                        property_kind: PropertyKind::new(format!(
                            "https://example.org/property#{name}"
                        ))
                        .unwrap(),
                        value: ProcedureValueExpression::IntentParameter {
                            parameter: local(name),
                            unit: None,
                        },
                    })
                    .collect(),
                materials: vec![],
                execution: ProcedureTaskExecutionDefinition::Primitive {
                    requirements: vec![CapabilityRequirementDefinition {
                        id: local("realization"),
                        capability_kind: CapabilityKind::new(
                            "https://example.org/capability#SpecimenRealization",
                        )
                        .unwrap(),
                        minimum_qualification: QualificationLevel::Plannable,
                        accepted_control_modes: [ControlMode::Manual].into_iter().collect(),
                        constraints: vec![],
                    }],
                },
            }],
            outputs: vec![MethodOutput {
                name: local("product"),
                source: ValueReference::TaskOutput {
                    task,
                    output: local("product"),
                },
            }],
        }
    }

    #[test]
    fn a_catalog_template_lowers_a_complete_thermal_program_without_a_builder() {
        let provider = compile_module_in_environment(
            ModuleId::new("pkg.thermal"),
            r#"use std.bio.designs

action cycle <sample> for <cycles> cycles at <temperature> -> product:
  sample: take Material<Medium>
  cycles: Integer
  temperature: Quantity<C>
  product: Material<Medium> continues from sample
"#,
            &SemanticEnvironment::default(),
        )
        .expect("the package action checks");
        let mut environment = SemanticEnvironment::default();
        environment.insert("pkg.thermal", provider.interface.clone());
        let consumer = compile_module_in_environment(
            ModuleId::new("demo.thermal"),
            r#"use std.bio.designs
use std.bio.build
use pkg.thermal

build medium placeholder:
  components = [Ingredient { substance: "water", concentration: 1 g/L }]

workflow run() -> Material<Medium>:
  sample <- realize placeholder
  product <- cycle sample for 3 cycles at 60 C
  return product

workflow main() -> Material<Medium>:
  product <- run
  return product
"#,
            &environment,
        )
        .expect("the workflow checks against the package action");

        let document: MethodCatalogDocument = serde_json::from_value(serde_json::json!({
            "schema_version": "lab.method-catalog.v2",
            "methods": [{
                "id": "https://example.org/method#template-thermal-cycle",
                "refines": "pkg.thermal.cycle",
                "inputs": [{
                    "name": "sample",
                    "port_type": {
                        "kind": "material",
                        "state": "https://www.lab-compiler.org/ns/material-state#MediumProduct"
                    }
                }],
                "parameters": [
                    {"name": "cycles", "value_type": {"kind": "scalar", "scalar_type": "integer"}},
                    {"name": "temperature", "value_type": {"kind": "scalar", "scalar_type": "real"}}
                ],
                "tasks": [{
                    "id": "cycle",
                    "operation": "https://example.org/procedure#ThermalCycle",
                    "inputs": [{"kind": "input", "input": "sample"}],
                    "outputs": [{
                        "name": "product",
                        "port_type": {
                            "kind": "material",
                            "state": "https://www.lab-compiler.org/ns/material-state#MediumProduct"
                        }
                    }],
                    "parameters": [
                        {
                            "id": "cycles",
                            "property_kind": "https://example.org/procedure#Cycles",
                            "value": {"kind": "intent_parameter", "parameter": "cycles"}
                        },
                        {
                            "id": "temperature",
                            "property_kind": "https://example.org/procedure#Temperature",
                            "value": {"kind": "intent_parameter", "parameter": "temperature"}
                        }
                    ],
                    "execution": {
                        "kind": "template",
                        "contract": "https://www.lab-compiler.org/ns/procedure-contract#ThermalProgramV1",
                        "body": {
                            "load": {
                                "input": {"$lab": {"kind": "input", "index": 0}},
                                "outputs": [{"$lab": {"kind": "output", "id": "product"}}],
                                "sample_count": 1,
                                "volume_each": {
                                    "value": {"type": "integer", "value": "20"},
                                    "unit": "http://qudt.org/vocab/unit/MicroL"
                                }
                            },
                            "lid_temperature": {
                                "value": {"type": "integer", "value": "105"},
                                "unit": "http://qudt.org/vocab/unit/DEG_C"
                            },
                            "stages": [{
                                "id": "cycle",
                                "repeats": {"$lab": {"kind": "integer", "id": "cycles"}},
                                "steps": [{
                                    "id": "hold",
                                    "temperature": {"$lab": {"kind": "scalar", "id": "temperature"}},
                                    "hold": {
                                        "value": {"type": "integer", "value": "30"},
                                        "unit": "http://qudt.org/vocab/unit/SEC"
                                    }
                                }]
                            }],
                            "final_hold": {
                                "value": {"type": "integer", "value": "4"},
                                "unit": "http://qudt.org/vocab/unit/DEG_C"
                            }
                        },
                        "policy": {
                            "minimum_qualification": "https://sbol.io/ns/facility#Plannable",
                            "accepted_control_modes": [
                                "https://sbol.io/ns/facility#ReviewedFileControl"
                            ]
                        }
                    }
                }],
                "outputs": [{
                    "name": "product",
                    "source": {"kind": "task_output", "task": "cycle", "output": "product"}
                }]
            }]
        }))
        .expect("the template is ordinary Method catalog data");
        let mut methods = document.into_methods().unwrap();
        methods.push(
            crate::method::standard_method_definitions()
                .into_iter()
                .find(|method| {
                    method.id.as_str()
                        == "https://www.lab-compiler.org/ns/method#manual-artifact-realization"
                })
                .expect("the standard manual realization Method exists"),
        );
        let registry = MethodRegistry::new(methods).unwrap();
        let procedures = ProcedureCompiler::new(
            builtin_procedure_contracts().clone(),
            ProcedureProgramBuilderRegistry::default(),
        )
        .unwrap();
        assert!(procedures.builders().builders().next().is_none());

        let problem = PortableLairProgram::lower_entry_program(
            &[&provider, &consumer],
            consumer.module.as_str(),
        )
        .expect("generic Intent lowers")
        .refine_methods(&registry, &procedures)
        .expect("the template renders and contract-validates")
        .planning_problem()
        .expect("the rendered program projects");
        let task = &problem
            .choices
            .iter()
            .find(|choice| choice.source_operation.as_str() == "pkg.thermal.cycle")
            .expect("the custom action is a Method choice")
            .candidates[0]
            .tasks[0];
        let program = task
            .program
            .as_ref()
            .expect("the template produced a program");
        assert_eq!(program.body["load"]["input"], 0);
        assert_eq!(
            program.body["load"]["outputs"],
            serde_json::json!(["product"])
        );
        assert_eq!(program.body["stages"][0]["repeats"], 3);
        assert_eq!(
            program.body["stages"][0]["steps"][0]["temperature"]["unit"],
            "http://qudt.org/vocab/unit/DEG_C"
        );
        assert!(task.requirements.iter().any(|requirement| {
            requirement
                .capability_kind
                .as_str()
                .ends_with("#ProgrammedBlockTemperatureControl")
        }));
    }

    fn conditioning_method(name: &str) -> MethodDefinition {
        let local = |name: &str| LocalId::new(name).unwrap();
        let material = |state: &str| PortType::Material {
            state: AbsoluteIri::new(format!(
                "https://www.lab-compiler.org/ns/material-state#{state}"
            ))
            .unwrap(),
        };
        let data = PortType::Data {
            data_kind: AbsoluteIri::new("https://www.lab-compiler.org/ns/data-kind#Evidence")
                .unwrap(),
        };
        let task = local("condition");
        MethodDefinition {
            id: MethodId::new(format!("https://example.org/method#{name}")).unwrap(),
            refines: IntentOperationId::new("pkg.conditioning.condition").unwrap(),
            inputs: vec![MethodInput {
                name: local("sample"),
                port_type: material("MediumProduct"),
            }],
            parameters: [
                (
                    "label",
                    ParameterType::Scalar {
                        scalar_type: ScalarType::Text,
                    },
                ),
                (
                    "temperature",
                    ParameterType::Scalar {
                        scalar_type: ScalarType::Real,
                    },
                ),
                (
                    "cycles",
                    ParameterType::Scalar {
                        scalar_type: ScalarType::Integer,
                    },
                ),
                (
                    "flags",
                    ParameterType::List {
                        element_type: ScalarType::Text,
                    },
                ),
            ]
            .into_iter()
            .map(|(name, value_type)| MethodParameter {
                name: local(name),
                source: None,
                value_type,
                default: None,
            })
            .collect(),
            tasks: vec![ProcedureTaskDefinition {
                id: task.clone(),
                operation: OperationId::new("https://example.org/procedure#Condition").unwrap(),
                inputs: vec![ValueReference::Input {
                    input: local("sample"),
                }],
                outputs: vec![
                    TaskOutput {
                        name: local("conditioned"),
                        port_type: PortType::MaterialAsRequested,
                    },
                    TaskOutput {
                        name: local("evidence"),
                        port_type: data,
                    },
                ],
                parameters: vec![],
                materials: vec![],
                execution: ProcedureTaskExecutionDefinition::Primitive {
                    requirements: vec![CapabilityRequirementDefinition {
                        id: local("conditioning"),
                        capability_kind: CapabilityKind::new(
                            "https://example.org/capability#Conditioning",
                        )
                        .unwrap(),
                        minimum_qualification: QualificationLevel::Plannable,
                        accepted_control_modes: [ControlMode::Manual].into_iter().collect(),
                        constraints: vec![],
                    }],
                },
            }],
            outputs: vec![
                MethodOutput {
                    name: local("conditioned"),
                    source: ValueReference::TaskOutput {
                        task: task.clone(),
                        output: local("conditioned"),
                    },
                },
                MethodOutput {
                    name: local("evidence"),
                    source: ValueReference::TaskOutput {
                        task,
                        output: local("evidence"),
                    },
                },
            ],
        }
    }

    #[test]
    fn provisioning_yields_the_state_of_the_thing_fetched() {
        const SOURCE: &str = r#"use std.bio.designs
use std.bio.build
use std.lab.plasmid

buy chassis DH5alpha:
  competence = competent
  efficiency = 1e9 cfu/ug
buy antibiotic chloramphenicol:
  sbol_identity = "https://example.org/cam"

build plasmid p:
  sequence = dna("ACGTACGT")

workflow w() -> (
  product: Material<Plasmid>,
  cells: Material<Chassis is competent>,
  drug: Material<Antibiotic>,
):
  dependencies = []
  product <- realize p from dependencies
  cells <- provision DH5alpha
  drug <- provision chloramphenicol
  return product, cells, drug

workflow main() -> (
  product: Material<Plasmid>,
  cells: Material<Chassis is competent>,
  drug: Material<Antibiotic>,
):
  product, cells, drug <- w
  return product, cells, drug
"#;
        let module = lab_language::compile_module(SOURCE).expect("module checks");
        let ir = PortableLairProgram::lower_entry_program(&[&module], module.module.as_str())
            .expect("program lowers")
            .ir();

        assert!(
            ir.contains("material-state#competent"),
            "the checked in-state is preserved exactly: {ir}"
        );
        assert!(
            ir.contains("material-state#AntibioticProduct"),
            "an antibiotic is an antibiotic, not a tube of cells: {ir}"
        );
        assert_eq!(
            ir.matches("material-state#competent").count(),
            1,
            "only the chassis is competent cells: {ir}"
        );
    }

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

        let program = PortableLairProgram::lower_entry_program(
            &[&designs, &workflows],
            workflows.module.as_str(),
        )
        .expect("program lowers");
        let split = program.ir();

        // What comes off a shelf is what was asked for. Fetching the chassis
        // yields competent cells; fetching the antibiotic used to yield them
        // too, which was the IR believing an antibiotic was a tube of cells.
        assert!(
            split.contains(
                "workflow.material <\"https://www.lab-compiler.org/ns/material-state#competent\">"
            ),
            "a provisioned chassis retains its checked state: {split}"
        );
        assert!(
            !split.contains("#AntibioticStock"),
            "this program provisions no antibiotic, so no such state appears"
        );

        assert_eq!(split.matches(" = design.define ").count(), 2);
        assert!(split.contains("\\\"local\\\":\\\"gfp_sequence\\\""));
        assert!(split.contains("\\\"artifact\\\":\\\"plasmid\\\""));
        assert!(split.contains("\\\"artifact\\\":\\\"strain\\\""));
        assert!(!split.contains("design.dna_sequence"));
        assert!(!split.contains("design.plasmid"));

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
        let single =
            PortableLairProgram::lower_entry_program(&[&combined], combined.module.as_str())
                .expect("single module lowers")
                .ir();

        assert_ne!(split, single, "exact source-module provenance is retained");
        assert!(split.contains("\\\"module\\\":\\\"demo.workflows\\\""));
        assert!(single.contains("\\\"module\\\":\\\"standalone\\\""));
    }

    #[test]
    fn several_designs_share_one_named_sequence_value() {
        let checked = compile_module(SHARED_SEQUENCE_PROGRAM).expect("shared sequence checks");
        let ir = PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
            .expect("shared sequence lowers")
            .ir();

        assert_eq!(ir.matches(" = design.define ").count(), 2);
        assert!(!ir.contains("design.dna_sequence"));
        assert!(!ir.contains("design.plasmid"));
        assert!(ir.contains("\\\"local\\\":\\\"shared_sequence\\\""));
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
        let portable =
            PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
                .expect("generic realization lowers");
        let portable_ir = portable.ir();
        assert!(portable_ir.contains("workflow.perform"), "{portable_ir}");
        assert!(
            !portable_ir.contains("realize_restriction_enzyme"),
            "{portable_ir}"
        );

        let refined = portable
            .refine_methods(
                crate::method::standard_method_registry(),
                crate::procedure::builtin_procedure_compiler(),
            )
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

    /// Making competent cells through package-defined actions and explicitly registered Methods.
    ///
    /// Growing cells up to a target optical density, chilling them, spinning
    /// them into a pellet, and washing them into cold buffer are four verbs the
    /// compiler has no dedicated Intent operation classes for. They arrive from
    /// `std.lab.competence` as generic actions and refine through ordinary portable Methods.
    #[test]
    fn a_competent_cell_protocol_lowers_and_refines() {
        const SOURCE: &str = r#"use std.bio.designs
use std.bio.build
use std.lab.plasmid
use std.lab.competence

buy buffer cold_cacl2:
  concentration = 100 mM

build chassis DH5a_competent:
  heat_shock_temperature = 42 C

workflow prepare() -> Material<Chassis is competent>:
  cells <- realize DH5a_competent
  wash <- provision cold_cacl2
  culture <- grow cells at 37 C to 0.40 OD600
  chilled <- chill culture for 10 min
  pellet <- centrifuge chilled at 4000 rcf for 10 min
  competent <- resuspend pellet in wash
  return competent

workflow main() -> Material<Chassis is competent>:
  competent <- prepare
  return competent
"#;
        let module = lab_language::compile_module(SOURCE).expect("the protocol checks");
        let portable = PortableLairProgram::lower_entry_program(&[&module], module.module.as_str())
            .expect("the protocol lowers");
        let intent = portable.ir();
        assert_eq!(
            intent.matches("workflow.perform").count(),
            6,
            "realize, provision, grow, chill, centrifuge, and resuspend share one operation: {intent}"
        );

        let refined = portable
            .refine_methods(
                crate::method::standard_method_registry(),
                crate::procedure::builtin_procedure_compiler(),
            )
            .expect("every competence action has an explicit standard Method");
        let problem = refined.planning_problem().expect("the problem projects");
        for verb in [
            "std.lab.competence.grow",
            "std.lab.competence.chill",
            "std.lab.competence.centrifuge",
            "std.lab.competence.resuspend",
        ] {
            let choice = problem
                .choices
                .iter()
                .find(|choice| choice.source_operation.as_str() == verb)
                .unwrap_or_else(|| panic!("'{verb}' becomes a planning choice"));
            assert_eq!(
                choice.candidates.len(),
                1,
                "'{verb}' has one explicit Method"
            );
        }
    }

    /// A protocol is written once and run for many designs: a workflow that
    /// realizes one of its own parameters is a template, and each call site's
    /// argument decides which declared artifact its flow builds.
    #[test]
    fn a_workflow_realizing_its_parameter_serves_every_design_its_callers_pass() {
        const SOURCE: &str = r#"use std.bio.designs
use std.bio.build
use std.lab.plasmid
use std.lab.competence

buy buffer cold_cacl2:
  concentration = 100 mM

build chassis DH5alpha:
  heat_shock_temperature = 42 C

build chassis Top10:
  heat_shock_temperature = 42 C

workflow prepare_competent_cells(chassis: Chassis) -> Material<Chassis is competent>:
  cells <- realize chassis
  wash <- provision cold_cacl2
  culture <- grow cells at 37 C to 0.40 OD600
  chilled <- chill culture for 10 min
  pellet <- centrifuge chilled at 4000 rcf for 10 min
  ready <- resuspend pellet in wash
  return ready

workflow main() -> (
  a: Material<Chassis is competent>,
  b: Material<Chassis is competent>,
):
  a <- prepare_competent_cells DH5alpha
  b <- prepare_competent_cells Top10
  return a, b
"#;
        let module = lab_language::compile_module(SOURCE).expect("the program checks");
        let portable = PortableLairProgram::lower_entry_program(&[&module], module.module.as_str())
            .expect("both instantiations lower");
        let intent = portable.ir();
        assert_eq!(
            intent.matches(" = design.define ").count(),
            2,
            "each chassis keeps its own design: {intent}"
        );

        let problem = portable
            .refine_methods(
                crate::method::standard_method_registry(),
                crate::procedure::builtin_procedure_compiler(),
            )
            .expect("both instantiations refine")
            .planning_problem()
            .expect("the problem projects");
        let realizations = problem
            .choices
            .iter()
            .filter(|choice| choice.source_operation.as_str() == "std.bio.build.realize")
            .count();
        assert_eq!(realizations, 2, "one realization per design the calls pass");
        assert_eq!(
            problem
                .choices
                .iter()
                .filter(|choice| {
                    choice.source_operation.as_str() == "std.lab.competence.centrifuge"
                })
                .count(),
            2,
            "the one written protocol runs once per instantiation"
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
        let refined =
            PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
                .expect("portable LAIR lowers")
                .refine_methods(
                    crate::method::standard_method_registry(),
                    crate::procedure::builtin_procedure_compiler(),
                )
                .expect("standard methods refine");
        let ir = refined.ir();

        assert!(ir.contains("lair.stage") && ir.contains("refined-alternatives"));
        assert!(!ir.contains("workflow."), "{ir}");
        assert!(ir.contains("https://www.lab-compiler.org/ns/method#automated-golden-gate"));
        assert!(ir.contains("https://www.lab-compiler.org/ns/method#manual-artifact-realization"));
        assert!(ir.contains("procedure.parameter"));
        assert!(ir.contains("program: procedure.program <"), "{ir}");
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
        let error = PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
            .expect("portable LAIR lowers")
            .refine_methods(
                &crate::method::MethodRegistry::default(),
                crate::procedure::builtin_procedure_compiler(),
            )
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
        let ir = PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
            .expect("portable LAIR lowers")
            .refine_methods(
                crate::method::standard_method_registry(),
                crate::procedure::builtin_procedure_compiler(),
            )
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
        let refined =
            PortableLairProgram::lower_entry_program(&[&checked], checked.module.as_str())
                .expect("portable LAIR lowers")
                .refine_methods(
                    crate::method::standard_method_registry(),
                    crate::procedure::builtin_procedure_compiler(),
                )
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
            .validate(builtin_procedure_contracts())
            .expect("normalized program validates");
        let program = program
            .pipetting()
            .expect("Golden Gate setup must normalize to the pipetting contract");
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
            .validate(builtin_procedure_contracts())
            .expect("normalized thermal program validates");
        let thermal = thermal
            .thermal()
            .expect("Golden Gate cycling must normalize to the thermal contract");
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
            .validate(builtin_procedure_contracts())
            .expect("temperature-staged setup validates");
        let staged_program = staged_program
            .pipetting()
            .expect("temperature-staged setup must normalize to the pipetting contract");
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
            .validate(builtin_procedure_contracts())
            .expect("normalized serial-dilution program validates");
        let dilution_program = dilution_program
            .pipetting()
            .expect("serial dilution must normalize to the pipetting contract");
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
            .validate(builtin_procedure_contracts())
            .expect("recovery medium program validates");
        let add_medium = add_medium
            .pipetting()
            .expect("recovery medium addition must be pipetting");
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
            .validate(builtin_procedure_contracts())
            .expect("recovery incubation program validates");
        let incubation = incubation
            .thermal()
            .expect("recovery incubation must be thermal");
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
            .validate(builtin_procedure_contracts())
            .expect("transformation preparation validates");
        let preparation = preparation
            .pipetting()
            .expect("transformation preparation must be pipetting");
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
            .validate(builtin_procedure_contracts())
            .expect("heat shock validates");
        let heat_shock = heat_shock.thermal().expect("heat shock must be thermal");
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
            .validate(builtin_procedure_contracts())
            .expect("selective plating validates");
        let plate_program = plate_program
            .pipetting()
            .expect("selective plating must be pipetting");
        assert_eq!(plate_program.as_program().steps.len(), 4);
        assert_eq!(plate_program.as_program().vessels.len(), 3);
        assert_eq!(automated_plating.tasks[0].requirements.len(), 3);

        let json = serde_json::to_string_pretty(&problem).expect("problem serializes");
        let decoded: PlanningProblem = serde_json::from_str(&json).expect("problem deserializes");
        decoded
            .validate(builtin_procedure_contracts())
            .expect("decoded problem revalidates");
        assert_eq!(decoded, problem);
    }
}

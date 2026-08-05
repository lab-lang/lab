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

use crate::lair::dialect::design::DesignPlasmidOp;
use crate::lair::dialect::workflow::{
    DiluteOp, PlateOp, ProvisionOp, RealizeOp, RecoverOp, TransformOp,
};
use crate::lair::source_lowering::{SourceLoweringError, WorkflowActionIntent, lower_build_intent};
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
    /// Lower checked, backend-neutral frontend IR into verified Design and
    /// Workflow LAIR. Protocol selection consumes this type; neither selection
    /// nor a robot backend can accept the checked module directly.
    pub fn lower(module: &CheckedModule) -> Result<Self, PortableLairError> {
        let artifacts = lower_build_intent(module)?;
        let mut context = Context::new();
        let root = ModuleOp::new(
            &mut context,
            Identifier::try_from("lab_build").expect("static module name is valid"),
        );
        let mut designs = BTreeMap::new();
        for artifact in &artifacts {
            let operation = DesignPlasmidOp::new(
                &mut context,
                artifact.name.clone(),
                artifact.sequence.clone(),
                1,
                true,
                None,
                None,
            );
            designs.insert(artifact.name.clone(), operation.get_result_design(&context));
            root.append_operation(&mut context, operation.get_operation(), 0);
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
    artifact: crate::lair::source_lowering::BuildArtifactIntent,
) -> Result<(), PortableLairError> {
    let recipe = artifact.recipe;
    let design = designs[&artifact.name];
    let mut values = BTreeMap::new();

    for action in artifact.actions {
        match action {
            WorkflowActionIntent::Realize { product, construct } => {
                let operation = RealizeOp::new(
                    context,
                    design,
                    artifact.name.clone(),
                    recipe.backbone.clone(),
                    recipe.components.clone(),
                    artifact.dependencies.clone(),
                    recipe.restriction_enzyme.clone(),
                    recipe.assembly_replicates,
                );
                values.insert(product, operation.get_result_product(context));
                values.insert(construct, operation.get_result_construct(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Provision { cells, item } => {
                let operation = ProvisionOp::competent_cells(context, item);
                values.insert(cells, operation.get_result_material(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Transform {
                culture,
                construct,
                cells,
            } => {
                let operation = TransformOp::new(
                    context,
                    workflow_value(&values, &construct, &artifact.name)?,
                    workflow_value(&values, &cells, &artifact.name)?,
                    recipe.transformation_replicates,
                );
                values.insert(culture, operation.get_result_culture(context));
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
                    workflow_value(&values, &input, &artifact.name)?,
                    duration_magnitude,
                    duration_unit,
                );
                values.insert(culture, operation.get_result_recovered(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Dilute { culture, input } => {
                let operation = DiluteOp::new(
                    context,
                    workflow_value(&values, &input, &artifact.name)?,
                    recipe.serial_dilutions,
                );
                values.insert(culture, operation.get_result_diluted(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
            WorkflowActionIntent::Plate {
                plate,
                culture,
                selection,
            } => {
                let operation = PlateOp::new(
                    context,
                    workflow_value(&values, &culture, &artifact.name)?,
                    selection,
                    recipe.plating_replicates,
                );
                values.insert(plate, operation.get_result_plate(context));
                root.append_operation(context, operation.get_operation(), 0);
            }
        }
    }
    Ok(())
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

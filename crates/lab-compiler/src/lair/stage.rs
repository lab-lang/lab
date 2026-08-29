use std::fmt::{self, Display};
use std::str::FromStr;

use pliron::builtin::op_interfaces::{OneRegionInterface, SingleBlockRegionInterface};
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::Operation;

use crate::lair::dialect::meta::StageOp;

/// A verifier-valid boundary in the current Lab Compiler lowering pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrStage {
    /// Facility-independent biological artifact intent expressed only in Design IR.
    Design,
    /// Facility-independent Design and method-neutral Intent/Workflow material dataflow.
    DesignIntent,
    /// Method-selected Protocol IR plus the retained Design value it currently consumes.
    MethodSelectedProtocol,
}

impl Display for IrStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Design => "design",
            Self::DesignIntent => "design-intent",
            Self::MethodSelectedProtocol => "method-selected-protocol",
        })
    }
}

impl FromStr for IrStage {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "design" => Ok(Self::Design),
            "design-intent" | "design-workflow" => Ok(Self::DesignIntent),
            "method-selected-protocol" => Ok(Self::MethodSelectedProtocol),
            other => Err(format!(
                "unknown IR stage '{other}'; expected design, design-intent, or method-selected-protocol"
            )),
        }
    }
}

/// Structural contract for a named, verifier-valid Lab Compiler IR stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageContract {
    stage: IrStage,
}

impl StageContract {
    pub fn for_stage(stage: IrStage) -> Self {
        Self { stage }
    }

    pub fn stage(self) -> IrStage {
        self.stage
    }

    pub(crate) fn verify(self, actual: IrStage) -> Result<(), String> {
        if actual != self.stage {
            return Err(format!(
                "expected {} IR, but the module satisfies the {} stage",
                self.stage, actual
            ));
        }
        Ok(())
    }
}

pub(crate) fn detect_stage(context: &Context, module: ModuleOp) -> Result<IrStage, String> {
    let declared = declared_stage(context, module)?;
    let (design_operations, workflow_operations, protocol_operations) =
        operation_counts(context, module)?;
    let structural = match (design_operations, workflow_operations, protocol_operations) {
        (1.., 0, 0) => IrStage::Design,
        (1.., 1.., 0) => IrStage::DesignIntent,
        (1.., 0, 1..) => IrStage::MethodSelectedProtocol,
        (0, _, _) => {
            return Err("a Lab Compiler module must contain at least one design operation".into());
        }
        (_, 1.., 1..) => {
            return Err(
                "Workflow operations must be fully eliminated before the method-selected Protocol boundary"
                    .into(),
            );
        }
    };
    if declared != structural {
        return Err(format!(
            "lair.stage declares {declared}, but the module structurally satisfies {structural}"
        ));
    }
    Ok(declared)
}

pub(crate) fn initialize_stage(context: &mut Context, module: ModuleOp, stage: IrStage) {
    let marker = StageOp::new(context, stage.to_string());
    module.append_operation(context, marker.get_operation(), 0);
}

pub(crate) fn set_stage(
    context: &mut Context,
    module: ModuleOp,
    stage: IrStage,
) -> Result<(), String> {
    let marker = stage_markers(context, module)?
        .into_iter()
        .next()
        .expect("stage_markers rejects a missing marker");
    marker.set(context, stage.to_string());
    Ok(())
}

fn declared_stage(context: &Context, module: ModuleOp) -> Result<IrStage, String> {
    let marker = stage_markers(context, module)?
        .into_iter()
        .next()
        .expect("stage_markers rejects a missing marker");
    let value = marker
        .get_attr_stage(context)
        .expect("generic verification requires the stage attribute");
    value.as_str().parse()
}

fn stage_markers(context: &Context, module: ModuleOp) -> Result<Vec<StageOp>, String> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| "builtin.module has no entry block".to_owned())?;
    let operations = block.deref(context).iter(context).collect::<Vec<_>>();
    let markers = operations
        .iter()
        .filter_map(|operation| Operation::get_op::<StageOp>(*operation, context))
        .collect::<Vec<_>>();
    if markers.len() != 1 {
        return Err(format!(
            "a Lab Compiler module requires exactly one lair.stage marker, found {}",
            markers.len()
        ));
    }
    if Operation::get_op::<StageOp>(operations[0], context).is_none() {
        return Err("lair.stage must be the first operation in the module".to_owned());
    }
    Ok(markers)
}

fn operation_counts(context: &Context, module: ModuleOp) -> Result<(usize, usize, usize), String> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| "builtin.module has no entry block".to_owned())?;
    let mut design_operations = 0;
    let mut workflow_operations = 0;
    let mut protocol_operations = 0;

    for operation in block.deref(context).iter(context) {
        let op_id = Operation::get_opid(operation, context);
        match op_id.dialect.as_ref() {
            "lair" if Operation::get_op::<StageOp>(operation, context).is_some() => {}
            "design" => design_operations += 1,
            "workflow" => workflow_operations += 1,
            "protocol" => protocol_operations += 1,
            dialect => {
                return Err(format!(
                    "operation '{op_id}' belongs to dialect '{dialect}', which is not legal at a Lab Compiler stage boundary"
                ));
            }
        }
    }
    Ok((design_operations, workflow_operations, protocol_operations))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transitional_design_workflow_spelling_parses_as_design_intent() {
        assert_eq!(
            "design-workflow".parse::<IrStage>().unwrap(),
            IrStage::DesignIntent
        );
        assert_eq!(IrStage::DesignIntent.to_string(), "design-intent");
    }
}

use std::fmt::{self, Display};
use std::str::FromStr;

use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::operation::Operation;

/// A verifier-valid boundary in the current Lab Compiler lowering pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrStage {
    /// Facility-independent biological artifact intent expressed only in Design IR.
    Design,
    /// Facility-independent artifact intent plus explicit Workflow material dataflow.
    DesignWorkflow,
    /// Method-selected Protocol IR plus the retained Design value it currently consumes.
    MethodSelectedProtocol,
}

impl Display for IrStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Design => "design",
            Self::DesignWorkflow => "design-workflow",
            Self::MethodSelectedProtocol => "method-selected-protocol",
        })
    }
}

impl FromStr for IrStage {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "design" => Ok(Self::Design),
            "design-workflow" => Ok(Self::DesignWorkflow),
            "method-selected-protocol" => Ok(Self::MethodSelectedProtocol),
            other => Err(format!(
                "unknown IR stage '{other}'; expected design, design-workflow, or method-selected-protocol"
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
    let (design_operations, workflow_operations, protocol_operations) =
        operation_counts(context, module)?;
    match (design_operations, workflow_operations, protocol_operations) {
        (1.., 0, 0) => Ok(IrStage::Design),
        (1.., 1.., 0) => Ok(IrStage::DesignWorkflow),
        (1.., 0, 1..) => Ok(IrStage::MethodSelectedProtocol),
        (0, _, _) => Err("a Lab Compiler module must contain at least one design operation".into()),
        (_, 1.., 1..) => Err(
            "Workflow operations must be fully eliminated before the method-selected Protocol boundary"
                .into(),
        ),
    }
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

use crate::{AssemblyMethod, LabProfile};
use pliron::builtin::ops::ModuleOp;
use pliron::pass::{OpGuard, OpPass};

use crate::passes::{CheckMaterialLinearityPass, LowerDesignToProtocolPass};

pub(crate) fn build_design_to_protocol_pass(
    lab: &LabProfile,
    assembly: AssemblyMethod,
) -> OpPass<ModuleOp, LowerDesignToProtocolPass> {
    OpPass::<ModuleOp, _>::new(
        OpGuard::default(),
        LowerDesignToProtocolPass::new(lab.clone(), assembly),
    )
}

pub(crate) fn build_material_linearity_pass() -> OpPass<ModuleOp, CheckMaterialLinearityPass> {
    OpPass::<ModuleOp, _>::new(OpGuard::default(), CheckMaterialLinearityPass)
}

use pliron::builtin::ops::ModuleOp;
use pliron::pass::{OpGuard, OpPass};

use crate::lair::transform::CheckMaterialLinearityPass;

pub(crate) fn build_material_linearity_pass() -> OpPass<ModuleOp, CheckMaterialLinearityPass> {
    OpPass::<ModuleOp, _>::new(OpGuard::default(), CheckMaterialLinearityPass)
}

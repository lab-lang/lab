use pliron::builtin::op_interfaces::OneRegionInterface;
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::operation::Operation;

use crate::IrStage;

pub(crate) fn detect_stage(context: &Context, module: ModuleOp) -> Result<IrStage, String> {
    let (design_operations, protocol_operations) = operation_counts(context, module)?;
    match (design_operations, protocol_operations) {
        (1, 0) => Ok(IrStage::Design),
        (1, 1..) => Ok(IrStage::TargetSelectedProtocol),
        (0, _) => Err("a Lab Compiler module must contain exactly one design operation".into()),
        (count, _) => Err(format!(
            "a Lab Compiler module must contain exactly one design operation, found {count}"
        )),
    }
}

fn operation_counts(context: &Context, module: ModuleOp) -> Result<(usize, usize), String> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| "builtin.module has no entry block".to_owned())?;
    let mut design_operations = 0;
    let mut protocol_operations = 0;

    for operation in block.deref(context).iter(context) {
        let op_id = Operation::get_opid(operation, context);
        match op_id.dialect.as_ref() {
            "design" => design_operations += 1,
            "protocol" => protocol_operations += 1,
            dialect => {
                return Err(format!(
                    "operation '{op_id}' belongs to dialect '{dialect}', which is not legal at a Lab Compiler stage boundary"
                ));
            }
        }
    }
    Ok((design_operations, protocol_operations))
}

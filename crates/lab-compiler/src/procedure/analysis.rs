use pliron::context::{Context, Ptr};
use pliron::graph::walkers::{
    IRNode, WALKCONFIG_PREORDER_FORWARD, uninterruptible::immutable::walk_op,
};
use pliron::operation::Operation;
use pliron::pass::{Analysis, AnalysisManager};
use pliron::result::Result;
use pliron::r#type::Typed;
use pliron::value::Value;
use pliron::verify_err;

use crate::procedure::ir::MaterialType as ProcedureMaterialType;

/// A whole-IR analysis of the affine physical-resource rule.
///
/// This is deliberately not an operation verifier: it follows SSA use lists,
/// while operation verifiers are restricted to operation-local invariants.
pub(crate) struct MaterialLinearityAnalysis;

impl Analysis for MaterialLinearityAnalysis {
    fn name(&self) -> &str {
        "material-linearity"
    }

    fn compute(
        root: Ptr<Operation>,
        context: &Context,
        _analyses: &mut AnalysisManager,
    ) -> Result<Self> {
        let mut material_values = Vec::new();
        walk_op(
            context,
            &mut material_values,
            &WALKCONFIG_PREORDER_FORWARD,
            root,
            collect_material_values,
        );

        for value in material_values {
            let uses = value.uses(context).len();
            if uses > 1 {
                return verify_err!(
                    value.loc(context),
                    "physical material value has {uses} consumers; use an explicit split or sample operation"
                );
            }
        }
        Ok(Self)
    }
}

fn collect_material_values(ctx: &Context, values: &mut Vec<Value>, node: IRNode) {
    let candidates: Vec<_> = match node {
        IRNode::Operation(operation) => operation.deref(ctx).results().collect(),
        IRNode::BasicBlock(block) => block.deref(ctx).arguments().collect(),
        IRNode::Region(_) => return,
    };
    for value in candidates {
        let handle = value.get_type(ctx);
        if handle
            .deref(ctx)
            .downcast_ref::<ProcedureMaterialType>()
            .is_some()
        {
            values.push(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use pliron::builtin::op_interfaces::OneRegionInterface;
    use pliron::builtin::ops::ModuleOp;
    use pliron::context::Context;
    use pliron::identifier::Identifier;
    use pliron::irbuild::inserter::{IRInserter, Inserter};
    use pliron::irbuild::listener::DummyListener;
    use pliron::linked_list::ContainsLinkedList;
    use pliron::op::Op;

    use crate::method::LocalId;
    use lab_capability::{MethodId, OperationId};
    use pliron::builtin::attributes::StringAttr;

    use crate::design::ir::{DesignDnaSequenceOp, DesignPlasmidOp};
    use crate::method::ir::{ChoiceOp, ChoicePorts, YieldOp};
    use crate::procedure::ir::{MaterialType, TaskOp};

    use crate::procedure::analysis::*;

    #[test]
    fn rejects_two_consumers_of_one_physical_value() {
        let ctx = &mut Context::new();
        let module = ModuleOp::new(ctx, Identifier::try_from("test").unwrap());
        let block = module.get_region(ctx).deref(ctx).get_head().unwrap();
        let mut inserter = IRInserter::<DummyListener>::new_at_block_end(block);

        let sequence = DesignDnaSequenceOp::new(ctx, "p_test_sequence", "ACGT");
        let sequence_value = sequence.get_result_sequence(ctx);
        inserter.append_op(ctx, &sequence);
        let design = DesignPlasmidOp::new(ctx, "p_test", sequence_value, 1, true, None, None);
        let design_value = design.get_result_design(ctx);
        inserter.append_op(ctx, &design);
        let material_type = MaterialType::get(
            ctx,
            StringAttr::new("https://example.org/material/sample".to_owned()),
        )
        .into();
        let produce = TaskOp::new(
            ctx,
            "produce",
            &OperationId::new("https://example.org/procedure/produce").unwrap(),
            vec![design_value],
            vec![material_type],
            &[LocalId::new("sample").unwrap()],
        );
        let sample = produce.get_operation().deref(ctx).get_result(0);
        inserter.append_op(ctx, &produce);
        for node in ["consume-first", "consume-second"] {
            let consume = TaskOp::new(
                ctx,
                node,
                &OperationId::new("https://example.org/procedure/consume").unwrap(),
                vec![sample],
                vec![],
                &[],
            );
            inserter.append_op(ctx, &consume);
        }

        assert!(
            MaterialLinearityAnalysis::compute(
                module.get_operation(),
                ctx,
                &mut AnalysisManager::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_two_consumers_of_a_material_method_argument() {
        let ctx = &mut Context::new();
        let module = ModuleOp::new(ctx, Identifier::try_from("test_block_argument").unwrap());
        let block = module.get_region(ctx).deref(ctx).get_head().unwrap();
        let mut inserter = IRInserter::<DummyListener>::new_at_block_end(block);

        let sequence = DesignDnaSequenceOp::new(ctx, "sequence", "ACGT");
        let sequence_value = sequence.get_result_sequence(ctx);
        inserter.append_op(ctx, &sequence);
        let design = DesignPlasmidOp::new(ctx, "design", sequence_value, 1, true, None, None);
        let design_value = design.get_result_design(ctx);
        inserter.append_op(ctx, &design);
        let material_type = MaterialType::get(
            ctx,
            StringAttr::new("https://example.org/material/sample".to_owned()),
        )
        .into();
        let produce = TaskOp::new(
            ctx,
            "produce",
            &OperationId::new("https://example.org/procedure/produce").unwrap(),
            vec![design_value],
            vec![material_type],
            &[LocalId::new("sample").unwrap()],
        );
        let sample = produce.get_operation().deref(ctx).get_result(0);
        inserter.append_op(ctx, &produce);

        let choice = ChoiceOp::new(
            ctx,
            "consume",
            "example.consume",
            &[MethodId::new("https://example.org/method/consume").unwrap()],
            ChoicePorts {
                inputs: vec![(LocalId::new("sample").unwrap(), sample)],
                outputs: vec![],
            },
            None,
            &[],
        );
        let candidate = choice
            .candidate_region(ctx, 0)
            .deref(ctx)
            .get_head()
            .unwrap();
        let argument = candidate.deref(ctx).get_argument(0);
        for node in ["consume-first", "consume-second"] {
            let consume = TaskOp::new(
                ctx,
                node,
                &OperationId::new("https://example.org/procedure/consume").unwrap(),
                vec![argument],
                vec![],
                &[],
            );
            choice.append_candidate_operation(ctx, 0, consume.get_operation());
        }
        let r#yield = YieldOp::new(ctx, vec![]);
        choice.append_candidate_operation(ctx, 0, r#yield.get_operation());
        inserter.append_op(ctx, &choice);

        assert!(
            MaterialLinearityAnalysis::compute(
                module.get_operation(),
                ctx,
                &mut AnalysisManager::default(),
            )
            .is_err()
        );
    }
}

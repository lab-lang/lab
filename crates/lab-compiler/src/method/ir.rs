//! Facility-independent method alternatives and candidate-region termination.

// Construction APIs are consumed by Method refinement.

use std::collections::BTreeSet;

use crate::method::{IntentOperationId, LocalId};
use lab_capability::MethodId;
use pliron::basic_block::BasicBlock;
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::{
    IsTerminatorInterface, IsolatedFromAboveInterface, NResultsInterface,
    SingleBlockRegionInterface,
};
use pliron::common_traits::Verify;
use pliron::context::{Context, Ptr};
use pliron::derive::pliron_op;
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::region::Region;
use pliron::result::Result;
use pliron::r#type::{TypeHandle, Typed};
use pliron::value::Value;
use pliron::verify_err;

use crate::ir::attributes::string_vec;
use crate::procedure::ir::is_stable_local_id;

/// One refinable source action with one single-block region per candidate method.
#[pliron_op(
    name = "method.choice",
    format,
    attributes = (
        choice_id: StringAttr,
        source_operation: StringAttr,
        candidates: VecAttr,
        choice_input_names: VecAttr,
        choice_output_names: VecAttr,
        choice_artifact: StringAttr,
        choice_dependencies: VecAttr
    ),
    interfaces = [IsolatedFromAboveInterface, SingleBlockRegionInterface]
)]
pub(crate) struct ChoiceOp;

pub(crate) struct ChoicePorts {
    pub inputs: Vec<(LocalId, Value)>,
    pub outputs: Vec<(LocalId, TypeHandle)>,
}

impl ChoiceOp {
    pub(crate) fn new(
        context: &mut Context,
        choice_id: impl Into<String>,
        source_operation: impl Into<String>,
        candidates: &[MethodId],
        ports: ChoicePorts,
        artifact: Option<&str>,
        dependencies: &[String],
    ) -> Self {
        let (input_names, operands): (Vec<_>, Vec<_>) = ports.inputs.into_iter().unzip();
        let (output_names, result_types): (Vec<_>, Vec<_>) = ports.outputs.into_iter().unzip();
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            result_types,
            operands,
            vec![],
            candidates.len(),
        );
        let result = Self { op: raw };
        result.set_attr_choice_id(context, StringAttr::new(choice_id.into()));
        result.set_attr_source_operation(context, StringAttr::new(source_operation.into()));
        result.set_attr_candidates(
            context,
            string_vec(candidates.iter().map(ToString::to_string).collect()),
        );
        result.set_attr_choice_input_names(
            context,
            string_vec(input_names.iter().map(ToString::to_string).collect()),
        );
        result.set_attr_choice_output_names(
            context,
            string_vec(output_names.iter().map(ToString::to_string).collect()),
        );
        result.set_attr_choice_artifact(
            context,
            StringAttr::new(artifact.unwrap_or_default().to_owned()),
        );
        result.set_attr_choice_dependencies(context, string_vec(dependencies.to_vec()));
        let argument_types = result
            .get_operation()
            .deref(context)
            .operands()
            .map(|operand| operand.get_type(context))
            .collect::<Vec<_>>();
        for index in 0..candidates.len() {
            let block = BasicBlock::new(context, None, argument_types.clone());
            block.insert_at_front(result.candidate_region(context, index), context);
        }
        result
    }

    pub(crate) fn candidate_region(&self, context: &Context, index: usize) -> Ptr<Region> {
        self.get_operation().deref(context).get_region(index)
    }

    pub(crate) fn choice_id(&self, context: &Context) -> String {
        self.get_attr_choice_id(context)
            .expect("verified method.choice carries choice_id")
            .as_str()
            .to_owned()
    }

    pub(crate) fn semantic_choice_id(&self, context: &Context) -> LocalId {
        LocalId::new(self.choice_id(context)).expect("verified method.choice carries a stable ID")
    }

    pub(crate) fn source_operation(&self, context: &Context) -> IntentOperationId {
        IntentOperationId::new(
            self.get_attr_source_operation(context)
                .expect("verified method.choice carries source_operation")
                .as_str(),
        )
        .expect("verified method.choice carries a stable source operation")
    }

    pub(crate) fn candidate_ids(&self, context: &Context) -> Vec<MethodId> {
        self.get_attr_candidates(context)
            .expect("verified method.choice carries candidates")
            .0
            .iter()
            .map(|candidate| {
                MethodId::new(
                    candidate
                        .downcast_ref::<StringAttr>()
                        .expect("verified method.choice candidates are strings")
                        .as_str(),
                )
                .expect("verified method.choice candidates are absolute IRIs")
            })
            .collect()
    }

    pub(crate) fn input_names(&self, context: &Context) -> Vec<LocalId> {
        local_ids(
            &self
                .get_attr_choice_input_names(context)
                .expect("verified method.choice carries input names"),
        )
    }

    pub(crate) fn output_names(&self, context: &Context) -> Vec<LocalId> {
        local_ids(
            &self
                .get_attr_choice_output_names(context)
                .expect("verified method.choice carries output names"),
        )
    }

    pub(crate) fn artifact_name(&self, context: &Context) -> Option<String> {
        self.get_attr_choice_artifact(context)
            .map(|artifact| artifact.as_str().to_owned())
            .filter(|artifact| !artifact.is_empty())
    }

    pub(crate) fn dependency_artifacts(&self, context: &Context) -> Vec<String> {
        self.get_attr_choice_dependencies(context)
            .expect("verified method.choice carries choice_dependencies")
            .0
            .iter()
            .map(|dependency| {
                dependency
                    .downcast_ref::<StringAttr>()
                    .expect("verified choice_dependencies are strings")
                    .as_str()
                    .to_owned()
            })
            .collect()
    }

    pub(crate) fn append_candidate_operation(
        &self,
        context: &mut Context,
        candidate: usize,
        operation: Ptr<Operation>,
    ) {
        self.append_operation(context, operation, candidate);
    }
}

fn local_ids(values: &VecAttr) -> Vec<LocalId> {
    values
        .0
        .iter()
        .map(|value| {
            LocalId::new(
                value
                    .downcast_ref::<StringAttr>()
                    .expect("verified local IDs are strings")
                    .as_str(),
            )
            .expect("verified local IDs contain no whitespace")
        })
        .collect()
}

impl Verify for ChoiceOp {
    fn verify(&self, context: &Context) -> Result<()> {
        if self
            .get_attr_choice_id(context)
            .is_none_or(|value| !is_stable_local_id(value.as_str()))
        {
            return verify_err!(
                self.loc(context),
                "method.choice choice_id must be non-empty and contain no whitespace"
            );
        }
        if self
            .get_attr_source_operation(context)
            .is_none_or(|value| !is_stable_local_id(value.as_str()))
        {
            return verify_err!(
                self.loc(context),
                "method.choice source_operation must be non-empty and contain no whitespace"
            );
        }
        let Some(candidates) = self.get_attr_candidates(context) else {
            return verify_err!(self.loc(context), "method.choice is missing candidates");
        };
        if candidates.0.is_empty() {
            return verify_err!(self.loc(context), "method.choice requires a candidate");
        }
        if candidates.0.len() != self.get_operation().deref(context).num_regions() {
            return verify_err!(
                self.loc(context),
                "method.choice candidate identities and regions must have the same length"
            );
        }
        self.verify_port_names(context, true)?;
        self.verify_port_names(context, false)?;
        if self.get_attr_choice_artifact(context).is_none() {
            return verify_err!(
                self.loc(context),
                "method.choice is missing choice_artifact"
            );
        }
        let Some(dependencies) = self.get_attr_choice_dependencies(context) else {
            return verify_err!(
                self.loc(context),
                "method.choice is missing choice_dependencies"
            );
        };
        let mut dependency_names = BTreeSet::new();
        for dependency in &dependencies.0 {
            let Some(dependency) = dependency.downcast_ref::<StringAttr>() else {
                return verify_err!(
                    self.loc(context),
                    "method.choice choice_dependencies must contain only strings"
                );
            };
            if dependency.as_str().is_empty() || !dependency_names.insert(dependency.as_str()) {
                return verify_err!(
                    self.loc(context),
                    "method.choice choice_dependencies must be non-empty and unique"
                );
            }
        }
        let mut seen = BTreeSet::new();
        for candidate in &candidates.0 {
            let Some(candidate) = candidate.downcast_ref::<StringAttr>() else {
                return verify_err!(
                    self.loc(context),
                    "method.choice candidates must contain only strings"
                );
            };
            if MethodId::new(candidate.as_str()).is_err() {
                return verify_err!(
                    self.loc(context),
                    "method.choice candidate identities must be absolute IRIs"
                );
            }
            if !seen.insert(candidate.as_str()) {
                return verify_err!(
                    self.loc(context),
                    "method.choice candidate identities must be unique"
                );
            }
        }
        for index in 0..candidates.0.len() {
            self.verify_yield(context, index)?;
        }
        Ok(())
    }
}

impl ChoiceOp {
    fn verify_port_names(&self, context: &Context, inputs: bool) -> Result<()> {
        let (kind, names, expected) = if inputs {
            (
                "input",
                self.get_attr_choice_input_names(context),
                self.get_operation().deref(context).get_num_operands(),
            )
        } else {
            (
                "output",
                self.get_attr_choice_output_names(context),
                self.get_operation().deref(context).get_num_results(),
            )
        };
        let Some(names) = names else {
            return verify_err!(self.loc(context), "method.choice is missing {kind} names");
        };
        if names.0.len() != expected {
            return verify_err!(
                self.loc(context),
                "method.choice {kind} names must match its {kind} arity"
            );
        }
        let mut seen = BTreeSet::new();
        for name in &names.0 {
            let Some(name) = name.downcast_ref::<StringAttr>() else {
                return verify_err!(
                    self.loc(context),
                    "method.choice {kind} names must contain only strings"
                );
            };
            if !is_stable_local_id(name.as_str()) {
                return verify_err!(
                    self.loc(context),
                    "method.choice {kind} names must be non-empty and contain no whitespace"
                );
            }
            if !seen.insert(name.as_str()) {
                return verify_err!(
                    self.loc(context),
                    "method.choice {kind} names must be unique"
                );
            }
        }
        Ok(())
    }

    fn verify_yield(&self, context: &Context, candidate: usize) -> Result<()> {
        let Some(block) = self
            .candidate_region(context, candidate)
            .deref(context)
            .get_head()
        else {
            return verify_err!(
                self.loc(context),
                "method.choice candidate region has no block"
            );
        };
        let Some(tail) = block.deref(context).get_tail() else {
            return verify_err!(self.loc(context), "method.choice candidate region is empty");
        };
        let choice = self.get_operation().deref(context);
        let body = block.deref(context);
        if body.get_num_arguments() != choice.get_num_operands() {
            return verify_err!(
                self.loc(context),
                "method.choice candidate entry arguments must match its operand arity"
            );
        }
        for index in 0..choice.get_num_operands() {
            if body.get_argument(index).get_type(context)
                != choice.get_operand(index).get_type(context)
            {
                return verify_err!(
                    self.loc(context),
                    "method.choice candidate entry argument types must match its operand types"
                );
            }
        }
        let Some(yield_op) = Operation::get_op::<YieldOp>(tail, context) else {
            return verify_err!(
                self.loc(context),
                "method.choice candidate region must terminate with method.yield"
            );
        };
        let yielded = yield_op.get_operation().deref(context);
        if yielded.get_num_operands() != choice.get_num_results() {
            return verify_err!(
                self.loc(context),
                "method.yield arity must match method.choice results"
            );
        }
        for index in 0..choice.get_num_results() {
            if yielded.get_operand(index).get_type(context)
                != choice.get_result(index).get_type(context)
            {
                return verify_err!(
                    self.loc(context),
                    "method.yield operand types must match method.choice result types"
                );
            }
        }
        Ok(())
    }
}

/// Terminates one candidate region with values compatible across every alternative.
#[pliron_op(
    name = "method.yield",
    format,
    interfaces = [IsTerminatorInterface, NResultsInterface<0>],
    verifier = "succ"
)]
pub(crate) struct YieldOp;

impl YieldOp {
    pub(crate) fn new(context: &mut Context, values: Vec<Value>) -> Self {
        Self {
            op: Operation::new(
                context,
                Self::get_concrete_op_info(),
                vec![],
                values,
                vec![],
                0,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use lab_capability::MethodId;
    use pliron::context::Context;
    use pliron::linked_list::ContainsLinkedList;
    use pliron::op::Op;
    use pliron::operation::verify_operation;
    use pliron::r#type::Typed;

    use crate::design::ir::DesignDnaSequenceOp;
    use crate::method::LocalId;

    use super::{ChoiceOp, ChoicePorts, YieldOp};

    #[test]
    fn aliased_choice_inputs_receive_distinct_candidate_arguments() {
        let context = &mut Context::new();
        let sequence = DesignDnaSequenceOp::new(context, "sequence", "ACGT");
        let input = sequence.get_result_sequence(context);
        let choice = ChoiceOp::new(
            context,
            "aliased-inputs",
            "example.alias",
            &[MethodId::new("https://example.org/method/alias").unwrap()],
            ChoicePorts {
                inputs: vec![
                    (LocalId::new("first").unwrap(), input),
                    (LocalId::new("second").unwrap(), input),
                ],
                outputs: vec![],
            },
            None,
            &[],
        );
        let block = choice
            .candidate_region(context, 0)
            .deref(context)
            .get_head()
            .unwrap();
        let arguments = block.deref(context).arguments().collect::<Vec<_>>();
        assert_eq!(arguments.len(), 2);
        assert_ne!(arguments[0], arguments[1]);
        assert_eq!(arguments[0].get_type(context), input.get_type(context));
        assert_eq!(arguments[1].get_type(context), input.get_type(context));

        let r#yield = YieldOp::new(context, vec![]);
        choice.append_candidate_operation(context, 0, r#yield.get_operation());
        verify_operation(choice.get_operation(), context).unwrap();
    }
}

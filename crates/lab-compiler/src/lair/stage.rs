use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::str::FromStr;

use pliron::builtin::op_interfaces::{OneRegionInterface, SingleBlockRegionInterface};
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::Operation;

use crate::lair::dialect::capability::{ConstraintOp, RequirementOp};
use crate::lair::dialect::meta::StageOp;
use crate::lair::dialect::method::{ChoiceOp, YieldOp};
use crate::lair::dialect::procedure::TaskOp;

/// A verifier-valid boundary in the current Lab Compiler lowering pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrStage {
    /// Facility-independent biological artifact intent expressed only in Design IR.
    Design,
    /// Facility-independent Design and method-neutral Intent/Workflow material dataflow.
    DesignIntent,
    /// Facility-independent Method candidates containing Procedure and Capability operations.
    RefinedAlternatives,
    /// Method-selected Protocol IR plus the retained Design value it currently consumes.
    MethodSelectedProtocol,
}

impl Display for IrStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Design => "design",
            Self::DesignIntent => "design-intent",
            Self::RefinedAlternatives => "refined-alternatives",
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
            "refined-alternatives" => Ok(Self::RefinedAlternatives),
            "method-selected-protocol" => Ok(Self::MethodSelectedProtocol),
            other => Err(format!(
                "unknown IR stage '{other}'; expected design, design-intent, refined-alternatives, or method-selected-protocol"
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
    if declared == IrStage::RefinedAlternatives {
        verify_refined_alternatives(context, module)?;
        return Ok(declared);
    }
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

fn verify_refined_alternatives(context: &Context, module: ModuleOp) -> Result<(), String> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| "builtin.module has no entry block".to_owned())?;
    let mut design_operations = 0;
    let mut choices = Vec::new();
    for operation in block.deref(context).iter(context) {
        let id = Operation::get_opid(operation, context);
        match id.dialect.as_ref() {
            "lair" if Operation::get_op::<StageOp>(operation, context).is_some() => {}
            "design" => design_operations += 1,
            "method" if Operation::get_op::<ChoiceOp>(operation, context).is_some() => {
                choices.push(Operation::get_op::<ChoiceOp>(operation, context).unwrap());
            }
            _ => {
                return Err(format!(
                    "operation '{id}' is not legal at the refined-alternatives module boundary"
                ));
            }
        }
    }
    if design_operations == 0 {
        return Err("refined-alternatives requires at least one Design operation".to_owned());
    }
    if choices.is_empty() {
        return Err("refined-alternatives requires at least one method.choice".to_owned());
    }

    let mut choice_ids = BTreeSet::new();
    let mut procedure_nodes = BTreeSet::new();
    let mut requirement_ids = BTreeSet::new();
    for choice in choices {
        let choice_id = choice.choice_id(context);
        if !choice_ids.insert(choice_id.clone()) {
            return Err(format!("duplicate method choice identity '{choice_id}'"));
        }
        let region_count = choice.get_operation().deref(context).num_regions();
        for candidate in 0..region_count {
            verify_candidate(
                context,
                &choice,
                candidate,
                &mut procedure_nodes,
                &mut requirement_ids,
            )?;
        }
    }
    Ok(())
}

fn verify_candidate(
    context: &Context,
    choice: &ChoiceOp,
    candidate: usize,
    global_procedure_nodes: &mut BTreeSet<String>,
    global_requirement_ids: &mut BTreeSet<String>,
) -> Result<(), String> {
    let choice_id = choice.choice_id(context);
    let block = choice
        .candidate_region(context, candidate)
        .deref(context)
        .get_head()
        .expect("generic verification requires one block per method candidate");
    let tail = block
        .deref(context)
        .get_tail()
        .expect("method.choice verification rejects an empty candidate");
    let mut tasks = BTreeSet::new();
    let mut requirements = BTreeMap::new();
    let mut constraints = Vec::new();

    for operation in block.deref(context).iter(context) {
        if let Some(task) = Operation::get_op::<TaskOp>(operation, context) {
            let node = task.node_id(context);
            if !tasks.insert(node.clone()) {
                return Err(format!(
                    "method choice '{choice_id}' candidate {candidate} repeats Procedure node '{node}'"
                ));
            }
            if !global_procedure_nodes.insert(node.clone()) {
                return Err(format!("duplicate Procedure node identity '{node}'"));
            }
            continue;
        }
        if let Some(requirement) = Operation::get_op::<RequirementOp>(operation, context) {
            let id = requirement.requirement_id(context);
            let node = requirement.procedure_node(context);
            if requirements.insert(id.clone(), node).is_some() {
                return Err(format!(
                    "method choice '{choice_id}' candidate {candidate} repeats Requirement '{id}'"
                ));
            }
            if !global_requirement_ids.insert(id.clone()) {
                return Err(format!("duplicate Requirement identity '{id}'"));
            }
            continue;
        }
        if let Some(constraint) = Operation::get_op::<ConstraintOp>(operation, context) {
            constraints.push(constraint.requirement_id(context));
            continue;
        }
        if Operation::get_op::<YieldOp>(operation, context).is_some() {
            if operation != tail {
                return Err(format!(
                    "method choice '{choice_id}' candidate {candidate} contains method.yield before its end"
                ));
            }
            continue;
        }
        return Err(format!(
            "operation '{}' is not legal inside method choice '{choice_id}' candidate {candidate}",
            Operation::get_opid(operation, context)
        ));
    }

    if tasks.is_empty() {
        return Err(format!(
            "method choice '{choice_id}' candidate {candidate} contains no Procedure task"
        ));
    }
    for (requirement, node) in &requirements {
        if !tasks.contains(node) {
            return Err(format!(
                "Requirement '{requirement}' references Procedure node '{node}' outside its candidate"
            ));
        }
    }
    for node in &tasks {
        if !requirements
            .values()
            .any(|required_node| required_node == node)
        {
            return Err(format!(
                "Procedure node '{node}' has no Capability requirement in its candidate"
            ));
        }
    }
    for requirement in constraints {
        if !requirements.contains_key(&requirement) {
            return Err(format!(
                "Capability constraint references Requirement '{requirement}' outside its candidate"
            ));
        }
    }
    Ok(())
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
    use lab_capability::{
        CapabilityKind, ConstraintRelation, ControlMode, ExactInteger, MethodId, OperationId,
        PropertyConstraint, PropertyKind, PropertyValue, QualificationLevel, ScalarValue, UnitIri,
    };
    use pliron::builtin::attributes::StringAttr;
    use pliron::builtin::ops::ModuleOp;
    use pliron::identifier::Identifier;
    use pliron::operation::verify_operation;
    use pliron::printable::Printable;

    use crate::lair::dialect::capability::{ConstraintOp, RequirementOp};
    use crate::lair::dialect::design::DesignDnaSequenceOp;
    use crate::lair::dialect::method::{ChoiceOp, YieldOp};
    use crate::lair::dialect::procedure::{MaterialType, TaskOp};
    use crate::lair::session::CompilerSession;

    use super::*;

    #[test]
    fn the_transitional_design_workflow_spelling_parses_as_design_intent() {
        assert_eq!(
            "design-workflow".parse::<IrStage>().unwrap(),
            IrStage::DesignIntent
        );
        assert_eq!(IrStage::DesignIntent.to_string(), "design-intent");
    }

    #[test]
    fn refined_alternatives_round_trip_with_typed_capability_constraints() {
        let (context, module) = refined_program(false);

        verify_operation(module.get_operation(), &context).unwrap();
        assert_eq!(
            detect_stage(&context, module).unwrap(),
            IrStage::RefinedAlternatives
        );
        let ir = module.get_operation().disp(&context).to_string();
        assert!(ir.contains("method.choice"), "{ir}");
        assert!(ir.contains("procedure.task"), "{ir}");
        assert!(ir.contains("capability.requirement"), "{ir}");
        assert!(ir.contains("capability.constraint"), "{ir}");
        assert!(ir.contains("9007199254740993"), "{ir}");

        let mut reparsed = CompilerSession::default();
        reparsed.parse_ir(&ir).unwrap();
        reparsed.verify_stage(IrStage::RefinedAlternatives).unwrap();
    }

    #[test]
    fn refined_alternatives_require_every_procedure_task_to_have_a_requirement() {
        let (context, module) = refined_program(true);

        verify_operation(module.get_operation(), &context).unwrap();
        let error = detect_stage(&context, module).unwrap_err();
        assert!(
            error.contains(
                "Procedure node 'incubation::candidate-0::uncovered' has no Capability requirement"
            ),
            "{error}"
        );
    }

    fn refined_program(uncovered_task: bool) -> (Context, ModuleOp) {
        let mut context = Context::new();
        let module = ModuleOp::new(&mut context, Identifier::try_from("refined_demo").unwrap());
        initialize_stage(&mut context, module, IrStage::RefinedAlternatives);
        let design = DesignDnaSequenceOp::new(&mut context, "sample_sequence", "ACGT");
        module.append_operation(&mut context, design.get_operation(), 0);

        let candidates = [
            MethodId::new("https://example.org/method/ambient-incubation").unwrap(),
            MethodId::new("https://example.org/method/controlled-incubation").unwrap(),
        ];
        let material_type = MaterialType::get(
            &context,
            StringAttr::new("https://example.org/material/incubated-sample".to_owned()),
        )
        .into();
        let choice = ChoiceOp::new(
            &mut context,
            "incubation",
            "std.lab.incubate",
            vec![],
            vec![material_type],
            &candidates,
        );
        let operation = OperationId::new("https://example.org/procedure/incubate").unwrap();
        let capability = CapabilityKind::new("https://sbol.io/ns/capability#Incubation").unwrap();

        for candidate in 0..candidates.len() {
            let node = format!("incubation::candidate-{candidate}::incubate");
            let requirement = format!("incubation::candidate-{candidate}::temperature-control");
            let task = TaskOp::new(&mut context, &node, &operation, vec![], vec![material_type]);
            let result = task.get_operation().deref(&context).get_result(0);
            choice.append_candidate_operation(&mut context, candidate, task.get_operation());
            let required = RequirementOp::new(
                &mut context,
                &requirement,
                &node,
                &capability,
                QualificationLevel::Plannable,
                [ControlMode::ReviewedFile, ControlMode::Api],
            );
            choice.append_candidate_operation(&mut context, candidate, required.get_operation());
            if candidate == 1 {
                let constraint = PropertyConstraint {
                    property_kind: PropertyKind::new("https://sbol.io/ns/capability#Temperature")
                        .unwrap(),
                    relation: ConstraintRelation::AtLeast,
                    required: PropertyValue::new(
                        ScalarValue::Integer(ExactInteger::parse("9007199254740993").unwrap()),
                        Some(UnitIri::new("http://qudt.org/vocab/unit/DEG_C").unwrap()),
                    )
                    .unwrap(),
                };
                let constraint = ConstraintOp::new(&mut context, &requirement, &constraint);
                choice.append_candidate_operation(
                    &mut context,
                    candidate,
                    constraint.get_operation(),
                );
            }
            if uncovered_task && candidate == 0 {
                let uncovered = TaskOp::new(
                    &mut context,
                    "incubation::candidate-0::uncovered",
                    &operation,
                    vec![],
                    vec![],
                );
                choice.append_candidate_operation(
                    &mut context,
                    candidate,
                    uncovered.get_operation(),
                );
            }
            let r#yield = YieldOp::new(&mut context, vec![result]);
            choice.append_candidate_operation(&mut context, candidate, r#yield.get_operation());
        }
        module.append_operation(&mut context, choice.get_operation(), 0);
        (context, module)
    }
}

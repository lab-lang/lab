use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::str::FromStr;

use pliron::builtin::op_interfaces::{OneRegionInterface, SingleBlockRegionInterface};
use pliron::builtin::ops::ModuleOp;
use pliron::context::Context;
use pliron::linked_list::ContainsLinkedList;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::value::{DefiningEntity, Value};

use crate::lair::dialect::allocation::{
    BindingOp, ContextOp as AllocationContextOp, MaterialBindingOp, MethodOp,
};
use crate::lair::dialect::capability::{ConstraintOp, RequirementOp};
use crate::lair::dialect::meta::StageOp;
use crate::method::ir::{ChoiceOp, YieldOp};
use crate::procedure::ir::{MaterialInputOp, ParameterOp, TaskOp};

/// A verifier-valid boundary in the current Lab Compiler lowering pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrStage {
    /// Facility-independent biological artifact intent expressed only in Design IR.
    Design,
    /// Facility-independent Design and method-neutral Intent/Workflow material dataflow.
    DesignIntent,
    /// Facility-independent Method candidates containing Procedure and Capability operations.
    RefinedAlternatives,
    /// One selected Procedure graph with exact facility and adapter decisions.
    AllocatedProcedure,
}

impl Display for IrStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Design => "design",
            Self::DesignIntent => "design-intent",
            Self::RefinedAlternatives => "refined-alternatives",
            Self::AllocatedProcedure => "allocated-procedure",
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
            "allocated-procedure" => Ok(Self::AllocatedProcedure),
            other => Err(format!(
                "unknown IR stage '{other}'; expected design, design-intent, refined-alternatives, or allocated-procedure"
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
    if declared == IrStage::AllocatedProcedure {
        verify_allocated_procedure(context, module)?;
        return Ok(declared);
    }
    let (design_operations, workflow_operations) = operation_counts(context, module)?;
    let structural = match (design_operations, workflow_operations) {
        (1.., 0) => IrStage::Design,
        (1.., 1..) => IrStage::DesignIntent,
        (0, _) => {
            return Err("a Lab Compiler module must contain at least one design operation".into());
        }
    };
    if declared != structural {
        return Err(format!(
            "lair.stage declares {declared}, but the module structurally satisfies {structural}"
        ));
    }
    Ok(declared)
}

fn verify_allocated_procedure(context: &Context, module: ModuleOp) -> Result<(), String> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| "builtin.module has no entry block".to_owned())?;
    let mut allocation_contexts = 0;
    let mut methods = Vec::new();
    let mut task_operations = Vec::new();
    let mut tasks = BTreeSet::new();
    let mut requirements = BTreeMap::new();
    let mut parameters = Vec::new();
    let mut materials = BTreeMap::new();
    let mut constraints = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut material_bindings = BTreeMap::new();
    let mut design_operations = 0;

    for operation in block.deref(context).iter(context) {
        let id = Operation::get_opid(operation, context);
        if Operation::get_op::<StageOp>(operation, context).is_some() {
            continue;
        }
        if id.dialect.as_ref() == "design" {
            design_operations += 1;
            continue;
        }
        if Operation::get_op::<AllocationContextOp>(operation, context).is_some() {
            allocation_contexts += 1;
            continue;
        }
        if let Some(method) = Operation::get_op::<MethodOp>(operation, context) {
            methods.push(method);
            continue;
        }
        if let Some(task) = Operation::get_op::<TaskOp>(operation, context) {
            let node = task.node_id(context);
            if !tasks.insert(node.clone()) {
                return Err(format!("duplicate Procedure node identity '{node}'"));
            }
            task_operations.push(operation);
            continue;
        }
        if let Some(requirement) = Operation::get_op::<RequirementOp>(operation, context) {
            let requirement_id = requirement.requirement_id(context);
            if requirements
                .insert(requirement_id.clone(), requirement)
                .is_some()
            {
                return Err(format!("duplicate Requirement identity '{requirement_id}'"));
            }
            continue;
        }
        if let Some(parameter) = Operation::get_op::<ParameterOp>(operation, context) {
            parameters.push((
                parameter.parameter_id(context),
                parameter.procedure_node(context),
            ));
            continue;
        }
        if let Some(material) = Operation::get_op::<MaterialInputOp>(operation, context) {
            let input = material.input_id(context);
            if materials.insert(input.clone(), material).is_some() {
                return Err(format!(
                    "duplicate Procedure material input identity '{input}'"
                ));
            }
            continue;
        }
        if let Some(constraint) = Operation::get_op::<ConstraintOp>(operation, context) {
            constraints.push(constraint.requirement_id(context));
            continue;
        }
        if let Some(binding) = Operation::get_op::<BindingOp>(operation, context) {
            let requirement = binding.requirement(context);
            if bindings.insert(requirement.clone(), binding).is_some() {
                return Err(format!(
                    "Requirement '{requirement}' has more than one allocation.binding"
                ));
            }
            continue;
        }
        if let Some(binding) = Operation::get_op::<MaterialBindingOp>(operation, context) {
            let input = binding.input(context);
            if material_bindings.insert(input.clone(), binding).is_some() {
                return Err(format!(
                    "Procedure material input '{input}' has more than one allocation.material"
                ));
            }
            continue;
        }
        return Err(format!(
            "operation '{id}' is not legal at the allocated-procedure module boundary"
        ));
    }
    if design_operations == 0 {
        return Err("allocated-procedure requires at least one Design operation".to_owned());
    }
    if allocation_contexts != 1 {
        return Err(format!(
            "allocated-procedure requires exactly one allocation.context, found {allocation_contexts}"
        ));
    }
    if methods.is_empty() || tasks.is_empty() {
        return Err("allocated-procedure requires selected Methods and Procedure tasks".to_owned());
    }

    let mut choices = BTreeSet::new();
    let mut selected_tasks = BTreeSet::new();
    for method in methods {
        let choice = method.choice(context);
        if !choices.insert(choice.clone()) {
            return Err(format!("duplicate selected Method choice '{choice}'"));
        }
        for task in method.procedure_nodes(context) {
            if !selected_tasks.insert(task.clone()) {
                return Err(format!(
                    "Procedure node '{task}' belongs to more than one selected Method"
                ));
            }
        }
    }
    let semantic_tasks = tasks
        .iter()
        .map(|task| crate::method::LocalId::new(task).expect("verified task IDs are stable"))
        .collect::<BTreeSet<_>>();
    if selected_tasks != semantic_tasks {
        return Err(
            "selected Methods must name every allocated Procedure node exactly once".to_owned(),
        );
    }
    for (parameter, node) in parameters {
        if !tasks.contains(&node) {
            return Err(format!(
                "Procedure parameter '{parameter}' references absent node '{node}'"
            ));
        }
    }
    if material_bindings.len() != materials.len() {
        return Err(
            "every allocated Procedure material input must have exactly one binding".to_owned(),
        );
    }
    for (input, material) in &materials {
        let stable_id =
            crate::method::LocalId::new(input).expect("verified material input IDs are stable");
        let Some(binding) = material_bindings.get(&stable_id) else {
            return Err(format!(
                "allocated Procedure material input '{input}' has no binding"
            ));
        };
        if binding.procedure_node(context).as_str() != material.procedure_node(context)
            || binding.symbol(context) != material.symbol(context)
        {
            return Err(format!(
                "allocation for Procedure material input '{input}' does not match its node and symbol"
            ));
        }
        if !tasks.contains(&material.procedure_node(context)) {
            return Err(format!(
                "Procedure material input '{input}' references absent node '{}'",
                material.procedure_node(context)
            ));
        }
    }
    for requirement in requirements.values() {
        let node = requirement.procedure_node(context);
        if !tasks.contains(&node) {
            return Err(format!(
                "Requirement '{}' references absent Procedure node '{node}'",
                requirement.requirement_id(context)
            ));
        }
    }
    for task in &tasks {
        if !requirements
            .values()
            .any(|requirement| requirement.procedure_node(context) == *task)
        {
            return Err(format!(
                "allocated Procedure node '{task}' has no Capability requirement"
            ));
        }
    }
    for requirement in constraints {
        if !requirements.contains_key(&requirement) {
            return Err(format!(
                "Capability constraint references absent Requirement '{requirement}'"
            ));
        }
    }
    if bindings.len() != requirements.len() {
        return Err("every allocated Requirement must have exactly one binding".to_owned());
    }
    for (requirement_id, requirement) in &requirements {
        let stable_id = crate::method::LocalId::new(requirement_id)
            .expect("verified Requirement IDs are stable");
        let Some(binding) = bindings.get(&stable_id) else {
            return Err(format!(
                "allocated Requirement '{requirement_id}' has no binding"
            ));
        };
        if binding.procedure_node(context).as_str() != requirement.procedure_node(context) {
            return Err(format!(
                "allocation for Requirement '{requirement_id}' names the wrong Procedure node"
            ));
        }
        let observed_qualification = lab_capability::QualificationLevel::try_from(
            binding
                .get_attr_observed_qualification(context)
                .expect("verified binding carries qualification")
                .as_str(),
        )
        .expect("binding verifier accepts only closed qualifications");
        if !requirement
            .semantic_minimum_qualification(context)
            .is_satisfied_by(observed_qualification)
        {
            return Err(format!(
                "allocation for Requirement '{requirement_id}' has insufficient qualification"
            ));
        }
        let observed_control_mode = lab_capability::ControlMode::try_from(
            binding
                .get_attr_observed_control_mode(context)
                .expect("verified binding carries control mode")
                .as_str(),
        )
        .expect("binding verifier accepts only closed control modes");
        if !requirement
            .semantic_control_modes(context)
            .contains(&observed_control_mode)
        {
            return Err(format!(
                "allocation for Requirement '{requirement_id}' uses an unaccepted control mode"
            ));
        }
    }
    verify_allocated_dataflow(context, &task_operations)
}

fn verify_allocated_dataflow(
    context: &Context,
    tasks: &[pliron::context::Ptr<Operation>],
) -> Result<(), String> {
    for (consumer_index, task) in tasks.iter().enumerate() {
        let task_id = Operation::get_op::<TaskOp>(*task, context)
            .expect("allocated task list contains Procedure tasks")
            .node_id(context);
        for operand in task.deref(context).operands() {
            let DefiningEntity::Op(definition) = operand.defining_entity() else {
                return Err(format!(
                    "allocated Procedure node '{task_id}' cannot consume a block argument"
                ));
            };
            let definition_id = Operation::get_opid(definition, context);
            if definition_id.dialect.as_ref() == "design" {
                continue;
            }
            let Some(definition_index) =
                tasks.iter().position(|candidate| *candidate == definition)
            else {
                return Err(format!(
                    "allocated Procedure node '{task_id}' consumes a value outside Design or Procedure LAIR"
                ));
            };
            if definition_index >= consumer_index {
                return Err(format!(
                    "allocated Procedure node '{task_id}' uses a value before its task defines it"
                ));
            }
        }
    }
    Ok(())
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
    let mut parameter_ids = BTreeSet::new();
    let mut material_input_ids = BTreeSet::new();
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
                &mut parameter_ids,
                &mut material_input_ids,
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
    global_parameter_ids: &mut BTreeSet<String>,
    global_material_input_ids: &mut BTreeSet<String>,
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
    let mut parameters = Vec::new();
    let mut material_inputs = Vec::new();
    let mut task_operations = Vec::new();

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
            task_operations.push(operation);
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
        if let Some(parameter) = Operation::get_op::<ParameterOp>(operation, context) {
            let id = parameter.parameter_id(context);
            if !global_parameter_ids.insert(id.clone()) {
                return Err(format!("duplicate Procedure parameter identity '{id}'"));
            }
            parameters.push((id, parameter.procedure_node(context)));
            continue;
        }
        if let Some(material) = Operation::get_op::<MaterialInputOp>(operation, context) {
            let id = material.input_id(context);
            if !global_material_input_ids.insert(id.clone()) {
                return Err(format!(
                    "duplicate Procedure material input identity '{id}'"
                ));
            }
            material_inputs.push((id, material.procedure_node(context)));
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
    for (parameter, node) in parameters {
        if !tasks.contains(&node) {
            return Err(format!(
                "Procedure parameter '{parameter}' references Procedure node '{node}' outside its candidate"
            ));
        }
    }
    for (material, node) in material_inputs {
        if !tasks.contains(&node) {
            return Err(format!(
                "Procedure material input '{material}' references Procedure node '{node}' outside its candidate"
            ));
        }
    }
    verify_candidate_dataflow(context, choice, candidate, &task_operations, tail)?;
    Ok(())
}

fn verify_candidate_dataflow(
    context: &Context,
    choice: &ChoiceOp,
    candidate: usize,
    tasks: &[pliron::context::Ptr<Operation>],
    yield_operation: pliron::context::Ptr<Operation>,
) -> Result<(), String> {
    let choice_id = choice.choice_id(context);
    let external = choice
        .get_operation()
        .deref(context)
        .operands()
        .collect::<Vec<_>>();
    for (task_index, task) in tasks.iter().enumerate() {
        for operand in task.deref(context).operands() {
            verify_candidate_value(
                context,
                operand,
                &external,
                tasks,
                Some(task_index),
            )
            .map_err(|error| {
                format!(
                    "method choice '{choice_id}' candidate {candidate} has invalid Procedure dataflow: {error}"
                )
            })?;
        }
    }
    for yielded in yield_operation.deref(context).operands() {
        verify_candidate_value(context, yielded, &external, tasks, None).map_err(|error| {
            format!(
                "method choice '{choice_id}' candidate {candidate} has invalid yield dataflow: {error}"
            )
        })?;
    }
    Ok(())
}

fn verify_candidate_value(
    _context: &Context,
    value: Value,
    external: &[Value],
    tasks: &[pliron::context::Ptr<Operation>],
    consumer_index: Option<usize>,
) -> Result<(), String> {
    if external.contains(&value) {
        return Ok(());
    }
    let DefiningEntity::Op(defining_operation) = value.defining_entity() else {
        return Err("a block value is not declared as a method.choice operand".to_owned());
    };
    let Some(definition_index) = tasks
        .iter()
        .position(|operation| *operation == defining_operation)
    else {
        return Err(
            "a value is defined outside this candidate and is not a method.choice operand"
                .to_owned(),
        );
    };
    if consumer_index.is_some_and(|consumer_index| definition_index >= consumer_index) {
        return Err("a Procedure task uses a value before its task defines it".to_owned());
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

fn operation_counts(context: &Context, module: ModuleOp) -> Result<(usize, usize), String> {
    let block = module
        .get_region(context)
        .deref(context)
        .get_head()
        .ok_or_else(|| "builtin.module has no entry block".to_owned())?;
    let mut design_operations = 0;
    let mut workflow_operations = 0;

    for operation in block.deref(context).iter(context) {
        let op_id = Operation::get_opid(operation, context);
        match op_id.dialect.as_ref() {
            "lair" if Operation::get_op::<StageOp>(operation, context).is_some() => {}
            "design" => design_operations += 1,
            "workflow" => workflow_operations += 1,
            dialect => {
                return Err(format!(
                    "operation '{op_id}' belongs to dialect '{dialect}', which is not legal at a Lab Compiler stage boundary"
                ));
            }
        }
    }
    Ok((design_operations, workflow_operations))
}

#[cfg(test)]
mod tests {
    use crate::method::ProcedureValue;
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
    use crate::lair::session::CompilerSession;
    use crate::method::ir::{ChoiceOp, ChoicePorts, YieldOp};
    use crate::procedure::ir::{MaterialInputOp, MaterialType, ParameterOp, TaskOp};

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
        assert!(ir.contains("choice_output_names"), "{ir}");
        assert!(ir.contains("procedure.task"), "{ir}");
        assert!(ir.contains("procedure.material_input"), "{ir}");
        assert!(ir.contains("task_output_names"), "{ir}");
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

    #[test]
    fn refined_alternatives_reject_values_crossing_candidate_regions() {
        let (context, module) = refined_program(false);
        let module_block = module
            .get_region(&context)
            .deref(&context)
            .get_head()
            .unwrap();
        let choice = module_block
            .deref(&context)
            .iter(&context)
            .find_map(|operation| Operation::get_op::<ChoiceOp>(operation, &context))
            .unwrap();
        let first_block = choice
            .candidate_region(&context, 0)
            .deref(&context)
            .get_head()
            .unwrap();
        let first_result = first_block
            .deref(&context)
            .iter(&context)
            .find_map(|operation| Operation::get_op::<TaskOp>(operation, &context))
            .unwrap()
            .get_operation()
            .deref(&context)
            .get_result(0);
        let second_yield = choice
            .candidate_region(&context, 1)
            .deref(&context)
            .get_head()
            .unwrap()
            .deref(&context)
            .get_tail()
            .unwrap();
        Operation::replace_operand(second_yield, &context, 0, first_result);

        let error = detect_stage(&context, module).unwrap_err();
        assert!(
            error.contains("value is defined outside this candidate"),
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
            &candidates,
            ChoicePorts {
                inputs: vec![],
                outputs: vec![(
                    crate::method::LocalId::new("sample").unwrap(),
                    material_type,
                )],
            },
            None,
            &[],
        );
        let operation = OperationId::new("https://example.org/procedure/incubate").unwrap();
        let capability = CapabilityKind::new("https://sbol.io/ns/capability#Incubation").unwrap();

        for candidate in 0..candidates.len() {
            let node = format!("incubation::candidate-{candidate}::incubate");
            let requirement = format!("incubation::candidate-{candidate}::temperature-control");
            let task = TaskOp::new(
                &mut context,
                &node,
                &operation,
                vec![],
                vec![material_type],
                &[crate::method::LocalId::new("sample").unwrap()],
            );
            let result = task.get_operation().deref(&context).get_result(0);
            choice.append_candidate_operation(&mut context, candidate, task.get_operation());
            let material = MaterialInputOp::new(
                &mut context,
                format!("{node}::material::medium"),
                &node,
                "recovery_medium",
            );
            choice.append_candidate_operation(&mut context, candidate, material.get_operation());
            let parameter = ParameterOp::new(
                &mut context,
                format!("{node}::parameter::cycles"),
                &node,
                &PropertyKind::new("https://sbol.io/ns/capability#CycleCount").unwrap(),
                &ProcedureValue::Scalar {
                    value: PropertyValue::unitless(ScalarValue::Integer(
                        ExactInteger::parse("30").unwrap(),
                    )),
                },
            );
            choice.append_candidate_operation(&mut context, candidate, parameter.get_operation());
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
                    &[],
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

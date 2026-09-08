//! Target-neutral workflow intent.
//!
//! One open operation represents every durable laboratory action. Its checked
//! invocation remains the source of truth for declaration identity, exact typed
//! values, ownership, result lineage, and source provenance; Methods own every
//! refinement from that intent into executable Procedure tasks.

use std::collections::{BTreeMap, BTreeSet};

use lab_capability::AbsoluteIri;
#[cfg(test)]
use lab_language::TypedExpression;
use lab_language::{
    CheckedActionArgument, CheckedActionResult, CheckedExpression, CheckedField, CheckedType,
    DefinitionId, OwnershipMode, ResolvedAction, ResolvedActionCallee, ResultLineage,
};
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::{AtLeastNOpdsInterface, AtLeastNResultsInterface};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::{pliron_op, pliron_type};
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::r#type::{TypeHandle, Typed};
use pliron::value::Value;
use pliron::{verify_err, verify_err_noloc};
use serde::{Deserialize, Serialize};

use crate::design::ArtifactDesign;
use crate::design::ir::DesignType;
use crate::ir::attributes::{require_string, string_vec};
use crate::method::{PortType, ProcedureValue};

/// Namespace for material states named by checked action result types.
pub const STATE_NS: &str = "https://www.lab-compiler.org/ns/material-state#";

/// Namespace for non-physical data kinds named by checked action result types.
pub const DATA_NS: &str = "https://www.lab-compiler.org/ns/data-kind#";

/// One abstract material state visible in a source-level workflow.
#[pliron_type(
    name = "workflow.material",
    generate_get = true,
    format = "`<` $state `>`"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MaterialType {
    state: StringAttr,
}

impl MaterialType {
    pub fn state(ctx: &Context, name: &str) -> TypeHandle {
        Self::get(ctx, StringAttr::new(format!("{STATE_NS}{name}"))).into()
    }

    pub fn iri(&self) -> &str {
        self.state.as_str()
    }
}

impl Verify for MaterialType {
    fn verify(&self, _context: &Context) -> Result<()> {
        if AbsoluteIri::new(self.state.as_str()).is_err() {
            return verify_err_noloc!("workflow.material state must be an absolute IRI");
        }
        Ok(())
    }
}

/// One non-physical information or evidence kind in source-level workflow dataflow.
#[pliron_type(name = "workflow.data", generate_get = true, format = "`<` $kind `>`")]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DataType {
    kind: StringAttr,
}

impl DataType {
    pub fn kind(ctx: &Context, name: &str) -> TypeHandle {
        Self::get(ctx, StringAttr::new(format!("{DATA_NS}{name}"))).into()
    }

    pub fn iri(&self) -> &str {
        self.kind.as_str()
    }
}

impl Verify for DataType {
    fn verify(&self, _context: &Context) -> Result<()> {
        if AbsoluteIri::new(self.kind.as_str()).is_err() {
            return verify_err_noloc!("workflow.data kind must be an absolute IRI");
        }
        Ok(())
    }
}

/// Stable source coordinates for one checked action invocation.
///
/// Checked modules intentionally contain no source text. A module and exact
/// workflow definition plus a statement-index path identify the source
/// construct without making later passes reinterpret syntax.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentSource {
    pub module: String,
    pub workflow: DefinitionId,
    pub statement_path: Vec<usize>,
}

/// The complete compiler boundary for one durable source action.
///
/// `action` preserves exact checked types and expressions, ownership modes,
/// declaration identity, operation identity, and declared result lineage.
/// `result_bindings` preserves the names chosen at this call site. `parameters`
/// contains exact scalar/list semantic values available to Methods, including
/// design facts projected independently of the action operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentAction {
    pub source: IntentSource,
    pub action: ResolvedAction,
    /// Action argument names whose values are carried by Workflow SSA rather
    /// than by scalar/list Method parameters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ssa_operands: Vec<String>,
    pub result_bindings: Vec<CheckedField>,
    /// Complete checked artifact declaration whose realization workflow owns
    /// this action. This remains present after Method refinement so no later
    /// stage has to reconstruct design semantics from convenience parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactDesign>,
    /// Artifact identities consumed by the realization that owns this action.
    ///
    /// This is structural workflow context, not an ad hoc Method parameter.
    /// Keeping it beside `artifact` lets refinement preserve dependency edges
    /// without agreeing on magic parameter names with source lowering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_dependencies: Vec<String>,
    pub parameters: BTreeMap<String, ProcedureValue>,
}

impl IntentAction {
    pub fn operation(&self) -> Option<&str> {
        self.action.operation()
    }

    /// Validate the durable source semantics retained across compiler stages.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.source.module.is_empty()
            || self.source.workflow.module.as_str().is_empty()
            || self.source.workflow.local.is_empty()
            || self.source.statement_path.is_empty()
        {
            return Err("Intent source coordinates must be complete".to_owned());
        }
        if self.source.module != self.source.workflow.module.as_str() {
            return Err("Intent source module must match its workflow definition".to_owned());
        }
        let ResolvedActionCallee::Action {
            definition,
            operation,
        } = &self.action.callee
        else {
            return Err("Intent action must resolve to a stable operation".to_owned());
        };
        if definition.module.as_str().is_empty()
            || definition.local.is_empty()
            || !is_stable_name(operation)
        {
            return Err("Intent action declaration identity must be complete".to_owned());
        }
        validate_named_values(
            "Intent action argument",
            self.action
                .arguments
                .iter()
                .map(|argument| argument.name.as_str()),
        )?;
        validate_named_values(
            "Intent action result",
            self.action
                .results
                .iter()
                .map(|result| result.name.as_str()),
        )?;
        validate_named_values(
            "Intent SSA operand",
            self.ssa_operands.iter().map(String::as_str),
        )?;
        validate_named_values(
            "Intent result binding",
            self.result_bindings
                .iter()
                .map(|binding| binding.name.as_str()),
        )?;
        if self.result_bindings.len() != self.action.results.len()
            || self
                .result_bindings
                .iter()
                .zip(&self.action.results)
                .any(|(binding, result)| binding.r#type != result.r#type)
        {
            return Err("Intent result bindings must match the retained action results".to_owned());
        }
        let argument_names = self
            .action
            .arguments
            .iter()
            .map(|argument| argument.name.as_str())
            .collect::<BTreeSet<_>>();
        if self
            .ssa_operands
            .iter()
            .any(|name| !argument_names.contains(name.as_str()))
        {
            return Err("Intent SSA operands must name retained action arguments".to_owned());
        }
        for result in &self.action.results {
            let lineage = match &result.lineage {
                ResultLineage::Begins => continue,
                ResultLineage::Continues { from } => from,
                ResultLineage::IdentifiedBy { operands } => operands,
            };
            if lineage.is_empty()
                || lineage
                    .iter()
                    .any(|name| !argument_names.contains(name.as_str()))
                || lineage.iter().collect::<BTreeSet<_>>().len() != lineage.len()
            {
                return Err(format!(
                    "Intent action result '{}' has invalid operand lineage",
                    result.name
                ));
            }
        }
        if let Some(artifact) = &self.artifact {
            artifact.validate()?;
        }
        if self
            .artifact_dependencies
            .iter()
            .any(|dependency| dependency.is_empty())
        {
            return Err("Intent artifact dependencies cannot be empty".to_owned());
        }
        let unique = self.artifact_dependencies.iter().collect::<BTreeSet<_>>();
        if unique.len() != self.artifact_dependencies.len() {
            return Err("Intent artifact dependencies must be unique".to_owned());
        }
        Ok(())
    }

    /// Bind durable checked action semantics to the named, typed ports exposed
    /// by a later compiler stage.
    pub(crate) fn validate_ports<'a>(
        &self,
        inputs: impl IntoIterator<Item = (&'a str, &'a PortType)>,
        outputs: impl IntoIterator<Item = (&'a str, &'a PortType)>,
    ) -> std::result::Result<(), String> {
        let inputs = inputs.into_iter().collect::<Vec<_>>();
        let outputs = outputs.into_iter().collect::<Vec<_>>();
        if inputs.len() != self.ssa_operands.len() {
            return Err(format!(
                "Intent exposes {} input ports but declares {} SSA operands",
                inputs.len(),
                self.ssa_operands.len()
            ));
        }
        let mut seen = BTreeSet::new();
        for ((name, port_type), expected) in inputs.into_iter().zip(&self.ssa_operands) {
            if !seen.insert(name) {
                return Err(format!("Intent input port '{name}' is repeated"));
            }
            if name != expected {
                return Err(format!(
                    "Intent input port '{name}' does not match declared SSA operand '{expected}'"
                ));
            }
            let Some(argument) = self
                .action
                .arguments
                .iter()
                .find(|argument| argument.name == name)
            else {
                return Err(format!(
                    "Intent input port '{name}' is not a retained action argument"
                ));
            };
            if !checked_input_matches_port(argument, port_type) {
                return Err(format!(
                    "Intent input port '{name}' does not match its retained checked type"
                ));
            }
        }
        if outputs.len() != self.action.results.len() {
            return Err(format!(
                "Intent exposes {} output ports but its retained action declares {} results",
                outputs.len(),
                self.action.results.len()
            ));
        }
        for ((name, port_type), result) in outputs.into_iter().zip(&self.action.results) {
            if name != result.name {
                return Err(format!(
                    "Intent output port '{name}' does not match retained action result '{}'",
                    result.name
                ));
            }
            if !checked_result_matches_port(result, port_type) {
                return Err(format!(
                    "Intent output port '{name}' does not match its retained checked type"
                ));
            }
        }
        Ok(())
    }
}

fn validate_named_values<'a>(
    owner: &str,
    names: impl IntoIterator<Item = &'a str>,
) -> std::result::Result<(), String> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !is_stable_name(name) || !seen.insert(name) {
            return Err(format!("{owner} names must be stable and unique"));
        }
    }
    Ok(())
}

fn is_stable_name(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_whitespace)
}

fn checked_input_matches_port(argument: &CheckedActionArgument, port_type: &PortType) -> bool {
    match port_type {
        PortType::Design => {
            argument.mode == OwnershipMode::Copy
                && matches!(argument.value.value, CheckedExpression::Reference { .. })
                && material_state(&argument.value.r#type).is_none()
        }
        PortType::Material { state } => material_state(&argument.value.r#type)
            .is_some_and(|expected| expected == state.as_str()),
        PortType::Data { data_kind } => data_kind_for(&argument.value.r#type) == data_kind.as_str(),
        PortType::MaterialAsRequested | PortType::MaterialAsSupplied => false,
    }
}

fn checked_result_matches_port(result: &CheckedActionResult, port_type: &PortType) -> bool {
    match port_type {
        PortType::Design => material_state(&result.r#type).is_none(),
        PortType::Material { state } => {
            material_state(&result.r#type).is_some_and(|expected| expected == state.as_str())
        }
        PortType::Data { data_kind } => data_kind_for(&result.r#type) == data_kind.as_str(),
        PortType::MaterialAsRequested | PortType::MaterialAsSupplied => false,
    }
}

fn material_state(ty: &CheckedType) -> Option<String> {
    let CheckedType::Named { name, arguments } = ty else {
        return None;
    };
    if name != "Material" {
        return None;
    }
    let state = match arguments.first() {
        Some(CheckedType::InState { state, .. }) if AbsoluteIri::new(state).is_ok() => {
            return Some(state.clone());
        }
        Some(CheckedType::InState { state, .. }) => state.clone(),
        Some(subject) => format!("{}Product", subject.subject().display_name()),
        None => "MaterialProduct".to_owned(),
    };
    Some(format!("{STATE_NS}{state}"))
}

fn data_kind_for(ty: &CheckedType) -> String {
    let name = ty.subject().display_name();
    if AbsoluteIri::new(&name).is_ok() {
        return name;
    }
    let name = if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        name
    } else {
        format!(
            "type-{}",
            name.as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    };
    format!("{DATA_NS}{name}")
}

#[cfg(test)]
pub(crate) fn synthetic_intent(operation: &str) -> IntentAction {
    synthetic_intent_with_ports(operation, &[], &[])
}

#[cfg(test)]
pub(crate) fn synthetic_intent_with_ports(
    operation: &str,
    inputs: &[(String, PortType)],
    outputs: &[(String, PortType)],
) -> IntentAction {
    let module = "compiler.synthetic";
    let arguments = inputs
        .iter()
        .map(|(name, port_type)| CheckedActionArgument {
            name: name.clone(),
            mode: if port_type.is_material() {
                OwnershipMode::Take
            } else {
                OwnershipMode::Copy
            },
            value: TypedExpression {
                r#type: checked_type_for_port(port_type),
                value: CheckedExpression::Reference {
                    definition: DefinitionId::exported(module, name),
                    path: vec![name.clone()],
                },
            },
        })
        .collect();
    let results = outputs
        .iter()
        .map(|(name, port_type)| CheckedActionResult {
            name: name.clone(),
            r#type: checked_type_for_port(port_type),
            lineage: ResultLineage::Begins,
        })
        .collect::<Vec<_>>();
    IntentAction {
        source: IntentSource {
            module: module.to_owned(),
            workflow: DefinitionId::exported(module, "synthetic"),
            statement_path: vec![0],
        },
        action: ResolvedAction {
            callee: lab_language::ResolvedActionCallee::Action {
                definition: DefinitionId::exported(module, "synthetic"),
                operation: operation.to_owned(),
            },
            arguments,
            results: results.clone(),
        },
        ssa_operands: inputs.iter().map(|(name, _)| name.clone()).collect(),
        result_bindings: results
            .into_iter()
            .map(|result| CheckedField {
                name: result.name,
                r#type: result.r#type,
            })
            .collect(),
        artifact: None,
        artifact_dependencies: vec![],
        parameters: Default::default(),
    }
}

#[cfg(test)]
fn checked_type_for_port(port_type: &PortType) -> CheckedType {
    match port_type {
        PortType::Design => CheckedType::Named {
            name: "SyntheticDesign".to_owned(),
            arguments: vec![],
        },
        PortType::Material { state } => CheckedType::Named {
            name: "Material".to_owned(),
            arguments: vec![CheckedType::InState {
                subject: Box::new(CheckedType::Named {
                    name: "SyntheticMaterial".to_owned(),
                    arguments: vec![],
                }),
                state: state.to_string(),
            }],
        },
        PortType::Data { data_kind } => CheckedType::Named {
            name: data_kind.to_string(),
            arguments: vec![],
        },
        PortType::MaterialAsRequested | PortType::MaterialAsSupplied => CheckedType::Named {
            name: "Material".to_owned(),
            arguments: vec![],
        },
    }
}

/// One durable laboratory action, independent of scientific domain.
///
/// Adding an action or Method never adds a Rust operation class. The operation
/// carries one lossless intent document, named SSA operands, and typed results.
#[pliron_op(
    name = "workflow.perform",
    format,
    attributes = (
        perform_operation: StringAttr,
        perform_intent: StringAttr,
        perform_operand_names: VecAttr
    ),
    interfaces = [AtLeastNOpdsInterface<0>, AtLeastNResultsInterface<0>]
)]
pub struct PerformOp;

impl PerformOp {
    pub fn new(
        ctx: &mut Context,
        intent: &IntentAction,
        operand_names: Vec<String>,
        operands: Vec<Value>,
        result_types: Vec<TypeHandle>,
    ) -> Self {
        let result = Self {
            op: Operation::new(
                ctx,
                Self::get_concrete_op_info(),
                result_types,
                operands,
                vec![],
                0,
            ),
        };
        result.set_attr_perform_operation(
            ctx,
            StringAttr::new(
                intent
                    .operation()
                    .expect("IntentAction always describes an action declaration")
                    .to_owned(),
            ),
        );
        result.set_attr_perform_intent(
            ctx,
            StringAttr::new(
                serde_json::to_string(intent).expect("checked Intent actions serialize infallibly"),
            ),
        );
        result.set_attr_perform_operand_names(ctx, string_vec(operand_names));
        result
    }

    /// The complete checked invocation and compiler source coordinates.
    pub fn intent(&self, ctx: &Context) -> IntentAction {
        serde_json::from_str(
            self.get_attr_perform_intent(ctx)
                .expect("a verified workflow.perform carries its intent")
                .as_str(),
        )
        .expect("a verified workflow.perform carries a valid intent document")
    }

    pub fn operation(&self, ctx: &Context) -> String {
        self.get_attr_perform_operation(ctx)
            .expect("a verified workflow.perform carries its operation")
            .as_str()
            .to_owned()
    }

    pub fn operand_names(&self, ctx: &Context) -> Vec<String> {
        self.get_attr_perform_operand_names(ctx)
            .expect("a verified workflow.perform carries operand names")
            .0
            .iter()
            .map(|name| {
                name.downcast_ref::<StringAttr>()
                    .expect("verified operand names are strings")
                    .as_str()
                    .to_owned()
            })
            .collect()
    }

    pub fn results(&self, ctx: &Context) -> Vec<Value> {
        self.get_operation().deref(ctx).results().collect()
    }
}

impl Verify for PerformOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_perform_operation(ctx).as_deref(),
            "perform_operation",
            self.loc(ctx),
        )?;
        let Some(serialized) = self.get_attr_perform_intent(ctx) else {
            return verify_err!(self.loc(ctx), "workflow.perform requires perform_intent");
        };
        let Ok(intent) = serde_json::from_str::<IntentAction>(serialized.as_str()) else {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform requires a valid perform_intent contract"
            );
        };
        if let Err(message) = intent.validate() {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform perform_intent is invalid: {}",
                message
            );
        }
        if intent.operation() != Some(self.operation(ctx).as_str()) {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform operation must match its checked action declaration"
            );
        }
        if intent.action.results.len() != self.get_operation().deref(ctx).get_num_results() {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform results must match its checked action arity"
            );
        }
        let Some(names) = self.get_attr_perform_operand_names(ctx) else {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform requires perform_operand_names"
            );
        };
        if names.0.len() != self.get_operation().deref(ctx).get_num_operands()
            || names.0.iter().any(|name| {
                name.downcast_ref::<StringAttr>()
                    .is_none_or(|name| name.as_str().is_empty())
            })
        {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform operand names must match its SSA operands"
            );
        }
        let input_ports = names
            .0
            .iter()
            .zip(self.get_operation().deref(ctx).operands())
            .map(|(name, value)| {
                let name = name
                    .downcast_ref::<StringAttr>()
                    .expect("operand names were verified as strings above");
                semantic_workflow_port_type(ctx, value.get_type(ctx))
                    .map(|port_type| (name.as_str(), port_type))
            })
            .collect::<Option<Vec<_>>>();
        let Some(input_ports) = input_ports else {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform operands must be Design, material, or data ports"
            );
        };
        let output_ports = intent
            .action
            .results
            .iter()
            .zip(self.get_operation().deref(ctx).results())
            .map(|(result, value)| {
                semantic_workflow_port_type(ctx, value.get_type(ctx))
                    .map(|port_type| (result.name.as_str(), port_type))
            })
            .collect::<Option<Vec<_>>>();
        let Some(output_ports) = output_ports else {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform results must be Design, material, or data ports"
            );
        };
        if let Err(message) = intent.validate_ports(
            input_ports.iter().map(|(name, ty)| (*name, ty)),
            output_ports.iter().map(|(name, ty)| (*name, ty)),
        ) {
            return verify_err!(
                self.loc(ctx),
                "workflow.perform ports do not match perform_intent: {}",
                message
            );
        }
        Ok(())
    }
}

pub(crate) fn semantic_workflow_port_type(
    context: &Context,
    handle: TypeHandle,
) -> Option<PortType> {
    let ty = handle.deref(context);
    if ty.downcast_ref::<DesignType>().is_some() {
        return Some(PortType::Design);
    }
    if let Some(material) = ty.downcast_ref::<MaterialType>() {
        return Some(PortType::Material {
            state: AbsoluteIri::new(material.iri())
                .expect("verified Workflow material state is an absolute IRI"),
        });
    }
    ty.downcast_ref::<DataType>().map(|data| PortType::Data {
        data_kind: AbsoluteIri::new(data.iri())
            .expect("verified Workflow data kind is an absolute IRI"),
    })
}

#[cfg(test)]
mod tests {
    use pliron::operation::verify_operation;
    use pliron::printable::Printable;

    use super::*;

    fn material_port() -> PortType {
        PortType::Material {
            state: AbsoluteIri::new(format!("{STATE_NS}SampleProduct")).unwrap(),
        }
    }

    fn material_value(context: &mut Context) -> Value {
        let intent = synthetic_intent_with_ports(
            "example.produce",
            &[],
            &[("sample".to_owned(), material_port())],
        );
        let producer = PerformOp::new(
            context,
            &intent,
            Vec::new(),
            Vec::new(),
            vec![MaterialType::state(context, "SampleProduct")],
        );
        verify_operation(producer.get_operation(), context).unwrap();
        producer.get_operation().deref(context).get_result(0)
    }

    fn verification_error(operation: &PerformOp, context: &Context) -> String {
        verify_operation(operation.get_operation(), context)
            .unwrap_err()
            .disp(context)
            .to_string()
    }

    #[test]
    fn perform_operand_names_are_unique_members_of_the_retained_action() {
        let context = &mut Context::new();
        let material = material_value(context);
        let intent = synthetic_intent_with_ports(
            "example.consume",
            &[("sample".to_owned(), material_port())],
            &[],
        );
        let unknown = PerformOp::new(
            context,
            &intent,
            vec!["other".to_owned()],
            vec![material],
            vec![],
        );
        assert!(
            verification_error(&unknown, context).contains("does not match declared SSA operand")
        );
        let missing = PerformOp::new(context, &intent, Vec::new(), Vec::new(), vec![]);
        assert!(
            verification_error(&missing, context)
                .contains("exposes 0 input ports but declares 1 SSA operands")
        );

        let intent = synthetic_intent_with_ports(
            "example.combine",
            &[
                ("first".to_owned(), material_port()),
                ("second".to_owned(), material_port()),
            ],
            &[],
        );
        let duplicate = PerformOp::new(
            context,
            &intent,
            vec!["first".to_owned(), "first".to_owned()],
            vec![material, material],
            vec![],
        );
        assert!(verification_error(&duplicate, context).contains("is repeated"));
    }

    #[test]
    fn perform_ports_match_retained_checked_types() {
        let context = &mut Context::new();
        let material = material_value(context);
        let data = PortType::Data {
            data_kind: AbsoluteIri::new(format!("{DATA_NS}Evidence")).unwrap(),
        };
        let wrong_input_intent = synthetic_intent_with_ports(
            "example.inspect",
            &[("sample".to_owned(), data.clone())],
            &[],
        );
        let wrong_input = PerformOp::new(
            context,
            &wrong_input_intent,
            vec!["sample".to_owned()],
            vec![material],
            vec![],
        );
        assert!(verification_error(&wrong_input, context).contains("retained checked type"));

        let wrong_output_intent =
            synthetic_intent_with_ports("example.measure", &[], &[("evidence".to_owned(), data)]);
        let wrong_output = PerformOp::new(
            context,
            &wrong_output_intent,
            vec![],
            vec![],
            vec![MaterialType::state(context, "SampleProduct")],
        );
        assert!(verification_error(&wrong_output, context).contains("retained checked type"));
    }

    #[test]
    fn durable_intent_rejects_tampered_result_bindings() {
        let mut intent = synthetic_intent_with_ports(
            "example.measure",
            &[],
            &[(
                "evidence".to_owned(),
                PortType::Data {
                    data_kind: AbsoluteIri::new(format!("{DATA_NS}Evidence")).unwrap(),
                },
            )],
        );
        intent.result_bindings[0].r#type = CheckedType::String;
        assert_eq!(
            intent.validate().unwrap_err(),
            "Intent result bindings must match the retained action results"
        );
    }
}

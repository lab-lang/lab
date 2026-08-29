//! Generic, facility-independent procedure graph operations and value types.
//!
//! The dialect keeps its structural vocabulary deliberately small. A task's operation and each
//! material or data state are open absolute IRIs, so adding a method does not add a Rust operation
//! class to the compiler.

// Construction APIs are consumed by the forthcoming method-refinement pass.
#![allow(dead_code)]

use std::collections::BTreeSet;

use lab_capability::{AbsoluteIri, OperationId, PropertyKind};
use lab_method::{LocalId, PortType, ProcedureValue};
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::{NOpdsInterface, NResultsInterface};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::{pliron_op, pliron_type};
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::r#type::{TypeHandle, Typed};
use pliron::value::Value;
use pliron::{verify_err, verify_err_noloc};

use crate::lair::dialect::attributes::string_vec;
use crate::lair::dialect::design::DesignType;

/// One physical state in a method candidate.
#[pliron_type(
    name = "procedure.material",
    generate_get = true,
    format = "`<` $state `>`"
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MaterialType {
    state: StringAttr,
}

impl Verify for MaterialType {
    fn verify(&self, _context: &Context) -> Result<()> {
        if AbsoluteIri::new(self.state.as_str()).is_err() {
            return verify_err_noloc!("procedure.material state must be an absolute IRI");
        }
        Ok(())
    }
}

/// One non-physical information or evidence state in a method candidate.
#[pliron_type(name = "procedure.data", generate_get = true, format = "`<` $kind `>`")]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DataType {
    kind: StringAttr,
}

impl Verify for DataType {
    fn verify(&self, _context: &Context) -> Result<()> {
        if AbsoluteIri::new(self.kind.as_str()).is_err() {
            return verify_err_noloc!("procedure.data kind must be an absolute IRI");
        }
        Ok(())
    }
}

/// One generic procedure node with open semantic identity and typed SSA ports.
#[pliron_op(
    name = "procedure.task",
    format,
    attributes = (
        node_id: StringAttr,
        operation: StringAttr,
        task_output_names: VecAttr
    )
)]
pub(crate) struct TaskOp;

impl TaskOp {
    pub(crate) fn new(
        context: &mut Context,
        node_id: impl Into<String>,
        operation: &OperationId,
        operands: Vec<Value>,
        result_types: Vec<TypeHandle>,
        output_names: &[LocalId],
    ) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            result_types,
            operands,
            vec![],
            0,
        );
        let result = Self { op: raw };
        result.set_attr_node_id(context, StringAttr::new(node_id.into()));
        result.set_attr_operation(context, StringAttr::new(operation.to_string()));
        result.set_attr_task_output_names(
            context,
            string_vec(output_names.iter().map(ToString::to_string).collect()),
        );
        result
    }

    pub(crate) fn node_id(&self, context: &Context) -> String {
        self.get_attr_node_id(context)
            .expect("verified procedure.task carries node_id")
            .as_str()
            .to_owned()
    }

    pub(crate) fn semantic_node_id(&self, context: &Context) -> LocalId {
        LocalId::new(self.node_id(context)).expect("verified procedure.task carries a stable ID")
    }

    pub(crate) fn semantic_operation(&self, context: &Context) -> OperationId {
        OperationId::new(
            self.get_attr_operation(context)
                .expect("verified procedure.task carries operation")
                .as_str(),
        )
        .expect("verified procedure.task operation is an absolute IRI")
    }

    pub(crate) fn output_names(&self, context: &Context) -> Vec<LocalId> {
        self.get_attr_task_output_names(context)
            .expect("verified procedure.task carries output names")
            .0
            .iter()
            .map(|name| {
                LocalId::new(
                    name.downcast_ref::<StringAttr>()
                        .expect("verified procedure.task output names are strings")
                        .as_str(),
                )
                .expect("verified procedure.task output names are stable IDs")
            })
            .collect()
    }
}

impl Verify for TaskOp {
    fn verify(&self, context: &Context) -> Result<()> {
        let Some(node_id) = self.get_attr_node_id(context) else {
            return verify_err!(self.loc(context), "procedure.task is missing node_id");
        };
        if !is_stable_local_id(node_id.as_str()) {
            return verify_err!(
                self.loc(context),
                "procedure.task node_id must be non-empty and contain no whitespace"
            );
        }
        let Some(operation) = self.get_attr_operation(context) else {
            return verify_err!(self.loc(context), "procedure.task is missing operation");
        };
        if OperationId::new(operation.as_str()).is_err() {
            return verify_err!(
                self.loc(context),
                "procedure.task operation must be an absolute IRI"
            );
        }
        let operation = self.get_operation().deref(context);
        let Some(output_names) = self.get_attr_task_output_names(context) else {
            return verify_err!(self.loc(context), "procedure.task is missing output names");
        };
        if output_names.0.len() != operation.get_num_results() {
            return verify_err!(
                self.loc(context),
                "procedure.task output names must match its result arity"
            );
        }
        let mut seen = BTreeSet::new();
        for name in &output_names.0 {
            let Some(name) = name.downcast_ref::<StringAttr>() else {
                return verify_err!(
                    self.loc(context),
                    "procedure.task output names must contain only strings"
                );
            };
            if !is_stable_local_id(name.as_str()) {
                return verify_err!(
                    self.loc(context),
                    "procedure.task output names must be non-empty and contain no whitespace"
                );
            }
            if !seen.insert(name.as_str()) {
                return verify_err!(
                    self.loc(context),
                    "procedure.task output names must be unique"
                );
            }
        }
        for operand in operation.operands() {
            if !is_procedure_port_type(context, operand.get_type(context)) {
                return verify_err!(
                    self.loc(context),
                    "procedure.task operands must be Design, Procedure material, or Procedure data values"
                );
            }
        }
        for result in operation.results() {
            if !is_procedure_port_type(context, result.get_type(context)) {
                return verify_err!(
                    self.loc(context),
                    "procedure.task results must be Design, Procedure material, or Procedure data values"
                );
            }
        }
        Ok(())
    }
}

fn is_procedure_port_type(context: &Context, handle: TypeHandle) -> bool {
    let ty = handle.deref(context);
    ty.downcast_ref::<DesignType>().is_some()
        || ty.downcast_ref::<MaterialType>().is_some()
        || ty.downcast_ref::<DataType>().is_some()
}

pub(crate) fn semantic_port_type(context: &Context, handle: TypeHandle) -> Option<PortType> {
    let ty = handle.deref(context);
    if ty.downcast_ref::<DesignType>().is_some() {
        return Some(PortType::Design);
    }
    if let Some(material) = ty.downcast_ref::<MaterialType>() {
        return Some(PortType::Material {
            state: AbsoluteIri::new(material.state.as_str())
                .expect("verified Procedure material state is an absolute IRI"),
        });
    }
    ty.downcast_ref::<DataType>().map(|data| PortType::Data {
        data_kind: AbsoluteIri::new(data.kind.as_str())
            .expect("verified Procedure data kind is an absolute IRI"),
    })
}

/// One exact semantic parameter attached to a Procedure task by stable identity.
#[pliron_op(
    name = "procedure.parameter",
    format,
    attributes = (
        procedure_parameter_id: StringAttr,
        procedure_parameter_node: StringAttr,
        procedure_parameter_kind: StringAttr,
        procedure_parameter_value: StringAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct ParameterOp;

impl ParameterOp {
    pub(crate) fn new(
        context: &mut Context,
        parameter_id: impl Into<String>,
        procedure_node: impl Into<String>,
        property_kind: &PropertyKind,
        value: &ProcedureValue,
    ) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: raw };
        result.set_attr_procedure_parameter_id(context, StringAttr::new(parameter_id.into()));
        result.set_attr_procedure_parameter_node(context, StringAttr::new(procedure_node.into()));
        result
            .set_attr_procedure_parameter_kind(context, StringAttr::new(property_kind.to_string()));
        result.set_attr_procedure_parameter_value(
            context,
            StringAttr::new(
                serde_json::to_string(value)
                    .expect("ProcedureValue contains only infallibly serializable values"),
            ),
        );
        result
    }

    pub(crate) fn parameter_id(&self, context: &Context) -> String {
        self.get_attr_procedure_parameter_id(context)
            .expect("verified procedure.parameter carries parameter_id")
            .as_str()
            .to_owned()
    }

    pub(crate) fn procedure_node(&self, context: &Context) -> String {
        self.get_attr_procedure_parameter_node(context)
            .expect("verified procedure.parameter carries procedure_node")
            .as_str()
            .to_owned()
    }

    pub(crate) fn semantic_parameter(
        &self,
        context: &Context,
    ) -> (LocalId, PropertyKind, ProcedureValue) {
        let id = LocalId::new(self.parameter_id(context))
            .expect("verified procedure.parameter carries a stable ID");
        let property = PropertyKind::new(
            self.get_attr_procedure_parameter_kind(context)
                .expect("verified procedure.parameter carries a property kind")
                .as_str(),
        )
        .expect("verified procedure.parameter property kind is an absolute IRI");
        let value = self
            .get_attr_procedure_parameter_value(context)
            .expect("verified procedure.parameter carries a value");
        let value = serde_json::from_str(value.as_str())
            .expect("verified procedure.parameter carries a semantic value");
        (id, property, value)
    }
}

impl Verify for ParameterOp {
    fn verify(&self, context: &Context) -> Result<()> {
        for (name, value) in [
            (
                "parameter_id",
                self.get_attr_procedure_parameter_id(context),
            ),
            (
                "procedure_node",
                self.get_attr_procedure_parameter_node(context),
            ),
        ] {
            if value.is_none_or(|value| !is_stable_local_id(value.as_str())) {
                return verify_err!(
                    self.loc(context),
                    "procedure.parameter {name} must be non-empty and contain no whitespace"
                );
            }
        }
        if self
            .get_attr_procedure_parameter_kind(context)
            .is_none_or(|value| PropertyKind::new(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "procedure.parameter property_kind must be an absolute IRI"
            );
        }
        let Some(value) = self.get_attr_procedure_parameter_value(context) else {
            return verify_err!(self.loc(context), "procedure.parameter is missing value");
        };
        let parsed = match serde_json::from_str::<ProcedureValue>(value.as_str()) {
            Ok(parsed) => parsed,
            Err(error) => {
                return verify_err!(self.loc(context), "invalid procedure.parameter: {error}");
            }
        };
        if !parsed.validate() {
            return verify_err!(
                self.loc(context),
                "invalid procedure.parameter: list values do not match their element type"
            );
        }
        Ok(())
    }
}

pub(crate) fn is_stable_local_id(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

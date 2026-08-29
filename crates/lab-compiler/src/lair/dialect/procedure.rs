//! Generic, facility-independent procedure graph operations and value types.
//!
//! The dialect keeps its structural vocabulary deliberately small. A task's operation and each
//! material or data state are open absolute IRIs, so adding a method does not add a Rust operation
//! class to the compiler.

// Construction APIs are consumed by the forthcoming method-refinement pass.
#![allow(dead_code)]

use lab_capability::{AbsoluteIri, OperationId};
use pliron::builtin::attributes::StringAttr;
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::{pliron_op, pliron_type};
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::r#type::TypeHandle;
use pliron::value::Value;
use pliron::{verify_err, verify_err_noloc};

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
    attributes = (node_id: StringAttr, operation: StringAttr)
)]
pub(crate) struct TaskOp;

impl TaskOp {
    pub(crate) fn new(
        context: &mut Context,
        node_id: impl Into<String>,
        operation: &OperationId,
        operands: Vec<Value>,
        result_types: Vec<TypeHandle>,
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
        result
    }

    pub(crate) fn node_id(&self, context: &Context) -> String {
        self.get_attr_node_id(context)
            .expect("verified procedure.task carries node_id")
            .as_str()
            .to_owned()
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
        Ok(())
    }
}

pub(crate) fn is_stable_local_id(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

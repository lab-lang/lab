use std::collections::BTreeSet;

use lab_capability::{
    AbsoluteIri, CapabilityKind, ConstraintRelation, ControlMode, ExactDecimal, ExactInteger,
    MethodId, OperationId, ProcedureContractId, PropertyKind, PropertyValue, QualificationLevel,
    ScalarValue, UnitIri,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::method::{IntentOperationId, LocalId};
use crate::procedure::ProcedureProgramBuilderId;

/// The semantic type of one method or Procedure port.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum PortType {
    /// Reusable biological design information retained in the Design dialect.
    Design,
    /// Physical matter in an open, method-defined state.
    Material { state: AbsoluteIri },
    /// Physical matter in whatever state the Intent operation asked for.
    ///
    /// A Method usually knows the state it produces: a transformation yields a
    /// transformed culture whatever went in. Fetching something off a shelf does
    /// not. What `provision` returns is competent cells, or a plasmid prep, or a
    /// bottle of medium, according to what was asked for, and one signature has
    /// to serve all of them because every candidate refining one Intent
    /// implements one signature.
    ///
    /// This is a port whose state the Intent decides. It is only meaningful on
    /// an output, and only where a Method output exports it, since the state is
    /// read from the Intent result it corresponds to.
    MaterialAsRequested,
    /// Non-physical information or evidence of an open semantic kind.
    Data { data_kind: AbsoluteIri },
    /// Physical matter in the exact state supplied by the Intent input.
    ///
    /// This is the input-side counterpart of `MaterialAsRequested`. It lets a
    /// domain action such as disposal accept every checked material state
    /// without weakening the concrete state carried by the resulting planning
    /// port. It is meaningful only on a Method input.
    MaterialAsSupplied,
}

impl PortType {
    /// Whether this port carries physical matter, in a stated state or in
    /// whichever one the Intent asked for.
    pub fn is_material(&self) -> bool {
        matches!(
            self,
            Self::Material { .. } | Self::MaterialAsRequested | Self::MaterialAsSupplied
        )
    }
}

/// One named input accepted by every candidate refining the same Intent operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodInput {
    pub name: LocalId,
    pub port_type: PortType,
}

/// An Intent parameter required by this Method candidate.
///
/// Candidates refining the same Intent operation may require different parameters. The operation's
/// typed value inputs and outputs form the common interface; a candidate is applicable only when
/// every parameter it declares is present with the declared semantic value type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodParameter {
    /// Local name used by this Method and its Procedure tasks.
    pub name: LocalId,
    /// Intent parameter to read. Omission means the local name, which keeps the common case terse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<LocalId>,
    pub value_type: ParameterType,
    /// Method-owned value used only when the Intent does not state the source parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

/// The closed structural shape expected from one Intent parameter.
///
/// Scalar kinds use the same exact lexical model as capability properties. Lists deliberately
/// contain one scalar kind so Methods can carry ordered recipe data without accepting arbitrary
/// JSON values into the compiler ABI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ParameterType {
    Scalar { scalar_type: ScalarType },
    List { element_type: ScalarType },
}

/// The closed scalar shape expected from an Intent parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScalarType {
    Text,
    Integer,
    Real,
    Boolean,
    Iri,
}

impl ScalarType {
    pub fn of(value: &ScalarValue) -> Self {
        match value {
            ScalarValue::Text(_) => Self::Text,
            ScalarValue::Integer(_) => Self::Integer,
            ScalarValue::Real(_) => Self::Real,
            ScalarValue::Boolean(_) => Self::Boolean,
            ScalarValue::Iri(_) => Self::Iri,
        }
    }

    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::Integer | Self::Real)
    }
}

/// A reference to a Procedure value available at a task or method output.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ValueReference {
    Input { input: LocalId },
    TaskOutput { task: LocalId, output: LocalId },
}

/// One typed result produced by a Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskOutput {
    pub name: LocalId,
    pub port_type: PortType,
}

/// One result yielded from a complete method candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodOutput {
    pub name: LocalId,
    pub source: ValueReference,
}

/// A literal value or a reference to a scalar supplied by the refined Intent operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ScalarValueExpression {
    Literal {
        value: PropertyValue,
    },
    IntentParameter {
        parameter: LocalId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<UnitIri>,
    },
}

/// One exact semantic value carried from Intent into a selected Procedure task.
///
/// Capability constraints remain scalar. Procedure parameters are broader because an adapter also
/// needs ordered recipe data such as component or dependency lists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ProcedureValue {
    Scalar {
        value: PropertyValue,
    },
    List {
        element_type: ScalarType,
        values: Vec<PropertyValue>,
    },
}

impl ProcedureValue {
    pub fn value_type(&self) -> ParameterType {
        match self {
            Self::Scalar { value } => ParameterType::Scalar {
                scalar_type: ScalarType::of(&value.value),
            },
            Self::List { element_type, .. } => ParameterType::List {
                element_type: *element_type,
            },
        }
    }

    pub fn validate(&self) -> bool {
        match self {
            Self::Scalar { .. } => true,
            Self::List {
                element_type,
                values,
            } => values
                .iter()
                .all(|value| ScalarType::of(&value.value) == *element_type),
        }
    }
}

impl MethodParameter {
    /// Convert the compact JSON default into the same exact typed value carried by Intent.
    /// Defaults are unitless; a task reference applies and validates its Procedure unit exactly
    /// as it does for a source-supplied value.
    pub(crate) fn resolved_default(&self) -> Result<Option<ProcedureValue>, String> {
        let Some(default) = &self.default else {
            return Ok(None);
        };
        let scalar = |value| ProcedureValue::Scalar {
            value: PropertyValue::unitless(value),
        };
        let convert = |value: &serde_json::Value, expected: ScalarType| {
            default_scalar(value, expected).map(PropertyValue::unitless)
        };
        let value = match &self.value_type {
            ParameterType::Scalar { scalar_type } => scalar(default_scalar(default, *scalar_type)?),
            ParameterType::List { element_type } => {
                let values = default
                    .as_array()
                    .ok_or_else(|| format!("default for `{}` must be a JSON array", self.name))?;
                ProcedureValue::List {
                    element_type: *element_type,
                    values: values
                        .iter()
                        .map(|value| convert(value, *element_type))
                        .collect::<Result<Vec<_>, _>>()?,
                }
            }
        };
        Ok(Some(value))
    }
}

fn default_scalar(value: &serde_json::Value, expected: ScalarType) -> Result<ScalarValue, String> {
    let invalid = || format!("default value does not match {expected:?}");
    match expected {
        ScalarType::Text => value
            .as_str()
            .map(|value| ScalarValue::Text(value.to_owned()))
            .ok_or_else(invalid),
        ScalarType::Integer => value
            .as_number()
            .and_then(|value| ExactInteger::parse(value.to_string()).ok())
            .map(ScalarValue::Integer)
            .ok_or_else(invalid),
        ScalarType::Real => value
            .as_number()
            .and_then(|value| ExactDecimal::parse(value.to_string()).ok())
            .map(ScalarValue::Real)
            .ok_or_else(invalid),
        ScalarType::Boolean => value
            .as_bool()
            .map(ScalarValue::Boolean)
            .ok_or_else(invalid),
        ScalarType::Iri => value
            .as_str()
            .ok_or_else(invalid)
            .and_then(|value| AbsoluteIri::new(value).map_err(|_| invalid()))
            .map(ScalarValue::Iri),
    }
}

/// A literal Procedure value or a reference to a value supplied by the refined Intent operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ProcedureValueExpression {
    Literal {
        value: ProcedureValue,
    },
    IntentParameter {
        parameter: LocalId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<UnitIri>,
    },
}

/// One typed offering-property constraint template in a portable method definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilityConstraintDefinition {
    pub property_kind: PropertyKind,
    pub relation: ConstraintRelation,
    pub required: ScalarValueExpression,
}

/// One exact semantic parameter carried by a Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcedureParameterDefinition {
    pub id: LocalId,
    pub property_kind: PropertyKind,
    pub value: ProcedureValueExpression,
}

/// A literal inventory lookup symbol or a symbol-valued Intent parameter.
///
/// Symbols remain frontend values at this portable boundary. Facility planning resolves each
/// concrete symbol through the checked declaration's exact SBOL Component identity and then binds
/// that Component to a physical MaterialLot. A list-valued parameter expands to one stable
/// material input per element during refinement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum MaterialSourceExpression {
    Literal { symbol: String },
    IntentParameter { parameter: LocalId },
}

/// One external material source required by a Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialInputDefinition {
    pub id: LocalId,
    pub source: MaterialSourceExpression,
}

/// One semantic capability requirement owned by its enclosing Procedure task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequirementDefinition {
    pub id: LocalId,
    pub capability_kind: CapabilityKind,
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<CapabilityConstraintDefinition>,
}

/// The facility policy applied to every capability clause derived from one Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicyDefinition {
    pub minimum_qualification: QualificationLevel,
    pub accepted_control_modes: BTreeSet<ControlMode>,
}

/// How one Method task becomes executable Procedure semantics.
///
/// A template renders checked Method-task values into a declarative contract body. A builder
/// names a statically linked construction algorithm. Neither can author capability requirements,
/// because those are derived from the completed program. A primitive task has no program and
/// states its capability requirements directly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ProcedureTaskExecutionDefinition {
    Template {
        contract: ProcedureContractId,
        body: serde_json::Value,
        policy: ExecutionPolicyDefinition,
    },
    Builder {
        builder: ProcedureProgramBuilderId,
        contract: ProcedureContractId,
        policy: ExecutionPolicyDefinition,
    },
    Primitive {
        requirements: Vec<CapabilityRequirementDefinition>,
    },
}

/// One task in topological order within a facility-independent Procedure graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcedureTaskDefinition {
    pub id: LocalId,
    pub operation: OperationId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<ValueReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<TaskOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ProcedureParameterDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<MaterialInputDefinition>,
    pub execution: ProcedureTaskExecutionDefinition,
}

/// A portable method definition that refines one Intent operation without selecting a facility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodDefinition {
    pub id: MethodId,
    pub refines: IntentOperationId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<MethodInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<MethodParameter>,
    pub tasks: Vec<ProcedureTaskDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<MethodOutput>,
}

/// The common typed value boundary all methods refining one Intent operation must implement.
///
/// Method parameters are deliberately absent. They are candidate-specific applicability
/// requirements, not part of the common value interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSignature {
    pub inputs: Vec<MethodInput>,
    pub outputs: Vec<TaskOutput>,
}

#[cfg(test)]
mod strictness_tests {
    use super::{
        CapabilityRequirementDefinition, MaterialInputDefinition, MaterialSourceExpression,
        ProcedureTaskDefinition, ProcedureTaskExecutionDefinition,
    };

    /// A misspelled collection key must not silently deserialize as an empty collection. Dropping
    /// `materials` would plan a task that never allocates its reagent, and dropping `constraints`
    /// would bind an offering whose envelope was never checked.
    #[test]
    fn a_misspelled_key_is_rejected_rather_than_dropped() {
        let task = r#"{
            "id": "setup",
            "operation": "https://example.org/procedure#Setup",
            "materialz": [{"id": "buffer", "source": {"kind": "literal", "symbol": "T4"}}],
            "execution": {"kind": "primitive", "requirements": []}
        }"#;
        let error = serde_json::from_str::<ProcedureTaskDefinition>(task).unwrap_err();
        assert!(
            error.to_string().contains("materialz"),
            "the unknown key must be named: {error}"
        );

        let requirement = r#"{
            "id": "transfer",
            "capability_kind": "https://sbol.io/ns/capability#MeteredLiquidTransfer",
            "minimum_qualification": "https://sbol.io/ns/facility#Plannable",
            "accepted_control_modes": ["https://sbol.io/ns/facility#ReviewedFileControl"],
            "constraintz": []
        }"#;
        let error =
            serde_json::from_str::<CapabilityRequirementDefinition>(requirement).unwrap_err();
        assert!(error.to_string().contains("constraintz"), "{error}");
    }

    #[test]
    fn an_unknown_key_inside_a_tagged_expression_is_rejected() {
        let material = r#"{
            "id": "buffer",
            "source": {"kind": "literal", "symbol": "T4", "symbal": "T5"}
        }"#;
        let error = serde_json::from_str::<MaterialInputDefinition>(material).unwrap_err();
        assert!(error.to_string().contains("symbal"), "{error}");
    }

    #[test]
    fn a_well_formed_record_still_round_trips() {
        let source: MaterialSourceExpression =
            serde_json::from_str(r#"{"kind": "literal", "symbol": "T4"}"#).unwrap();
        assert_eq!(
            source,
            MaterialSourceExpression::Literal {
                symbol: "T4".to_owned()
            }
        );
    }

    #[test]
    fn template_builder_and_primitive_execution_have_disjoint_serialized_shapes() {
        let builder_with_requirements = r#"{
            "kind": "builder",
            "builder": "https://example.org/builder",
            "contract": "https://example.org/contract",
            "policy": {
                "minimum_qualification": "https://sbol.io/ns/facility#Plannable",
                "accepted_control_modes": ["https://sbol.io/ns/facility#ManualControl"]
            },
            "requirements": []
        }"#;
        let error =
            serde_json::from_str::<ProcedureTaskExecutionDefinition>(builder_with_requirements)
                .unwrap_err();
        assert!(error.to_string().contains("requirements"), "{error}");

        let primitive_with_builder = r#"{
            "kind": "primitive",
            "builder": "https://example.org/builder",
            "requirements": []
        }"#;
        let error =
            serde_json::from_str::<ProcedureTaskExecutionDefinition>(primitive_with_builder)
                .unwrap_err();
        assert!(error.to_string().contains("builder"), "{error}");

        let removed_program_kind = r#"{
            "kind": "program",
            "builder": "https://example.org/builder",
            "contract": "https://example.org/contract",
            "policy": {
                "minimum_qualification": "https://sbol.io/ns/facility#Plannable",
                "accepted_control_modes": ["https://sbol.io/ns/facility#ManualControl"]
            }
        }"#;
        let error = serde_json::from_str::<ProcedureTaskExecutionDefinition>(removed_program_kind)
            .unwrap_err();
        assert!(
            error.to_string().contains("unknown variant `program`"),
            "{error}"
        );
    }
}

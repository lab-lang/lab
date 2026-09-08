//! Declarative Procedure-program templates embedded in portable Method catalogs.

use lab_capability::{ProcedureContractId, ScalarValue};
use serde::Deserialize;
use serde_json::{Map, Number, Value};
use thiserror::Error;

use crate::method::{LocalId, ProcedureValue};
use crate::procedure::{
    ProcedureProgramBuildContext, ProcedureProgramValidationError, ResolvedProcedureMaterial,
};

const MAX_TEMPLATE_DEPTH: usize = 128;
const SLOT_KEY: &str = "$lab";

/// An error while resolving one declarative Procedure template against its Method task.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("invalid Procedure template at {path}: {message}")]
pub struct ProcedureTemplateEvaluationError {
    pub path: String,
    pub message: String,
}

/// Rendering or contract validation failed for a declarative Procedure template.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ProcedureProgramTemplateError {
    #[error(transparent)]
    Evaluation(#[from] ProcedureTemplateEvaluationError),
    #[error("Procedure template for `{contract}` rendered an invalid program: {source}")]
    InvalidProgram {
        contract: ProcedureContractId,
        #[source]
        source: Box<ProcedureProgramValidationError>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TemplateSlot {
    /// The complete checked [`crate::workflow::IntentAction`] being refined.
    Intent,
    /// The complete checked [`crate::design::ArtifactDesign`] owning this action.
    Artifact,
    /// The complete tagged [`ProcedureValue`] carried by the task parameter.
    Parameter { id: LocalId },
    /// The complete typed [`lab_capability::PropertyValue`] carried by a scalar parameter.
    Scalar { id: LocalId },
    /// A unitless exact integer represented as a JSON integer.
    Integer { id: LocalId },
    /// A unitless text scalar represented as a JSON string.
    Text { id: LocalId },
    /// A unitless boolean scalar represented as a JSON boolean.
    Boolean { id: LocalId },
    /// A unitless absolute-IRI scalar represented as a JSON string.
    Iri { id: LocalId },
    /// A checked, zero-based input index of the enclosing task.
    Input { index: usize },
    /// One output ID declared by the enclosing task.
    Output { id: LocalId },
    /// One material ID declared by the enclosing task, resolved to its stable task-local ID.
    Material { id: LocalId },
}

/// Resolve every explicit `$lab` slot in a JSON value.
///
/// Ordinary JSON is copied exactly. A slot is an object whose sole key is `$lab`; its value must
/// be one of the closed tagged shapes represented by [`TemplateSlot`]. Keys beginning with
/// `$lab` are reserved at every depth so misspellings cannot silently become contract data.
pub fn evaluate_procedure_template(
    template: &Value,
    context: &ProcedureProgramBuildContext<'_>,
) -> Result<Value, ProcedureTemplateEvaluationError> {
    evaluate(template, context, "$", 0)
}

fn evaluate(
    value: &Value,
    context: &ProcedureProgramBuildContext<'_>,
    path: &str,
    depth: usize,
) -> Result<Value, ProcedureTemplateEvaluationError> {
    if depth > MAX_TEMPLATE_DEPTH {
        return invalid(
            path,
            format!("template nesting exceeds {MAX_TEMPLATE_DEPTH} levels"),
        );
    }
    match value {
        Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| evaluate(value, context, &format!("{path}[{index}]"), depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(object) => evaluate_object(object, context, path, depth),
        primitive => Ok(primitive.clone()),
    }
}

fn evaluate_object(
    object: &Map<String, Value>,
    context: &ProcedureProgramBuildContext<'_>,
    path: &str,
    depth: usize,
) -> Result<Value, ProcedureTemplateEvaluationError> {
    if let Some(slot) = parse_slot(object, path)? {
        return evaluate_slot(slot, context, path);
    }

    object
        .iter()
        .map(|(key, value)| {
            let key_path = format!(
                "{path}[{}]",
                serde_json::to_string(key).expect("a JSON object key serializes infallibly")
            );
            evaluate(value, context, &key_path, depth + 1).map(|value| (key.clone(), value))
        })
        .collect::<Result<Map<_, _>, _>>()
        .map(Value::Object)
}

fn parse_slot(
    object: &Map<String, Value>,
    path: &str,
) -> Result<Option<TemplateSlot>, ProcedureTemplateEvaluationError> {
    let reserved = object
        .keys()
        .filter(|key| key.starts_with(SLOT_KEY))
        .collect::<Vec<_>>();
    if !reserved.is_empty() {
        if object.len() != 1 || !object.contains_key(SLOT_KEY) {
            return invalid(
                path,
                format!(
                    "`{SLOT_KEY}` is reserved for a slot object and cannot have siblings or be extended"
                ),
            );
        }
        return serde_json::from_value::<TemplateSlot>(object[SLOT_KEY].clone())
            .map(Some)
            .map_err(|error| ProcedureTemplateEvaluationError {
                path: path.to_owned(),
                message: format!("malformed `{SLOT_KEY}` slot: {error}"),
            });
    }
    Ok(None)
}

/// Validate a Method's template slots using only its declared task shape.
///
/// Value-dependent projections are checked again while rendering, but malformed slots and
/// references to absent inputs, outputs, parameters, or materials can and should fail when the
/// Method catalog is loaded.
pub(crate) fn validate_procedure_template_shape(
    template: &Value,
    input_count: usize,
    outputs: &[LocalId],
    parameters: &[LocalId],
    materials: &[LocalId],
) -> Result<(), ProcedureTemplateEvaluationError> {
    fn walk(
        value: &Value,
        path: &str,
        depth: usize,
        input_count: usize,
        outputs: &[LocalId],
        parameters: &[LocalId],
        materials: &[LocalId],
    ) -> Result<(), ProcedureTemplateEvaluationError> {
        if depth > MAX_TEMPLATE_DEPTH {
            return invalid(
                path,
                format!("template nesting exceeds {MAX_TEMPLATE_DEPTH} levels"),
            );
        }
        match value {
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    walk(
                        value,
                        &format!("{path}[{index}]"),
                        depth + 1,
                        input_count,
                        outputs,
                        parameters,
                        materials,
                    )?;
                }
            }
            Value::Object(object) => {
                if let Some(slot) = parse_slot(object, path)? {
                    match slot {
                        TemplateSlot::Input { index } if index >= input_count => {
                            return invalid(
                                path,
                                format!(
                                    "input index {index} is out of bounds for {input_count} declared input(s)"
                                ),
                            );
                        }
                        TemplateSlot::Output { id } if !outputs.contains(&id) => {
                            return invalid(
                                path,
                                format!("output `{id}` is not declared by this task"),
                            );
                        }
                        TemplateSlot::Material { id } if !materials.contains(&id) => {
                            return invalid(
                                path,
                                format!("material `{id}` is not declared by this task"),
                            );
                        }
                        TemplateSlot::Parameter { id }
                        | TemplateSlot::Scalar { id }
                        | TemplateSlot::Integer { id }
                        | TemplateSlot::Text { id }
                        | TemplateSlot::Boolean { id }
                        | TemplateSlot::Iri { id }
                            if !parameters.contains(&id) =>
                        {
                            return invalid(
                                path,
                                format!("parameter `{id}` is not declared by this task"),
                            );
                        }
                        _ => {}
                    }
                    return Ok(());
                }
                for (key, value) in object {
                    let key_path = format!(
                        "{path}[{}]",
                        serde_json::to_string(key)
                            .expect("a JSON object key serializes infallibly")
                    );
                    walk(
                        value,
                        &key_path,
                        depth + 1,
                        input_count,
                        outputs,
                        parameters,
                        materials,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    walk(
        template,
        "$",
        0,
        input_count,
        outputs,
        parameters,
        materials,
    )
}

fn evaluate_slot(
    slot: TemplateSlot,
    context: &ProcedureProgramBuildContext<'_>,
    path: &str,
) -> Result<Value, ProcedureTemplateEvaluationError> {
    match slot {
        TemplateSlot::Intent => serialize(context.intent, path),
        TemplateSlot::Artifact => context
            .intent
            .artifact
            .as_ref()
            .ok_or_else(|| ProcedureTemplateEvaluationError {
                path: path.to_owned(),
                message: "this Intent action is not owned by an artifact design".to_owned(),
            })
            .and_then(|artifact| serialize(artifact, path)),
        TemplateSlot::Parameter { id } => serialize(parameter(context, &id, path)?, path),
        TemplateSlot::Scalar { id } => {
            let ProcedureValue::Scalar { value } = parameter(context, &id, path)? else {
                return invalid(path, format!("parameter `{id}` is not a scalar"));
            };
            serialize(value, path)
        }
        TemplateSlot::Integer { id } => {
            let scalar = unitless_scalar(context, &id, "integer", path)?;
            let ScalarValue::Integer(integer) = scalar else {
                return wrong_scalar(path, &id, "integer", scalar);
            };
            let number = if integer.as_str().starts_with('-') {
                integer.as_str().parse::<i64>().map(Number::from)
            } else {
                integer.as_str().parse::<u64>().map(Number::from)
            }
            .map_err(|_| ProcedureTemplateEvaluationError {
                path: path.to_owned(),
                message: format!(
                    "integer parameter `{id}` is outside the exact JSON integer range"
                ),
            })?;
            Ok(Value::Number(number))
        }
        TemplateSlot::Text { id } => {
            let scalar = unitless_scalar(context, &id, "text", path)?;
            let ScalarValue::Text(text) = scalar else {
                return wrong_scalar(path, &id, "text", scalar);
            };
            Ok(Value::String(text.clone()))
        }
        TemplateSlot::Boolean { id } => {
            let scalar = unitless_scalar(context, &id, "boolean", path)?;
            let ScalarValue::Boolean(value) = scalar else {
                return wrong_scalar(path, &id, "boolean", scalar);
            };
            Ok(Value::Bool(*value))
        }
        TemplateSlot::Iri { id } => {
            let scalar = unitless_scalar(context, &id, "IRI", path)?;
            let ScalarValue::Iri(iri) = scalar else {
                return wrong_scalar(path, &id, "IRI", scalar);
            };
            Ok(Value::String(iri.to_string()))
        }
        TemplateSlot::Input { index } => {
            if index >= context.input_count {
                return invalid(
                    path,
                    format!(
                        "input index {index} is out of bounds for {} declared input(s)",
                        context.input_count
                    ),
                );
            }
            let index = u64::try_from(index).map_err(|_| ProcedureTemplateEvaluationError {
                path: path.to_owned(),
                message: format!("input index {index} cannot be represented as a JSON integer"),
            })?;
            Ok(Value::Number(Number::from(index)))
        }
        TemplateSlot::Output { id } => {
            let Some(output) = context.outputs.iter().find(|output| *output == &id) else {
                return invalid(path, format!("output `{id}` is not declared by this task"));
            };
            Ok(Value::String(output.to_string()))
        }
        TemplateSlot::Material { id } => {
            let matches = context
                .materials
                .iter()
                .filter(|material| material_declared_as(material, &id))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [material] => Ok(Value::String(material.id.to_string())),
                [] => invalid(
                    path,
                    format!("material `{id}` is not declared by this task"),
                ),
                _ => invalid(
                    path,
                    format!(
                        "material `{id}` resolves to {} values; a template material slot requires exactly one",
                        matches.len()
                    ),
                ),
            }
        }
    }
}

fn parameter<'a>(
    context: &'a ProcedureProgramBuildContext<'_>,
    id: &LocalId,
    path: &str,
) -> Result<&'a ProcedureValue, ProcedureTemplateEvaluationError> {
    let suffix = format!("::parameter::{id}");
    let matches = context
        .parameters
        .iter()
        .filter(|parameter| {
            parameter.id.as_str() == id.as_str() || parameter.id.as_str().ends_with(&suffix)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [parameter] => Ok(&parameter.value),
        [] => invalid(
            path,
            format!("parameter `{id}` is not declared by this task"),
        ),
        _ => invalid(
            path,
            format!(
                "parameter `{id}` resolves ambiguously to {} values",
                matches.len()
            ),
        ),
    }
}

fn unitless_scalar<'a>(
    context: &'a ProcedureProgramBuildContext<'_>,
    id: &LocalId,
    projection: &str,
    path: &str,
) -> Result<&'a ScalarValue, ProcedureTemplateEvaluationError> {
    let ProcedureValue::Scalar { value } = parameter(context, id, path)? else {
        return invalid(
            path,
            format!("parameter `{id}` is not a scalar and cannot be projected as {projection}"),
        );
    };
    if let Some(unit) = &value.unit {
        return invalid(
            path,
            format!(
                "parameter `{id}` has unit `{unit}` and cannot be projected as unitless {projection}"
            ),
        );
    }
    Ok(&value.value)
}

fn wrong_scalar<T>(
    path: &str,
    id: &LocalId,
    expected: &str,
    actual: &ScalarValue,
) -> Result<T, ProcedureTemplateEvaluationError> {
    invalid(
        path,
        format!(
            "parameter `{id}` is {}, not {expected}",
            scalar_kind(actual)
        ),
    )
}

fn scalar_kind(value: &ScalarValue) -> &'static str {
    match value {
        ScalarValue::Text(_) => "text",
        ScalarValue::Integer(_) => "integer",
        ScalarValue::Real(_) => "real",
        ScalarValue::Boolean(_) => "boolean",
        ScalarValue::Iri(_) => "an IRI",
    }
}

fn material_declared_as(material: &ResolvedProcedureMaterial, declared: &LocalId) -> bool {
    if material.id.as_str() == declared.as_str() {
        return true;
    }
    let Some((_, suffix)) = material.id.as_str().rsplit_once("::material::") else {
        return false;
    };
    suffix == declared.as_str()
        || suffix
            .strip_prefix(declared.as_str())
            .is_some_and(|suffix| suffix.starts_with("::"))
}

fn serialize(
    value: &impl serde::Serialize,
    path: &str,
) -> Result<Value, ProcedureTemplateEvaluationError> {
    serde_json::to_value(value).map_err(|error| ProcedureTemplateEvaluationError {
        path: path.to_owned(),
        message: format!("resolved value cannot be represented as JSON: {error}"),
    })
}

fn invalid<T>(
    path: &str,
    message: impl Into<String>,
) -> Result<T, ProcedureTemplateEvaluationError> {
    Err(ProcedureTemplateEvaluationError {
        path: path.to_owned(),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use lab_capability::{AbsoluteIri, ExactInteger, PropertyValue};
    use serde_json::json;

    use crate::procedure::{ResolvedProcedureMaterial, ResolvedProcedureParameter};

    use super::*;

    fn local(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    #[test]
    fn slots_resolve_recursively_and_preserve_exact_typed_values() {
        let parameters = [
            ResolvedProcedureParameter {
                id: local("task::parameter::cycles"),
                value: ProcedureValue::Scalar {
                    value: PropertyValue::unitless(ScalarValue::Integer(
                        ExactInteger::parse("30").unwrap(),
                    )),
                },
            },
            ResolvedProcedureParameter {
                id: local("task::parameter::predicate"),
                value: ProcedureValue::Scalar {
                    value: PropertyValue::unitless(ScalarValue::Boolean(true)),
                },
            },
            ResolvedProcedureParameter {
                id: local("task::parameter::label"),
                value: ProcedureValue::Scalar {
                    value: PropertyValue::unitless(ScalarValue::Text("sample-a".to_owned())),
                },
            },
            ResolvedProcedureParameter {
                id: local("task::parameter::ontology"),
                value: ProcedureValue::Scalar {
                    value: PropertyValue::unitless(ScalarValue::Iri(
                        AbsoluteIri::new("https://example.org/term").unwrap(),
                    )),
                },
            },
        ];
        let outputs = [local("product")];
        let materials = [ResolvedProcedureMaterial {
            id: local("task::material::buffer"),
            symbol: "buffer-stock".to_owned(),
        }];
        let intent = crate::workflow::ir::synthetic_intent("https://example.org/action");
        let context = ProcedureProgramBuildContext {
            intent: &intent,
            input_count: 1,
            outputs: &outputs,
            parameters: &parameters,
            materials: &materials,
        };
        let rendered = evaluate_procedure_template(
            &json!({
                "load": {
                    "input": {"$lab": {"kind": "input", "index": 0}},
                    "output": {"$lab": {"kind": "output", "id": "product"}},
                    "material": {"$lab": {"kind": "material", "id": "buffer"}}
                },
                "cycles": {"$lab": {"kind": "integer", "id": "cycles"}},
                "label": {"$lab": {"kind": "text", "id": "label"}},
                "predicate": {"$lab": {"kind": "boolean", "id": "predicate"}},
                "ontology": {"$lab": {"kind": "iri", "id": "ontology"}},
                "typed": {"$lab": {"kind": "parameter", "id": "cycles"}},
                "intent": {"$lab": {"kind": "intent"}}
            }),
            &context,
        )
        .unwrap();

        assert_eq!(rendered["load"]["input"], 0);
        assert_eq!(rendered["load"]["output"], "product");
        assert_eq!(rendered["load"]["material"], "task::material::buffer");
        assert_eq!(rendered["cycles"], 30);
        assert_eq!(rendered["label"], "sample-a");
        assert_eq!(rendered["predicate"], true);
        assert_eq!(rendered["ontology"], "https://example.org/term");
        assert_eq!(rendered["typed"]["kind"], "scalar");
        assert_eq!(rendered["typed"]["value"]["value"]["value"], "30");
        assert_eq!(rendered["intent"]["source"]["module"], "compiler.synthetic");
    }

    #[test]
    fn malformed_reserved_slots_and_wrong_projections_fail_at_the_exact_path() {
        let parameters = [ResolvedProcedureParameter {
            id: local("task::parameter::label"),
            value: ProcedureValue::Scalar {
                value: PropertyValue::unitless(ScalarValue::Text("sample-a".to_owned())),
            },
        }];
        let intent = crate::workflow::ir::synthetic_intent("https://example.org/action");
        let context = ProcedureProgramBuildContext {
            intent: &intent,
            input_count: 0,
            outputs: &[],
            parameters: &parameters,
            materials: &[],
        };

        let malformed = evaluate_procedure_template(
            &json!({"nested": {"$lab": {"kind": "text", "id": "label"}, "extra": 1}}),
            &context,
        )
        .unwrap_err();
        assert!(malformed.to_string().contains("$[\"nested\"]"));
        assert!(malformed.to_string().contains("cannot have siblings"));

        let mismatch = evaluate_procedure_template(
            &json!({"$lab": {"kind": "integer", "id": "label"}}),
            &context,
        )
        .unwrap_err();
        assert!(
            mismatch
                .to_string()
                .contains("`label` is text, not integer")
        );

        let unknown = evaluate_procedure_template(
            &json!({"$lab": {"kind": "output", "id": "missing"}}),
            &context,
        )
        .unwrap_err();
        assert!(
            unknown
                .to_string()
                .contains("output `missing` is not declared")
        );
    }
}

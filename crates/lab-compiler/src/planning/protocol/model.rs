use std::collections::BTreeMap;
use std::fmt;

use super::AcceptanceCriterion;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Design,
    Material,
    Evidence,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanValue {
    pub name: String,
    pub kind: ValueKind,
}

impl PlanValue {
    pub fn new(name: impl Into<String>, kind: ValueKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }

    pub fn design(name: impl Into<String>) -> Self {
        Self::new(name, ValueKind::Design)
    }

    pub fn material(name: impl Into<String>) -> Self {
        Self::new(name, ValueKind::Material)
    }

    pub fn evidence(name: impl Into<String>) -> Self {
        Self::new(name, ValueKind::Evidence)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Provision,
    Synthesize,
    Assemble,
    Transform,
    Recover,
    Select,
    Screen,
    Grow,
    Purify,
    Sample,
    Sequence,
    Quantify,
    Accept,
}

impl fmt::Display for OperationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Provision => "provision inventory material",
            Self::Synthesize => "synthesize DNA",
            Self::Assemble => "assemble construct",
            Self::Transform => "transform host",
            Self::Recover => "recover transformed cells",
            Self::Select => "select colonies",
            Self::Screen => "screen candidate colonies",
            Self::Grow => "grow selected clone",
            Self::Purify => "purify plasmid DNA",
            Self::Sample => "take verification aliquot",
            Self::Sequence => "sequence verification aliquot",
            Self::Quantify => "measure concentration and volume",
            Self::Accept => "evaluate acceptance criteria",
        };
        f.write_str(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub operation: OperationKind,
    pub inputs: Vec<String>,
    pub outputs: Vec<PlanValue>,
    pub parameters: BTreeMap<String, String>,
}

impl PlanStep {
    pub fn new(
        id: impl Into<String>,
        operation: OperationKind,
        inputs: impl IntoIterator<Item = impl Into<String>>,
        outputs: impl IntoIterator<Item = PlanValue>,
    ) -> Self {
        Self {
            id: id.into(),
            operation,
            inputs: inputs.into_iter().map(Into::into).collect(),
            outputs: outputs.into_iter().collect(),
            parameters: BTreeMap::new(),
        }
    }

    pub fn with_parameter(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(name.into(), value.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceObligation {
    pub criterion: AcceptanceCriterion,
    pub evidence_step: String,
    pub evidence_value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolPlan {
    pub artifact: String,
    pub lab_profile: String,
    pub initial_values: Vec<PlanValue>,
    pub steps: Vec<PlanStep>,
    pub acceptance: Vec<AcceptanceObligation>,
}

use std::fmt::{self, Display};
use std::str::FromStr;

use thiserror::Error;

use crate::stages::IrStage;

/// A discoverable compiler pass and the stage contract it preserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassInfo {
    pub name: &'static str,
    pub summary: &'static str,
    pub input: IrStage,
    pub output: IrStage,
}

const MATERIAL_LINEARITY: PassInfo = PassInfo {
    name: "protocol-check-material-linearity",
    summary: "require every physical material value to have at most one consumer",
    input: IrStage::TargetSelectedProtocol,
    output: IrStage::TargetSelectedProtocol,
};

const REGISTERED_PASSES: [PassInfo; 1] = [MATERIAL_LINEARITY];

/// Return the target-independent passes available to textual IR tooling.
pub fn registered_passes() -> &'static [PassInfo] {
    &REGISTERED_PASSES
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegisteredPass {
    CheckMaterialLinearity,
}

impl RegisteredPass {
    pub(crate) fn info(self) -> &'static PassInfo {
        match self {
            Self::CheckMaterialLinearity => &MATERIAL_LINEARITY,
        }
    }
}

impl FromStr for RegisteredPass {
    type Err = PassPipelineError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "protocol-check-material-linearity" => Ok(Self::CheckMaterialLinearity),
            name => Err(PassPipelineError::UnknownPass {
                name: name.to_owned(),
                available: registered_passes()
                    .iter()
                    .map(|pass| pass.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        }
    }
}

/// A textual, module-anchored sequence of registered Lab Compiler passes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PassPipeline {
    passes: Vec<RegisteredPass>,
}

impl PassPipeline {
    pub fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn passes(&self) -> &[RegisteredPass] {
        &self.passes
    }

    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }
}

impl FromStr for PassPipeline {
    type Err = PassPipelineError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        let body = if let Some(body) = value.strip_prefix("builtin.module(") {
            body.strip_suffix(')')
                .ok_or(PassPipelineError::MalformedModulePipeline)?
        } else {
            value
        };

        if body.trim().is_empty() {
            return Ok(Self::empty());
        }

        let passes = body
            .split(',')
            .map(str::trim)
            .map(RegisteredPass::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { passes })
    }
}

impl Display for PassPipeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "builtin.module(")?;
        for (index, pass) in self.passes.iter().enumerate() {
            if index != 0 {
                write!(formatter, ",")?;
            }
            write!(formatter, "{}", pass.info().name)?;
        }
        write!(formatter, ")")
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PassPipelineError {
    #[error("malformed module pass pipeline; expected 'builtin.module(...)'")]
    MalformedModulePipeline,
    #[error("unknown pass '{name}'; available passes: {available}")]
    UnknownPass { name: String, available: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_module_anchored_pipelines() {
        let pipeline = PassPipeline::from_str(
            "builtin.module(protocol-check-material-linearity,protocol-check-material-linearity)",
        )
        .unwrap();

        assert_eq!(pipeline.passes().len(), 2);
        assert_eq!(
            pipeline.to_string(),
            "builtin.module(protocol-check-material-linearity,protocol-check-material-linearity)"
        );
    }

    #[test]
    fn reports_unknown_passes_with_the_registry_contents() {
        let error = PassPipeline::from_str("does-not-exist").unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown pass 'does-not-exist'; available passes: protocol-check-material-linearity"
        );
    }
}

use std::fmt::{self, Display};
use std::str::FromStr;

/// A verifier-valid boundary in the current Lab Compiler lowering pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrStage {
    /// Target-neutral biological artifact intent expressed only in Design IR.
    Design,
    /// Target-selected Protocol IR plus the retained Design value it currently consumes.
    TargetSelectedProtocol,
}

impl Display for IrStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Design => "design",
            Self::TargetSelectedProtocol => "target-selected-protocol",
        })
    }
}

impl FromStr for IrStage {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "design" => Ok(Self::Design),
            "target-selected-protocol" => Ok(Self::TargetSelectedProtocol),
            other => Err(format!(
                "unknown IR stage '{other}'; expected design or target-selected-protocol"
            )),
        }
    }
}

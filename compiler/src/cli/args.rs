use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "labc",
    version,
    about = "Lab Compiler: compile biological specifications for laboratories"
)]
pub(crate) struct Cli {
    /// Lab Lang artifact specification to compile.
    pub(crate) source: PathBuf,
    /// Compiler representation to write to standard output.
    #[arg(long, value_enum, default_value_t = Emit::Human)]
    pub(crate) emit: Emit,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Emit {
    Human,
    SpecificationJson,
    DesignIr,
    #[value(name = "target-ir", alias = "ir")]
    TargetIr,
    PlanJson,
    Simulation,
}

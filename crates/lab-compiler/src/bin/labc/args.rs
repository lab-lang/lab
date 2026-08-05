use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "labc",
    version,
    about = "Lab Compiler: compile biological specifications for laboratories"
)]
pub(crate) struct Cli {
    /// Lab Lang source module to compile.
    pub(crate) source: PathBuf,
    /// Compiler representation to write to standard output.
    #[arg(long, value_enum, default_value_t = Emit::Human)]
    pub(crate) emit: Emit,
    /// Directory written by --emit automation-bundle.
    #[arg(long)]
    pub(crate) output_dir: Option<PathBuf>,
    /// JSON inventory used by dependency-plan and full-build-bundle.
    #[arg(long)]
    pub(crate) inventory: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Emit {
    Human,
    SourceAst,
    ModuleIr,
    AutomationJson,
    ManualProtocol,
    OpentronsAssembly,
    OpentronsTransformation,
    OpentronsPlating,
    AutomationBundle,
    DependencyPlan,
    FullBuildBundle,
}

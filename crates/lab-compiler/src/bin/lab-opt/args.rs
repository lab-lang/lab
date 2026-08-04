use std::path::PathBuf;

use clap::{ArgAction, Parser};
use lab_compiler::{IrStage, PassPipeline};

#[derive(Debug, Parser)]
#[command(
    name = "lab-opt",
    version,
    about = "Parse, verify, and transform textual Lab Compiler IR"
)]
pub(crate) struct Cli {
    /// Textual Pliron input file, or '-' for standard input.
    #[arg(default_value = "-")]
    pub(crate) input: String,

    /// Module-anchored pipeline, for example builtin.module(protocol-check-material-linearity).
    #[arg(long, default_value = "builtin.module()")]
    pub(crate) pass_pipeline: PassPipeline,

    /// Require the input to satisfy this Lab Compiler stage contract.
    #[arg(long)]
    pub(crate) input_stage: Option<IrStage>,

    /// Verify the complete IR before and after every pass.
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub(crate) verify_each: bool,

    /// Capture the IR before every pass.
    #[arg(long)]
    pub(crate) print_before_all: bool,

    /// Capture the IR after every pass.
    #[arg(long)]
    pub(crate) print_after_all: bool,

    /// Directory for requested before/after pass snapshots.
    #[arg(long)]
    pub(crate) ir_printing_dir: Option<PathBuf>,

    /// Print the registered target-independent passes and exit.
    #[arg(long)]
    pub(crate) list_passes: bool,
}

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser};
use lab_compiler::{CompilerSession, IrStage, PassPipeline, SessionOptions, registered_passes};

#[derive(Debug, Parser)]
#[command(
    name = "lab-opt",
    version,
    about = "Parse, verify, and transform textual Lab Compiler IR"
)]
struct Cli {
    /// Textual Pliron input file, or '-' for standard input.
    #[arg(default_value = "-")]
    input: String,

    /// Module-anchored pipeline, for example builtin.module(check-material-linearity).
    #[arg(long, default_value = "builtin.module()")]
    pass_pipeline: PassPipeline,

    /// Require the input to satisfy this Lab Compiler stage contract.
    #[arg(long)]
    input_stage: Option<IrStage>,

    /// Verify the complete IR before and after every pass.
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    verify_each: bool,

    /// Capture the IR before every pass.
    #[arg(long)]
    print_before_all: bool,

    /// Capture the IR after every pass.
    #[arg(long)]
    print_after_all: bool,

    /// Directory for requested before/after pass snapshots.
    #[arg(long)]
    ir_printing_dir: Option<PathBuf>,

    /// Print the registered facility-independent passes and exit.
    #[arg(long)]
    list_passes: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.list_passes {
        for pass in registered_passes() {
            println!(
                "{}\t{} -> {}\t{}",
                pass.name, pass.input, pass.output, pass.summary
            );
        }
        return Ok(());
    }

    let source = read_input(&cli.input)?;
    let mut session = CompilerSession::new(SessionOptions {
        verify_each: cli.verify_each,
        print_before_all: cli.print_before_all,
        print_after_all: cli.print_after_all,
        ir_printing_dir: cli.ir_printing_dir,
        ..SessionOptions::default()
    });
    session
        .parse_ir(&source)
        .with_context(|| input_context("parse", &cli.input))?;
    if let Some(stage) = cli.input_stage {
        session
            .verify_stage(stage)
            .with_context(|| input_context("verify", &cli.input))?;
    } else {
        session
            .detect_stage()
            .with_context(|| input_context("verify", &cli.input))?;
    }
    session
        .run_pass_pipeline(&cli.pass_pipeline)
        .with_context(|| format!("failed to run pass pipeline {}", cli.pass_pipeline))?;
    print!("{}", session.ir()?);
    Ok(())
}

fn read_input(input: &str) -> Result<String> {
    if input == "-" {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .context("failed to read compiler IR from standard input")?;
        Ok(source)
    } else {
        fs::read_to_string(input).with_context(|| format!("failed to read {input}"))
    }
}

fn input_context(action: &str, input: &str) -> String {
    if input == "-" {
        format!("failed to {action} compiler IR from standard input")
    } else {
        format!("failed to {action} {input}")
    }
}

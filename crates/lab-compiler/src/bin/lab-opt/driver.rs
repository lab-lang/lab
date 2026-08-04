use std::fs;
use std::io::{self, Read};

use anyhow::{Context, Result};
use clap::Parser;
use lab_compiler::{CompilerSession, SessionOptions, registered_passes};

use super::args::Cli;

pub(crate) fn run() -> Result<()> {
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

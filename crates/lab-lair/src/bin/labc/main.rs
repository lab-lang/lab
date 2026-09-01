use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use lab_lair::program::PortableLairProgram;
use lab_lair::{compile_module, parse_module, render_checked_module};

#[derive(Debug, Parser)]
#[command(
    name = "labc",
    version,
    about = "Lab Compiler: compile biological specifications for laboratories"
)]
struct Cli {
    /// Lab Lang source module to compile.
    source: PathBuf,
    /// Compiler representation to write to standard output.
    #[arg(long, value_enum, default_value_t = Emit::Human)]
    emit: Emit,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Emit {
    Human,
    SourceAst,
    ModuleIr,
    DesignIntentLair,
    RefinedLair,
    PlanningProblem,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let text = std::fs::read_to_string(&cli.source)
        .with_context(|| format!("failed to read {}", cli.source.display()))?;
    if matches!(cli.emit, Emit::SourceAst) {
        let module = parse_module(&text)
            .with_context(|| format!("failed to parse {}", cli.source.display()))?;
        println!("{}", serde_json::to_string_pretty(&module)?);
        return Ok(());
    }
    if matches!(cli.emit, Emit::ModuleIr) {
        let module = compile_module(&text)
            .with_context(|| format!("failed to compile module {}", cli.source.display()))?;
        println!("{}", serde_json::to_string_pretty(&module)?);
        return Ok(());
    }
    if matches!(cli.emit, Emit::Human) {
        let module = compile_module(&text)
            .with_context(|| format!("failed to compile module {}", cli.source.display()))?;
        print!("{}", render_checked_module(&module));
        return Ok(());
    }

    let checked = compile_module(&text)
        .with_context(|| format!("failed to check build module {}", cli.source.display()))?;
    let lair = PortableLairProgram::lower(&checked)
        .with_context(|| format!("failed to lower LAIR for {}", cli.source.display()))?;
    if matches!(cli.emit, Emit::DesignIntentLair) {
        print!("{}", lair.ir());
        return Ok(());
    }
    let refined = lair
        .refine_standard_methods()
        .with_context(|| format!("failed to refine methods for {}", cli.source.display()))?;
    match cli.emit {
        Emit::RefinedLair => print!("{}", refined.ir()),
        Emit::PlanningProblem => println!(
            "{}",
            serde_json::to_string_pretty(&refined.planning_problem()?)?
        ),
        Emit::Human | Emit::SourceAst | Emit::ModuleIr | Emit::DesignIntentLair => unreachable!(),
    }
    Ok(())
}

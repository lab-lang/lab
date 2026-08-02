use anyhow::{Context, Result};
use clap::Parser;
use labc::{Compiler, CompilerSession, IrStage, LabProfile, parse, render_human, simulate};

use super::args::{Cli, Emit};

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let text = std::fs::read_to_string(&cli.source)
        .with_context(|| format!("failed to read {}", cli.source.display()))?;
    let specification =
        parse(&text).with_context(|| format!("failed to parse {}", cli.source.display()))?;
    match cli.emit {
        Emit::SpecificationJson => {
            println!("{}", serde_json::to_string_pretty(&specification)?);
        }
        Emit::DesignIr => {
            let mut session = CompilerSession::default();
            session.import_specification(&specification)?;
            session.verify_stage(IrStage::Design)?;
            print!("{}", session.ir()?);
        }
        Emit::Human | Emit::TargetIr | Emit::PlanJson | Emit::Simulation => {
            let compilation = Compiler
                .compile(&specification, &LabProfile::reference())
                .with_context(|| format!("failed to compile {}", cli.source.display()))?;
            match cli.emit {
                Emit::Human => {
                    print!("{}", render_human(compilation.plan()));
                }
                Emit::TargetIr => print!("{}", compilation.ir()),
                Emit::PlanJson => {
                    println!("{}", serde_json::to_string_pretty(compilation.plan())?);
                }
                Emit::Simulation => {
                    let trace = simulate(compilation.plan())?;
                    println!("{}", serde_json::to_string_pretty(&trace)?);
                }
                Emit::SpecificationJson | Emit::DesignIr => unreachable!(),
            }
        }
    }
    Ok(())
}

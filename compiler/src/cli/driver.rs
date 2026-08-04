use anyhow::{Context, Result};
use clap::Parser;
use labc::{
    Compiler, CompilerSession, IrStage, LabProfile, ParseError, compile_module, parse,
    parse_module, render_checked_module, render_human, simulate,
};

use super::args::{Cli, Emit};

pub fn run() -> Result<()> {
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

    let specification = match parse(&text) {
        Ok(specification) => specification,
        Err(ParseError::Unsupported { .. }) if matches!(cli.emit, Emit::Human) => {
            let module = compile_module(&text)
                .with_context(|| format!("failed to compile module {}", cli.source.display()))?;
            print!("{}", render_checked_module(&module));
            return Ok(());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to lower {}", cli.source.display()));
        }
    };
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
                Emit::SourceAst | Emit::ModuleIr | Emit::SpecificationJson | Emit::DesignIr => {
                    unreachable!()
                }
            }
        }
        Emit::SourceAst | Emit::ModuleIr => unreachable!(),
    }
    Ok(())
}

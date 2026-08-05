use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use lab_compiler::backend::Backend;
use lab_compiler::backend::opentrons_ot2::{
    Ot2Backend, compile_dependency_build, emit_program, lower_build,
};
use lab_compiler::planning::BuildInventory;
use lab_compiler::{compile_module, parse_module, render_checked_module};

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
    /// Directory written by --emit automation-bundle.
    #[arg(long)]
    output_dir: Option<PathBuf>,
    /// JSON inventory used by dependency-plan and full-build-bundle.
    #[arg(long)]
    inventory: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Emit {
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
    if matches!(
        cli.emit,
        Emit::AutomationJson
            | Emit::ManualProtocol
            | Emit::OpentronsAssembly
            | Emit::OpentronsTransformation
            | Emit::OpentronsPlating
            | Emit::AutomationBundle
            | Emit::DependencyPlan
            | Emit::FullBuildBundle
    ) {
        let checked = compile_module(&text)
            .with_context(|| format!("failed to check build module {}", cli.source.display()))?;
        if matches!(cli.emit, Emit::DependencyPlan | Emit::FullBuildBundle) {
            let build = lower_build(&checked)
                .with_context(|| format!("failed to lower OT-2 build {}", cli.source.display()))?;
            let inventory = if let Some(path) = &cli.inventory {
                let contents = std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read inventory {}", path.display()))?;
                serde_json::from_str::<BuildInventory>(&contents)
                    .with_context(|| format!("failed to parse inventory {}", path.display()))?
            } else {
                BuildInventory::default()
            };
            let bundle = compile_dependency_build(&build, &inventory).with_context(|| {
                format!(
                    "failed to compile dependency build {}",
                    cli.source.display()
                )
            })?;
            match cli.emit {
                Emit::DependencyPlan => print!("{}", bundle.manifest_json()?),
                Emit::FullBuildBundle => {
                    let output_dir = cli
                        .output_dir
                        .as_ref()
                        .context("--emit full-build-bundle requires --output-dir <directory>")?;
                    std::fs::create_dir_all(output_dir).with_context(|| {
                        format!("failed to create output directory {}", output_dir.display())
                    })?;
                    for artifact in bundle.artifacts().iter() {
                        let path = output_dir.join(artifact.path());
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent).with_context(|| {
                                format!("failed to create output directory {}", parent.display())
                            })?;
                        }
                        std::fs::write(&path, artifact.contents())
                            .with_context(|| format!("failed to write {}", path.display()))?;
                    }
                    println!(
                        "Wrote dependency-driven build bundle to {}",
                        output_dir.display()
                    );
                }
                _ => unreachable!(),
            }
            return Ok(());
        }

        let program = Ot2Backend
            .compile(&checked)
            .with_context(|| format!("failed to compile OT-2 build {}", cli.source.display()))?;
        let bundle = emit_program(&program).with_context(|| {
            format!("failed to compile automated build {}", cli.source.display())
        })?;
        match cli.emit {
            Emit::AutomationJson => print!("{}", bundle.manifest_json()?),
            Emit::ManualProtocol => print!("{}", bundle.manual_protocol()),
            Emit::OpentronsAssembly => print!("{}", bundle.assembly_protocol()),
            Emit::OpentronsTransformation => print!("{}", bundle.transformation_protocol()),
            Emit::OpentronsPlating => print!("{}", bundle.plating_protocol()),
            Emit::AutomationBundle => {
                let output_dir = cli
                    .output_dir
                    .as_ref()
                    .context("--emit automation-bundle requires --output-dir <directory>")?;
                std::fs::create_dir_all(output_dir).with_context(|| {
                    format!("failed to create output directory {}", output_dir.display())
                })?;
                for artifact in bundle.artifacts().iter() {
                    let path = output_dir.join(artifact.path());
                    std::fs::write(&path, artifact.contents())
                        .with_context(|| format!("failed to write {}", path.display()))?;
                }
                println!("Wrote Lab automation bundle to {}", output_dir.display());
            }
            Emit::DependencyPlan | Emit::FullBuildBundle => unreachable!(),
            _ => unreachable!(),
        }
        return Ok(());
    }

    match cli.emit {
        Emit::Human
        | Emit::SourceAst
        | Emit::ModuleIr
        | Emit::AutomationJson
        | Emit::ManualProtocol
        | Emit::OpentronsAssembly
        | Emit::OpentronsTransformation
        | Emit::OpentronsPlating
        | Emit::AutomationBundle
        | Emit::DependencyPlan
        | Emit::FullBuildBundle => unreachable!(),
    }
}

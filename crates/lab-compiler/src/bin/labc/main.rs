use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use lab_compiler::ProtocolLairProgram;
use lab_compiler::backend::Backend;
use lab_compiler::backend::opentrons::flex::{FlexAdapterProfile, FlexBackend};
use lab_compiler::backend::opentrons::ot2::{Ot2AdapterProfile, Ot2Backend};
use lab_compiler::planning::{BuildInventory, LegacyBuildInventory};
use lab_compiler::{PortableLairProgram, compile_module, parse_module, render_checked_module};

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
    /// Explicit adapter implementation used by low-level backend emission.
    #[arg(long, default_value = "opentrons.ot2")]
    adapter: String,
    /// TOML operational profile for the explicitly selected adapter.
    #[arg(long)]
    adapter_profile: Option<PathBuf>,
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

enum AdapterProfile {
    Ot2(Ot2AdapterProfile),
    Flex(FlexAdapterProfile),
}

fn parse_adapter_profile(driver: &str, name: &str, contents: &str) -> Result<AdapterProfile> {
    match driver {
        "opentrons.ot2" => Ok(AdapterProfile::Ot2(Ot2AdapterProfile::parse(
            name, contents,
        )?)),
        "opentrons.flex" => Ok(AdapterProfile::Flex(FlexAdapterProfile::parse(
            name, contents,
        )?)),
        other => bail!(
            "adapter '{other}' does not support this low-level emitter; known adapters are 'opentrons.ot2' and 'opentrons.flex'"
        ),
    }
}

/// A build emits a stage protocol only when it realizes an artifact that
/// reaches that stage.
fn print_stage(stage: &str, protocol: Option<&str>) -> Result<()> {
    let protocol = protocol.with_context(|| format!("this build has no {stage} stage"))?;
    print!("{protocol}");
    Ok(())
}

fn write_bundle(
    artifacts: &lab_compiler::ArtifactBundle,
    output_dir: &std::path::Path,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output directory {}", output_dir.display()))?;
    for artifact in artifacts.iter() {
        let path = output_dir.join(artifact.path());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
        std::fs::write(&path, artifact.contents())
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn load_inventory(cli: &Cli) -> Result<BuildInventory> {
    if let Some(path) = &cli.inventory {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read inventory {}", path.display()))?;
        serde_json::from_str::<LegacyBuildInventory>(&contents)
            .map(BuildInventory::LegacySymbols)
            .with_context(|| format!("failed to parse legacy inventory {}", path.display()))
    } else {
        Ok(BuildInventory::default())
    }
}

fn emit_ot2(cli: &Cli, protocol: &ProtocolLairProgram, profile: Ot2AdapterProfile) -> Result<()> {
    use lab_compiler::backend::opentrons::ot2::{compile_dependency_build, emit_program};

    if matches!(cli.emit, Emit::DependencyPlan | Emit::FullBuildBundle) {
        let inventory = load_inventory(cli)?;
        let bundle =
            compile_dependency_build(protocol, &profile, &inventory).with_context(|| {
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
                write_bundle(bundle.artifacts(), output_dir)?;
                println!(
                    "Wrote dependency-driven build bundle to {}",
                    output_dir.display()
                );
            }
            _ => unreachable!(),
        }
        return Ok(());
    }

    let backend = Ot2Backend::new(profile);
    let program = backend
        .compile(protocol)
        .with_context(|| format!("failed to compile OT-2 build {}", cli.source.display()))?;
    let bundle = emit_program(&program)
        .with_context(|| format!("failed to compile automated build {}", cli.source.display()))?;
    match cli.emit {
        Emit::AutomationJson => print!("{}", bundle.manifest_json()?),
        Emit::ManualProtocol => print!("{}", bundle.manual_protocol()),
        Emit::OpentronsAssembly => print_stage("assembly", bundle.assembly_protocol())?,
        Emit::OpentronsTransformation => {
            print_stage("transformation", bundle.transformation_protocol())?
        }
        Emit::OpentronsPlating => print_stage("plating", bundle.plating_protocol())?,
        Emit::AutomationBundle => {
            let output_dir = cli
                .output_dir
                .as_ref()
                .context("--emit automation-bundle requires --output-dir <directory>")?;
            write_bundle(bundle.artifacts(), output_dir)?;
            println!("Wrote Lab automation bundle to {}", output_dir.display());
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn emit_flex(cli: &Cli, protocol: &ProtocolLairProgram, profile: FlexAdapterProfile) -> Result<()> {
    use lab_compiler::backend::opentrons::flex::{compile_dependency_build, emit_program};

    if matches!(cli.emit, Emit::DependencyPlan | Emit::FullBuildBundle) {
        let inventory = load_inventory(cli)?;
        let bundle =
            compile_dependency_build(protocol, &profile, &inventory).with_context(|| {
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
                write_bundle(bundle.artifacts(), output_dir)?;
                println!(
                    "Wrote dependency-driven build bundle to {}",
                    output_dir.display()
                );
            }
            _ => unreachable!(),
        }
        return Ok(());
    }

    let backend = FlexBackend::new(profile);
    let program = backend
        .compile(protocol)
        .with_context(|| format!("failed to compile Flex build {}", cli.source.display()))?;
    let bundle = emit_program(&program)
        .with_context(|| format!("failed to compile automated build {}", cli.source.display()))?;
    match cli.emit {
        Emit::AutomationJson => print!("{}", bundle.manifest_json()?),
        Emit::ManualProtocol => print!("{}", bundle.manual_protocol()),
        Emit::OpentronsAssembly => print_stage("assembly", bundle.assembly_protocol())?,
        Emit::OpentronsTransformation => {
            print_stage("transformation", bundle.transformation_protocol())?
        }
        Emit::OpentronsPlating => print_stage("plating", bundle.plating_protocol())?,
        Emit::AutomationBundle => {
            let output_dir = cli
                .output_dir
                .as_ref()
                .context("--emit automation-bundle requires --output-dir <directory>")?;
            write_bundle(bundle.artifacts(), output_dir)?;
            println!("Wrote Lab automation bundle to {}", output_dir.display());
        }
        _ => unreachable!(),
    }
    Ok(())
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
    let protocol = lair.select_protocol().with_context(|| {
        format!(
            "failed to select plasmid-build Protocol LAIR for {}",
            cli.source.display()
        )
    })?;
    let profile = match &cli.adapter_profile {
        Some(path) => {
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read adapter profile {}", path.display()))?;
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .with_context(|| format!("adapter profile {} has no name", path.display()))?;
            parse_adapter_profile(&cli.adapter, name, &contents)
                .with_context(|| format!("failed to load adapter profile {}", path.display()))?
        }
        None => parse_adapter_profile(&cli.adapter, &cli.adapter, "")?,
    };
    match profile {
        AdapterProfile::Ot2(profile) => emit_ot2(&cli, &protocol, profile),
        AdapterProfile::Flex(profile) => emit_flex(&cli, &protocol, profile),
    }
}

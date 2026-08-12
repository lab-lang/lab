use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use lab_compiler::ProtocolLairProgram;
use lab_compiler::backend::Backend;
use lab_compiler::backend::opentrons::flex::{FlexBackend, FlexTargetProfile};
use lab_compiler::backend::opentrons::ot2::{Ot2Backend, Ot2TargetProfile};
use lab_compiler::planning::BuildInventory;
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
    /// TOML target profile describing the bench to compile for.
    #[arg(long)]
    target_profile: Option<PathBuf>,
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
    Labop,
    LabopBundle,
}

/// A target profile parsed for whichever backend it declares. The `backend`
/// key is peeked out of the TOML before committing to a profile schema; an
/// absent key means `opentrons.ot2`, matching that profile schema's default.
enum TargetProfile {
    Ot2(Ot2TargetProfile),
    Flex(FlexTargetProfile),
}

fn parse_target_profile(name: &str, contents: &str) -> Result<TargetProfile> {
    let table = contents
        .parse::<toml::Table>()
        .context("failed to parse target profile")?;
    let backend = table
        .get("target")
        .and_then(|target| target.get("backend"))
        .and_then(|backend| backend.as_str())
        .unwrap_or("opentrons.ot2");
    match backend {
        "opentrons.ot2" => Ok(TargetProfile::Ot2(Ot2TargetProfile::parse(name, contents)?)),
        "opentrons.flex" => Ok(TargetProfile::Flex(FlexTargetProfile::parse(
            name, contents,
        )?)),
        other => bail!(
            "target profile declares backend '{other}', which this toolchain does not provide; known backends are 'opentrons.ot2' and 'opentrons.flex'"
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
        serde_json::from_str::<BuildInventory>(&contents)
            .with_context(|| format!("failed to parse inventory {}", path.display()))
    } else {
        Ok(BuildInventory::default())
    }
}

fn emit_ot2(cli: &Cli, protocol: &ProtocolLairProgram, profile: Ot2TargetProfile) -> Result<()> {
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

fn emit_flex(cli: &Cli, protocol: &ProtocolLairProgram, profile: FlexTargetProfile) -> Result<()> {
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

fn emit_labop(cli: &Cli, protocol: &ProtocolLairProgram) -> Result<()> {
    use lab_compiler::backend::labop::{LabopBackend, emit_program};

    let backend = LabopBackend::new();
    let program = backend
        .compile(protocol)
        .with_context(|| format!("failed to compile LabOP document {}", cli.source.display()))?;
    match cli.emit {
        Emit::Labop => print!("{}", program.document()),
        Emit::LabopBundle => {
            let output_dir = cli
                .output_dir
                .as_ref()
                .context("--emit labop-bundle requires --output-dir <directory>")?;
            let bundle = emit_program(&program)
                .with_context(|| format!("failed to emit LabOP bundle {}", cli.source.display()))?;
            write_bundle(&bundle, output_dir)?;
            println!(
                "Wrote LabOP document with {} statements across {} protocol(s) to {}",
                program.statement_count(),
                program.protocols().len(),
                output_dir.display()
            );
            if !program.omissions().is_empty() {
                println!(
                    "{} omission(s) recorded in labop/omissions.md",
                    program.omissions().len()
                );
            }
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
    // LabOP is an interchange target rather than a bench, so it is emitted
    // from Protocol LAIR alone and never consults a target profile.
    if matches!(cli.emit, Emit::Labop | Emit::LabopBundle) {
        return emit_labop(&cli, &protocol);
    }

    let profile = match &cli.target_profile {
        Some(path) => {
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read target profile {}", path.display()))?;
            // A profile is named by its file, the same way `lab build`
            // resolves one under `targets/`.
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .with_context(|| format!("target profile {} has no name", path.display()))?;
            parse_target_profile(name, &contents)
                .with_context(|| format!("failed to load target profile {}", path.display()))?
        }
        None => TargetProfile::Ot2(Ot2TargetProfile::default()),
    };
    match profile {
        TargetProfile::Ot2(profile) => emit_ot2(&cli, &protocol, profile),
        TargetProfile::Flex(profile) => emit_flex(&cli, &protocol, profile),
    }
}

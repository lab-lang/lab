//! Python source and stub generation from Lab's checked public interfaces.
//!
//! This crate is deliberately pure: callers supply checked module interfaces
//! and receive files. The CLI owns filesystem policy, while the Python wheel
//! uses the same renderer for its bundled standard-library bindings.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use lab_language::manifest::{Export as StandardExport, Library as StandardLibrary};
use lab_language::{ExportKind, ModuleInterface, ResolvedImport};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A checked Lab interface that cannot be represented as one unambiguous Python package.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PythonBindingGenerationError {
    #[error("Lab modules '{first_module}' and '{second_module}' both map to Python module {path}")]
    ModulePathCollision {
        path: PathBuf,
        first_module: String,
        second_module: String,
    },
    #[error(
        "bindings in Lab module '{module}' map both '{first}' and '{second}' to Python name '{python_name}'"
    )]
    NameCollision {
        module: String,
        python_name: String,
        first: String,
        second: String,
    },
    #[error("generated Python files collide at {path}")]
    FilePathCollision { path: PathBuf },
    #[error(
        "binding '{export}' in Lab module '{module}' refers to Python type '{type_name}', which cannot be resolved from its checked imports"
    )]
    UnresolvedType {
        module: String,
        export: String,
        type_name: String,
    },
}

/// One generated source or stub file, relative to the caller's output root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub source: String,
}

/// A generated binding set that cannot be persisted without losing ownership or path safety.
#[derive(Debug, Error)]
pub enum PythonBindingWriteError {
    #[error("failed to {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse generated-file manifest {path}: {source}")]
    ParseManifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize generated-file manifest {path}: {source}")]
    SerializeManifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported Python binding manifest schema {schema_version} in {path}")]
    UnsupportedManifestSchema { path: PathBuf, schema_version: u32 },
    #[error("generated Python binding path '{path}' must be a non-empty relative path")]
    UnsafePath { path: PathBuf },
    #[error("generated Python binding path '{path}' is duplicated")]
    DuplicatePath { path: PathBuf },
    #[error("refusing Python binding path {path} because it traverses symbolic link {link}")]
    SymbolicLink { path: PathBuf, link: PathBuf },
    #[error(
        "refusing to overwrite {path}: it is not owned by the previous Python binding manifest"
    )]
    UnownedFile { path: PathBuf },
}

const PYTHON_BINDINGS_MANIFEST: &str = ".lab-python-bindings.json";
const PYTHON_BINDINGS_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PythonBindingsManifest {
    schema_version: u32,
    files: Vec<PathBuf>,
}

/// Persist a complete generated binding set under `output_root`.
///
/// The adjacent manifest is the ownership boundary: later runs delete only obsolete paths it
/// names. Existing files that are neither owned nor recognizable output from an earlier Lab
/// generator are rejected instead of overwritten. Every current and stale path is validated
/// before the first write, and symbolic-link components are rejected so a manifest cannot escape
/// the selected output tree indirectly.
pub fn write_generated_files(
    output_root: &Path,
    generated: &[GeneratedFile],
) -> Result<(), PythonBindingWriteError> {
    let manifest_path = output_root.join(PYTHON_BINDINGS_MANIFEST);
    reject_symbolic_links(output_root, Path::new(PYTHON_BINDINGS_MANIFEST))?;
    let previous = match fs::read_to_string(&manifest_path) {
        Ok(contents) => {
            serde_json::from_str::<PythonBindingsManifest>(&contents).map_err(|source| {
                PythonBindingWriteError::ParseManifest {
                    path: manifest_path.clone(),
                    source,
                }
            })?
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => PythonBindingsManifest {
            schema_version: PYTHON_BINDINGS_MANIFEST_SCHEMA_VERSION,
            files: Vec::new(),
        },
        Err(source) => {
            return Err(PythonBindingWriteError::Io {
                operation: "read",
                path: manifest_path,
                source,
            });
        }
    };
    if previous.schema_version != PYTHON_BINDINGS_MANIFEST_SCHEMA_VERSION {
        return Err(PythonBindingWriteError::UnsupportedManifestSchema {
            path: manifest_path,
            schema_version: previous.schema_version,
        });
    }

    let mut current = BTreeSet::new();
    for file in generated {
        validate_generated_path(&file.path)?;
        reject_symbolic_links(output_root, &file.path)?;
        if !current.insert(file.path.clone()) {
            return Err(PythonBindingWriteError::DuplicatePath {
                path: file.path.clone(),
            });
        }
    }
    let previous_files = previous.files.iter().cloned().collect::<BTreeSet<_>>();
    for path in &previous.files {
        validate_generated_path(path)?;
        reject_symbolic_links(output_root, path)?;
    }
    for file in generated {
        let target = output_root.join(&file.path);
        if target.exists() && !previous_files.contains(&file.path) {
            let existing =
                fs::read_to_string(&target).map_err(|source| PythonBindingWriteError::Io {
                    operation: "read existing binding",
                    path: target.clone(),
                    source,
                })?;
            if existing != file.source && !recognizable_generated_source(&existing) {
                return Err(PythonBindingWriteError::UnownedFile { path: target });
            }
        }
    }

    fs::create_dir_all(output_root).map_err(|source| PythonBindingWriteError::Io {
        operation: "create directory",
        path: output_root.to_path_buf(),
        source,
    })?;
    for file in generated {
        let target = output_root.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| PythonBindingWriteError::Io {
                operation: "create directory",
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&target, &file.source).map_err(|source| PythonBindingWriteError::Io {
            operation: "write",
            path: target,
            source,
        })?;
    }
    for stale in previous
        .files
        .iter()
        .filter(|path| !current.contains(*path))
    {
        let target = output_root.join(stale);
        match fs::remove_file(&target) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(PythonBindingWriteError::Io {
                    operation: "remove stale binding",
                    path: target,
                    source,
                });
            }
        }
    }

    let manifest = PythonBindingsManifest {
        schema_version: PYTHON_BINDINGS_MANIFEST_SCHEMA_VERSION,
        files: current.into_iter().collect(),
    };
    let mut contents = serde_json::to_string_pretty(&manifest).map_err(|source| {
        PythonBindingWriteError::SerializeManifest {
            path: manifest_path.clone(),
            source,
        }
    })?;
    contents.push('\n');
    fs::write(&manifest_path, contents).map_err(|source| PythonBindingWriteError::Io {
        operation: "write",
        path: manifest_path,
        source,
    })?;
    Ok(())
}

fn validate_generated_path(path: &Path) -> Result<(), PythonBindingWriteError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PythonBindingWriteError::UnsafePath {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn reject_symbolic_links(
    output_root: &Path,
    relative: &Path,
) -> Result<(), PythonBindingWriteError> {
    let target = output_root.join(relative);
    match fs::symlink_metadata(output_root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(PythonBindingWriteError::SymbolicLink {
                path: target,
                link: output_root.to_path_buf(),
            });
        }
        Ok(_) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(PythonBindingWriteError::Io {
                operation: "inspect",
                path: output_root.to_path_buf(),
                source,
            });
        }
    }
    let mut path = output_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PythonBindingWriteError::SymbolicLink {
                    path: target,
                    link: path,
                });
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(PythonBindingWriteError::Io {
                    operation: "inspect",
                    path,
                    source,
                });
            }
        }
    }
    Ok(())
}

fn recognizable_generated_source(source: &str) -> bool {
    const MARKERS: &[&str] = &[
        "# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit.",
        "# Generated by `lab bindings python`. Do not edit.",
        "# Generated from the Lab standard library by `python -m lab.codegen`. Do not edit.",
    ];
    source.lines().any(|line| MARKERS.contains(&line))
}

/// One compiled package module and the imports that produced its interface.
#[derive(Clone, Debug)]
pub struct PackageModule {
    /// Manifest package that owns this module. Dependency modules participate in type resolution
    /// but are not emitted into the requested package.
    pub package: String,
    pub interface: ModuleInterface,
    pub imports: Vec<ResolvedImport>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindingKind {
    ArtifactKind,
    Type,
    Role,
    Facet,
    Value,
    Function,
    Constructor,
    Action,
    Workflow,
}

#[derive(Clone, Debug)]
struct Field {
    name: String,
    ty: String,
    optional: bool,
}

#[derive(Clone, Debug)]
struct TypeParameter {
    name: String,
    bound: Option<String>,
}

#[derive(Clone, Debug)]
struct BindingExport {
    definition_module: String,
    definition_local: String,
    kind: BindingKind,
    source_name: String,
    name: String,
    documentation: String,
    parameters: Vec<TypeParameter>,
    fields: Vec<Field>,
    roles: Vec<String>,
    produces: Option<String>,
    facet_states: Vec<String>,
    operation: Option<String>,
    phrase: Vec<String>,
    optional_clauses: Vec<Vec<String>>,
    inputs: Vec<Field>,
    results: Vec<Field>,
}

#[derive(Clone, Debug)]
struct BindingModule {
    lab_path: String,
    python_path: PathBuf,
    emit: bool,
    standard: bool,
    prelude: bool,
    documentation: String,
    imports: Vec<String>,
    exports: Vec<BindingExport>,
}

/// Generate the Python SDK's bundled standard-library mirror.
///
/// Paths are relative to the `lab` package directory: `std.prelude` becomes
/// `_prelude.py`, while `std.bio.designs` becomes `bio/designs.py`.
pub fn generate_standard_library(
    library: &StandardLibrary,
) -> Result<Vec<GeneratedFile>, PythonBindingGenerationError> {
    let modules = library
        .modules
        .iter()
        .map(|module| standard_module(module, true, false))
        .collect::<Vec<_>>();
    generate(modules, false)
}

/// Generate an installable Python package for one Lab package.
///
/// Paths include the normalized Python package directory, so writing the
/// result to `bindings/python` creates `bindings/python/my_package/...`.
pub fn generate_package(
    package: &str,
    modules: &[PackageModule],
) -> Result<Vec<GeneratedFile>, PythonBindingGenerationError> {
    let mut binding_modules = lab_language::standard_library_manifest()
        .modules
        .iter()
        .map(|module| standard_module(module, false, true))
        .collect::<Vec<_>>();
    binding_modules.extend(modules.iter().map(|module| package_module(package, module)));
    generate(binding_modules, true)
}

fn generate(
    mut modules: Vec<BindingModule>,
    root_package: bool,
) -> Result<Vec<GeneratedFile>, PythonBindingGenerationError> {
    modules.sort_by(|left, right| left.lab_path.cmp(&right.lab_path));
    for module in &mut modules {
        merge_type_constructors(module);
    }
    validate_module_names(&modules)?;
    let definitions = definition_modules(&modules);
    validate_type_references(&modules, &definitions)?;
    let mut generated = Vec::with_capacity(modules.len() * 2 + 4);
    for module in modules.iter().filter(|module| module.emit) {
        generated.push(GeneratedFile {
            path: module.python_path.with_extension("py"),
            source: render_runtime(module),
        });
        generated.push(GeneratedFile {
            path: module.python_path.with_extension("pyi"),
            source: render_stub(module, &definitions),
        });
    }
    generated.extend(package_files(&generated, root_package));
    generated.sort_by(|left, right| left.path.cmp(&right.path));
    for pair in generated.windows(2) {
        if pair[0].path == pair[1].path {
            return Err(PythonBindingGenerationError::FilePathCollision {
                path: pair[0].path.clone(),
            });
        }
    }
    Ok(generated)
}

fn validate_module_names(modules: &[BindingModule]) -> Result<(), PythonBindingGenerationError> {
    let mut paths = BTreeMap::<PathBuf, &str>::new();
    for module in modules.iter().filter(|module| module.emit) {
        if let Some(first_module) = paths.insert(module.python_path.clone(), &module.lab_path) {
            return Err(PythonBindingGenerationError::ModulePathCollision {
                path: module.python_path.clone(),
                first_module: first_module.to_owned(),
                second_module: module.lab_path.clone(),
            });
        }
        validate_export_names(module)?;
    }
    Ok(())
}

fn validate_export_names(module: &BindingModule) -> Result<(), PythonBindingGenerationError> {
    const RESERVED: &[&str] = &[
        "Action",
        "ArtifactKind",
        "Decimal",
        "Effect",
        "Final",
        "Function",
        "Generic",
        "ImportedWorkflow",
        "LAB_MODULE",
        "LabConstructor",
        "LabRole",
        "LabState",
        "LabType",
        "Protocol",
        "Quantity",
        "Symbol",
        "TypeVar",
        "WorkflowCall",
        "__all__",
    ];
    let mut names = RESERVED
        .iter()
        .map(|name| ((*name).to_owned(), "generated binding support".to_owned()))
        .collect::<BTreeMap<_, _>>();
    for export in &module.exports {
        insert_python_name(
            module,
            &mut names,
            python_binding_name(export),
            format!("export '{}'", export.source_name),
        )?;
        for state in &export.facet_states {
            insert_python_name(
                module,
                &mut names,
                python_identifier(state),
                format!("facet state '{state}'"),
            )?;
        }
        let helper = match export.kind {
            BindingKind::Function => Some(format!("_{}Function", pascal(&export.name))),
            BindingKind::Action => Some(format!("_{}Action", pascal(&export.name))),
            BindingKind::Workflow => Some(format!("_{}Workflow", pascal(&export.name))),
            _ => None,
        };
        if let Some(helper) = helper {
            insert_python_name(
                module,
                &mut names,
                helper,
                format!("typing helper for '{}'", export.source_name),
            )?;
        }
        let parameter_names = type_parameter_names(export);
        for name in ordered_type_parameter_names(export, &parameter_names) {
            insert_python_name(
                module,
                &mut names,
                name,
                format!("type parameter for '{}'", export.source_name),
            )?;
        }
    }
    Ok(())
}

fn insert_python_name(
    module: &BindingModule,
    names: &mut BTreeMap<String, String>,
    python_name: String,
    origin: String,
) -> Result<(), PythonBindingGenerationError> {
    if python_name.starts_with("_module_") {
        return Err(PythonBindingGenerationError::NameCollision {
            module: module.lab_path.clone(),
            python_name,
            first: "generated import alias".to_owned(),
            second: origin,
        });
    }
    if let Some(first) = names.insert(python_name.clone(), origin.clone()) {
        return Err(PythonBindingGenerationError::NameCollision {
            module: module.lab_path.clone(),
            python_name,
            first,
            second: origin,
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
struct DefinitionModules {
    exact: BTreeMap<(String, String), Option<PathBuf>>,
    prelude: BTreeMap<String, Option<PathBuf>>,
    standard: BTreeMap<String, Option<PathBuf>>,
}

impl DefinitionModules {
    fn insert(&mut self, module: &BindingModule, name: &str) {
        insert_definition(
            &mut self.exact,
            (module.lab_path.clone(), name.to_owned()),
            module.python_path.clone(),
        );
        if module.prelude {
            insert_definition(
                &mut self.prelude,
                name.to_owned(),
                module.python_path.clone(),
            );
        }
        if module.standard {
            insert_definition(
                &mut self.standard,
                name.to_owned(),
                module.python_path.clone(),
            );
        }
    }

    /// Resolve a visible nominal type exactly the way the checked module did: local declaration,
    /// explicit import, then the implicit standard-library surface. An ambiguous name has no
    /// location instead of binding to whichever module happened to sort first.
    fn resolve<'a>(&'a self, module: &BindingModule, name: &str) -> Option<&'a PathBuf> {
        if let Some(location) = self.exact.get(&(module.lab_path.clone(), name.to_owned())) {
            return location.as_ref();
        }
        let mut imported = None;
        for imported_module in &module.imports {
            let Some(Some(location)) = self.exact.get(&(imported_module.clone(), name.to_owned()))
            else {
                continue;
            };
            if imported.is_some_and(|previous| previous != location) {
                return None;
            }
            imported = Some(location);
        }
        imported
            .or_else(|| self.prelude.get(name).and_then(Option::as_ref))
            .or_else(|| self.standard.get(name).and_then(Option::as_ref))
    }
}

fn insert_definition<K: Ord>(
    definitions: &mut BTreeMap<K, Option<PathBuf>>,
    key: K,
    path: PathBuf,
) {
    match definitions.entry(key) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(Some(path));
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            if entry.get().as_ref() != Some(&path) {
                entry.insert(None);
            }
        }
    }
}

fn definition_modules(modules: &[BindingModule]) -> DefinitionModules {
    let mut definitions = DefinitionModules::default();
    for module in modules {
        for export in &module.exports {
            definitions.insert(module, &export.source_name);
            for state in &export.facet_states {
                definitions.insert(module, state);
            }
            if let Some(produces) = &export.produces {
                definitions.insert(module, produces);
            }
        }
    }
    definitions
}

fn merge_type_constructors(module: &mut BindingModule) {
    let constructors = module
        .exports
        .iter()
        .filter(|export| export.kind == BindingKind::Constructor)
        .map(|export| {
            (
                export.source_name.clone(),
                (export.fields.clone(), export.results.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let type_names = module
        .exports
        .iter()
        .filter(|export| export.kind == BindingKind::Type)
        .map(|export| export.source_name.clone())
        .collect::<BTreeSet<_>>();
    let mut merged = Vec::with_capacity(module.exports.len());
    for mut export in module.exports.drain(..) {
        let was_type = export.kind == BindingKind::Type;
        if was_type && let Some((fields, results)) = constructors.get(&export.source_name) {
            export.fields = fields.clone();
            export.results = results.clone();
        }
        if was_type && !export.fields.is_empty() {
            export.kind = BindingKind::Constructor;
        }
        if !was_type
            && export.kind == BindingKind::Constructor
            && type_names.contains(&export.source_name)
        {
            continue;
        }
        merged.push(export);
    }
    module.exports = merged;
}

fn render_runtime(module: &BindingModule) -> String {
    let mut blocks = vec![
        module_doc(&module.documentation),
        "# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit."
            .to_owned(),
        "# ruff: noqa".to_owned(),
        "from typing import Generic, TypeVar".to_owned(),
        [
            "from lab._effects import Action",
            "from lab._types import LabConstructor, LabRole, LabState, LabType",
            "from lab._vocabulary import ArtifactKind, Function, Symbol",
            "from lab._workflows import ImportedWorkflow",
        ]
        .join("\n"),
    ];
    let parameters = runtime_type_parameter_declarations(module);
    if !parameters.is_empty() {
        blocks.push(parameters);
    }
    blocks.push(format!(
        "LAB_MODULE = {:?}\n\"\"\"The exact Lab module these bindings import.\"\"\"",
        module.lab_path
    ));
    if module.prelude {
        let exported = module
            .exports
            .iter()
            .map(python_binding_name)
            .collect::<BTreeSet<_>>();
        let lines = exported
            .iter()
            .map(|name| format!("    {name:?},"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!("__all__ = [\n{lines}\n]"));
    }
    for export in &module.exports {
        blocks.push(render_runtime_export(export, module));
    }
    join_blocks(blocks)
}

fn render_runtime_export(export: &BindingExport, module: &BindingModule) -> String {
    let definition = definition_literal(export);
    let mut use_paths = module.imports.clone();
    if !module.prelude {
        use_paths.push(module.lab_path.clone());
    }
    let uses = tuple_literal(&use_paths);
    let declaration = match export.kind {
        BindingKind::Type | BindingKind::Role | BindingKind::Constructor => {
            let base = match export.kind {
                BindingKind::Role => "LabRole",
                BindingKind::Constructor => "LabConstructor",
                _ => "LabType",
            };
            let type_parameters = type_parameter_names(export);
            let generic = if type_parameters.is_empty() {
                String::new()
            } else {
                format!(
                    ", Generic[{}]",
                    ordered_type_parameter_names(export, &type_parameters).join(", ")
                )
            };
            let mut body = vec![
                format!("    __lab_name__ = {:?}", export.source_name),
                format!("    __lab_roles__ = {}", tuple_literal(&export.roles)),
                format!("    __lab_definition__ = {definition}"),
                format!("    __lab_uses__ = {uses}"),
            ];
            if export.kind == BindingKind::Constructor {
                body.insert(
                    0,
                    format!("    __lab_fields__ = {}", field_map_literal(&export.fields)),
                );
            }
            if export.kind == BindingKind::Role {
                body.insert(0, format!("    __lab_role__ = {:?}", export.source_name));
            }
            format!(
                "class {}({base}{generic}):\n{}",
                export.name,
                body.join("\n")
            )
        }
        BindingKind::ArtifactKind => {
            let source_produces = export.produces.as_deref().unwrap_or(&export.source_name);
            let produces = python_identifier(source_produces);
            format!(
                "class {produces}(ArtifactKind, LabType):\n    word = {:?}\n    produces = {source_produces:?}\n    definition = {definition}\n    uses = {uses}\n    __lab_name__ = {source_produces:?}\n    __lab_roles__ = {}\n    __lab_definition__ = {definition}\n    __lab_uses__ = {uses}\n    properties = {}",
                export.source_name,
                tuple_literal(&export.roles),
                tuple_literal(
                    &export
                        .fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>()
                )
            )
        }
        BindingKind::Facet => {
            let mut parts = vec![format!(
                "{} = Symbol(name={:?}, uses={uses}, definition={definition})",
                export.name, export.source_name
            )];
            let names = type_parameter_names(export);
            let parameter = ordered_type_parameter_names(export, &names)
                .into_iter()
                .next()
                .expect("facet bindings carry one subject parameter");
            for state in &export.facet_states {
                let python_state = python_identifier(state);
                parts.push(format!(
                    "class {python_state}(LabState, Generic[{parameter}]):\n    __lab_state__ = {state:?}\n    __lab_definition__ = {definition}\n    __lab_uses__ = {uses}"
                ));
            }
            parts.join("\n\n\n")
        }
        BindingKind::Value => format!(
            "{} = Symbol(name={:?}, uses={uses}, definition={definition})",
            export.name, export.source_name
        ),
        BindingKind::Function => format!(
            "{} = Function(\n    name={:?},\n    definition={definition},\n    inputs={},\n    python_inputs={},\n    uses={uses},\n)",
            export.name,
            export.source_name,
            tuple_literal(
                &export
                    .inputs
                    .iter()
                    .map(|field| field.name.clone())
                    .collect::<Vec<_>>()
            ),
            tuple_literal(&python_parameter_names(&export.inputs)),
        ),
        BindingKind::Action => format!(
            "{} = Action(\n    name={:?},\n    definition={definition},\n    operation={:?},\n    phrase={},\n    python_slots={},\n    results={},\n    optional={},\n    uses={uses},\n)",
            export.name,
            export.source_name,
            export.operation.as_deref().unwrap_or(""),
            tuple_literal(&export.phrase),
            tuple_literal(&python_parameter_names(&export.inputs)),
            tuple_literal(
                &export
                    .results
                    .iter()
                    .map(|field| field.name.clone())
                    .collect::<Vec<_>>()
            ),
            nested_tuple_literal(&export.optional_clauses),
        ),
        BindingKind::Workflow => format!(
            "{} = ImportedWorkflow(\n    name={:?},\n    definition={definition},\n    inputs={},\n    python_inputs={},\n    results={},\n    uses={uses},\n)",
            export.name,
            export.source_name,
            tuple_literal(
                &export
                    .inputs
                    .iter()
                    .map(|field| field.name.clone())
                    .collect::<Vec<_>>()
            ),
            tuple_literal(&python_parameter_names(&export.inputs)),
            tuple_literal(
                &export
                    .results
                    .iter()
                    .map(|field| field.name.clone())
                    .collect::<Vec<_>>()
            ),
        ),
    };
    documented(declaration, &export.documentation)
}

fn render_stub(module: &BindingModule, definitions: &DefinitionModules) -> String {
    let mut blocks = vec![
        module_doc(&module.documentation),
        "# Generated from a checked Lab ModuleInterface by `lab bindings python`. Do not edit."
            .to_owned(),
        "# ruff: noqa".to_owned(),
        "from __future__ import annotations".to_owned(),
        "from typing import Any, Final, Generic, Protocol, TypeVar".to_owned(),
        [
            "from lab._effects import Effect",
            "from lab._expressions import Decimal, Quantity",
            "from lab._types import LabConstructor, LabRole, LabState, LabType",
            "from lab._vocabulary import ArtifactKind, Function, Symbol",
            "from lab._workflows import WorkflowCall",
        ]
        .join("\n"),
    ];
    let aliases = type_aliases(module, definitions);
    if !aliases.is_empty() {
        blocks.push(
            aliases
                .iter()
                .map(|(path, alias)| {
                    let path = dotted_python_path(path);
                    let import = if module.lab_path.starts_with("std.") {
                        format!("lab.{path}")
                    } else {
                        path
                    };
                    format!("import {import} as {alias}")
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let parameters = stub_type_parameter_declarations(module, definitions, &aliases);
    if !parameters.is_empty() {
        blocks.push(parameters);
    }
    blocks.push("LAB_MODULE: Final[str]".to_owned());
    for export in &module.exports {
        blocks.push(render_stub_export(export, module, definitions, &aliases));
    }
    join_blocks(blocks)
}

fn render_stub_export(
    export: &BindingExport,
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
) -> String {
    match export.kind {
        BindingKind::Type | BindingKind::Role | BindingKind::Constructor => {
            let base = match export.kind {
                BindingKind::Role => "LabRole",
                BindingKind::Constructor => "LabConstructor",
                _ => "LabType",
            };
            let type_parameters = type_parameter_names(export);
            let roles = role_bases(export, module, definitions, aliases);
            let mut bases = if export.kind == BindingKind::Type && !roles.is_empty() {
                // Every generated role derives from LabType. Listing LabType before one of its
                // subclasses gives Python and static checkers an impossible C3 ordering, while
                // the role alone already preserves the nominal LabType relationship.
                roles
            } else {
                let mut bases = vec![base.to_owned()];
                bases.extend(roles);
                bases
            };
            if !type_parameters.is_empty() {
                bases.push(format!(
                    "Generic[{}]",
                    ordered_type_parameter_names(export, &type_parameters).join(", ")
                ));
            }
            let declaration = if export.kind == BindingKind::Constructor {
                format!(
                    "class {}({}):\n    def __new__({}) -> {}: ...",
                    export.name,
                    bases.join(", "),
                    constructor_signature(export, module, definitions, aliases),
                    constructor_result_type(export, module, definitions, aliases)
                )
            } else {
                format!("class {}({}): ...", export.name, bases.join(", "))
            };
            documented(declaration, &export.documentation)
        }
        BindingKind::ArtifactKind => {
            let produces =
                python_identifier(export.produces.as_deref().unwrap_or(&export.source_name));
            let roles = role_bases(export, module, definitions, aliases);
            let mut bases = vec!["ArtifactKind".to_owned()];
            if roles.is_empty() {
                bases.push("LabType".to_owned());
            } else {
                // A role is already a LabType. Omitting the redundant base keeps ArtifactKind's
                // declaration helpers while giving the stub a valid multiple-inheritance MRO.
                bases.extend(roles);
            }
            documented(
                format!("class {produces}({}): ...", bases.join(", ")),
                &export.documentation,
            )
        }
        BindingKind::Facet => {
            let mut parts = vec![format!("{}: Final[Symbol]", export.name)];
            let names = type_parameter_names(export);
            let parameter = ordered_type_parameter_names(export, &names)
                .into_iter()
                .next()
                .expect("facet bindings carry one subject parameter");
            parts.extend(export.facet_states.iter().map(|state| {
                format!(
                    "class {}(LabState, Generic[{}]): ...",
                    python_identifier(state),
                    parameter
                )
            }));
            documented(parts.join("\n\n\n"), &export.documentation)
        }
        BindingKind::Value => documented(
            format!("{}: Final[Symbol]", export.name),
            &export.documentation,
        ),
        BindingKind::Function => {
            let protocol = format!("_{}Function", pascal(&export.name));
            let type_parameters = type_parameter_names(export);
            let signature = signature(
                &export.inputs,
                module,
                definitions,
                aliases,
                &type_parameters,
            );
            documented(
                format!(
                    "class {protocol}(Protocol):\n    def __call__({signature}) -> {}: ...\n\n{}: Final[{protocol}]",
                    python_type_with_parameters(
                        export
                            .results
                            .first()
                            .map_or("object", |field| field.ty.as_str()),
                        module,
                        definitions,
                        aliases,
                        &type_parameters,
                    ),
                    export.name
                ),
                &export.documentation,
            )
        }
        BindingKind::Action | BindingKind::Workflow => {
            let suffix = if export.kind == BindingKind::Action {
                "Action"
            } else {
                "Workflow"
            };
            let protocol = format!("_{}{suffix}", pascal(&export.name));
            let type_parameters = type_parameter_names(export);
            let result = result_type(
                &export.results,
                module,
                definitions,
                aliases,
                &type_parameters,
            );
            let effect = if export.kind == BindingKind::Action {
                "Effect"
            } else {
                "WorkflowCall"
            };
            let signature = signature(
                &export.inputs,
                module,
                definitions,
                aliases,
                &type_parameters,
            );
            documented(
                format!(
                    "class {protocol}(Protocol):\n    @property\n    def definition(self) -> tuple[str, str]: ...\n    def __call__({signature}) -> {effect}[{result}]: ...\n\n{}: Final[{protocol}]",
                    export.name
                ),
                &export.documentation,
            )
        }
    }
}

fn synthetic_type_parameters(count: usize) -> Vec<TypeParameter> {
    (1..=count)
        .map(|index| TypeParameter {
            name: format!("T{index}"),
            bound: None,
        })
        .collect()
}

fn type_parameter_names(export: &BindingExport) -> BTreeMap<String, String> {
    export
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            (
                parameter.name.clone(),
                format!(
                    "_{}_{}_{}",
                    pascal(&export.name),
                    python_identifier(&parameter.name),
                    index + 1
                ),
            )
        })
        .collect()
}

fn ordered_type_parameter_names(
    export: &BindingExport,
    names: &BTreeMap<String, String>,
) -> Vec<String> {
    export
        .parameters
        .iter()
        .map(|parameter| names[&parameter.name].clone())
        .collect()
}

fn runtime_type_parameter_declarations(module: &BindingModule) -> String {
    module
        .exports
        .iter()
        .flat_map(|export| {
            let names = type_parameter_names(export);
            ordered_type_parameter_names(export, &names)
        })
        .map(|name| format!("{name} = TypeVar({name:?})"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn stub_type_parameter_declarations(
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
) -> String {
    module
        .exports
        .iter()
        .flat_map(|export| {
            let names = type_parameter_names(export);
            export.parameters.iter().map(move |parameter| {
                let name = &names[&parameter.name];
                parameter.bound.as_ref().map_or_else(
                    || format!("{name} = TypeVar({name:?})"),
                    |bound| {
                        format!(
                            "{name} = TypeVar({name:?}, bound={})",
                            python_type_with_parameters(
                                bound,
                                module,
                                definitions,
                                aliases,
                                &names,
                            )
                        )
                    },
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn signature(
    inputs: &[Field],
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
    type_parameters: &BTreeMap<String, String>,
) -> String {
    let names = python_parameter_names(inputs);
    let parameters = inputs
        .iter()
        .zip(names)
        .enumerate()
        .map(|(_index, (field, name))| {
            let default = if field.optional { " = ..." } else { "" };
            format!(
                "{name}: {}{default}",
                python_type_with_parameters(
                    &field.ty,
                    module,
                    definitions,
                    aliases,
                    type_parameters,
                )
            )
        })
        .collect::<Vec<_>>();
    if parameters.is_empty() {
        "self".to_owned()
    } else {
        format!("self, {}", parameters.join(", "))
    }
}

fn constructor_signature(
    export: &BindingExport,
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
) -> String {
    let type_parameters = type_parameter_names(export);
    let parameters = export
        .fields
        .iter()
        .zip(python_parameter_names(&export.fields))
        .map(|(field, name)| {
            let default = if field.optional { " = ..." } else { "" };
            format!(
                "{name}: {}{default}",
                python_type_with_parameters(
                    &field.ty,
                    module,
                    definitions,
                    aliases,
                    &type_parameters,
                )
            )
        })
        .collect::<Vec<_>>();
    if parameters.is_empty() {
        "cls".to_owned()
    } else {
        format!("cls, *, {}", parameters.join(", "))
    }
}

fn constructor_result_type(
    export: &BindingExport,
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
) -> String {
    export.results.first().map_or_else(
        || {
            let names = type_parameter_names(export);
            let parameters = ordered_type_parameter_names(export, &names);
            if parameters.is_empty() {
                export.name.clone()
            } else {
                format!("{}[{}]", export.name, parameters.join(", "))
            }
        },
        |result| {
            python_type_with_parameters(
                &result.ty,
                module,
                definitions,
                aliases,
                &type_parameter_names(export),
            )
        },
    )
}

fn field_map_literal(fields: &[Field]) -> String {
    let entries = fields
        .iter()
        .zip(python_parameter_names(fields))
        .map(|(field, python_name)| {
            let optional = if field.optional { "True" } else { "False" };
            format!("({python_name:?}, {:?}, {optional})", field.name)
        })
        .collect::<Vec<_>>();
    match entries.as_slice() {
        [] => "()".to_owned(),
        [entry] => format!("({entry},)"),
        _ => format!("({})", entries.join(", ")),
    }
}

/// The exact spelling used by both generated runtime objects and typing stubs.
///
/// Lab identifiers may be Python keywords, and two independently legal Lab spellings may
/// normalize to the same Python identifier. Suffixing is deterministic and never changes the
/// underlying Lab operand name retained by the runtime object.
fn python_parameter_names(inputs: &[Field]) -> Vec<String> {
    let mut used = BTreeSet::from(["self".to_owned()]);
    inputs
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let base = if valid_identifier(&field.name) {
                field.name.clone()
            } else {
                python_identifier(&field.name)
            };
            let mut candidate = base.clone();
            let mut suffix = index + 1;
            while !used.insert(candidate.clone()) {
                candidate = format!("{base}_{suffix}");
                suffix += 1;
            }
            candidate
        })
        .collect()
}

fn result_type(
    results: &[Field],
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
    type_parameters: &BTreeMap<String, String>,
) -> String {
    match results {
        [] => "None".to_owned(),
        [result] => {
            python_type_with_parameters(&result.ty, module, definitions, aliases, type_parameters)
        }
        many => format!(
            "tuple[{}]",
            many.iter()
                .map(|field| {
                    python_type_with_parameters(
                        &field.ty,
                        module,
                        definitions,
                        aliases,
                        type_parameters,
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn type_aliases(
    module: &BindingModule,
    definitions: &DefinitionModules,
) -> BTreeMap<PathBuf, String> {
    let mut paths = BTreeSet::new();
    for export in &module.exports {
        for role in &export.roles {
            if let Some(path) = definitions.resolve(module, role)
                && path != &module.python_path
            {
                paths.insert(path.clone());
            }
        }
        for parameter in &export.parameters {
            if let Some(bound) = &parameter.bound {
                for word in type_words(bound) {
                    if let Some(path) = definitions.resolve(module, word)
                        && path != &module.python_path
                    {
                        paths.insert(path.clone());
                    }
                }
            }
        }
        for field in export
            .fields
            .iter()
            .chain(&export.inputs)
            .chain(&export.results)
        {
            for word in type_words(&field.ty) {
                if let Some(path) = definitions.resolve(module, word)
                    && path != &module.python_path
                {
                    paths.insert(path.clone());
                }
            }
        }
    }
    paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| (path, format!("_module_{index}")))
        .collect()
}

fn role_bases(
    export: &BindingExport,
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
) -> Vec<String> {
    export
        .roles
        .iter()
        .map(|role| qualify_word(role, module, definitions, aliases))
        .filter(|role| role != "object")
        .collect()
}

fn python_type_with_parameters(
    ty: &str,
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
    type_parameters: &BTreeMap<String, String>,
) -> String {
    let ty = ty.trim();
    if ty.starts_with("any ") || ty.starts_with("same as ") {
        return "Any".to_owned();
    }
    if let Some((subject, state)) = split_top_level_once(ty, " is ") {
        return format!(
            "{}[{}]",
            qualify_word(state.trim(), module, definitions, aliases),
            python_type_with_parameters(subject, module, definitions, aliases, type_parameters,)
        );
    }
    if let Some(parts) = split_top_level(ty, " | ") {
        return parts
            .iter()
            .map(|part| {
                python_type_with_parameters(part, module, definitions, aliases, type_parameters)
            })
            .collect::<Vec<_>>()
            .join(" | ");
    }
    if let Some(open) = ty.find('<')
        && ty.ends_with('>')
    {
        let constructor = &ty[..open];
        if constructor == "Quantity" {
            return "Quantity".to_owned();
        }
        let arguments = &ty[open + 1..ty.len() - 1];
        let rendered = split_arguments(arguments)
            .iter()
            .map(|argument| {
                python_type_with_parameters(argument, module, definitions, aliases, type_parameters)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let constructor = if constructor == "List" {
            "list".to_owned()
        } else {
            qualify_word(constructor, module, definitions, aliases)
        };
        return format!("{constructor}[{rendered}]");
    }
    match ty {
        "Integer" => "int".to_owned(),
        "Decimal" => "Decimal".to_owned(),
        "String" => "str".to_owned(),
        "Bool" => "bool".to_owned(),
        "None" => "None".to_owned(),
        other => type_parameters
            .get(other)
            .cloned()
            .unwrap_or_else(|| qualify_word(other, module, definitions, aliases)),
    }
}

fn qualify_word(
    word: &str,
    module: &BindingModule,
    definitions: &DefinitionModules,
    aliases: &BTreeMap<PathBuf, String>,
) -> String {
    let Some(path) = definitions.resolve(module, word) else {
        // `generate` validates every rendered type before reaching this pure formatting helper.
        // Keep the fallback a true wildcard rather than the invariant supertype `object` so a
        // malformed internal BindingModule cannot accidentally become narrower than Lab's open
        // type semantics.
        return "Any".to_owned();
    };
    if path == &module.python_path {
        python_identifier(word)
    } else if let Some(alias) = aliases.get(path) {
        format!("{alias}.{}", python_identifier(word))
    } else {
        "Any".to_owned()
    }
}

fn validate_type_references(
    modules: &[BindingModule],
    definitions: &DefinitionModules,
) -> Result<(), PythonBindingGenerationError> {
    for module in modules.iter().filter(|module| module.emit) {
        for export in &module.exports {
            let parameters = export
                .parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<BTreeSet<_>>();
            for role in &export.roles {
                validate_type_reference(role, module, export, definitions, &parameters)?;
            }
            for parameter in &export.parameters {
                if let Some(bound) = &parameter.bound {
                    validate_type_reference(bound, module, export, definitions, &parameters)?;
                }
            }
            for field in export
                .fields
                .iter()
                .chain(&export.inputs)
                .chain(&export.results)
            {
                validate_type_reference(&field.ty, module, export, definitions, &parameters)?;
            }
        }
    }
    Ok(())
}

fn validate_type_reference(
    ty: &str,
    module: &BindingModule,
    export: &BindingExport,
    definitions: &DefinitionModules,
    type_parameters: &BTreeSet<&str>,
) -> Result<(), PythonBindingGenerationError> {
    let ty = ty.trim();
    if ty.starts_with("any ") || ty.starts_with("same as ") {
        return Ok(());
    }
    if let Some((subject, state)) = split_top_level_once(ty, " is ") {
        validate_nominal_type(
            state.trim(),
            ty,
            module,
            export,
            definitions,
            type_parameters,
        )?;
        return validate_type_reference(subject, module, export, definitions, type_parameters);
    }
    if let Some(parts) = split_top_level(ty, " | ") {
        for part in parts {
            validate_type_reference(part, module, export, definitions, type_parameters)?;
        }
        return Ok(());
    }
    if let Some(open) = ty.find('<')
        && ty.ends_with('>')
    {
        let constructor = ty[..open].trim();
        if constructor == "Quantity" {
            // Quantity arguments are units, not nominal Lab types.
            return Ok(());
        }
        if constructor != "List" {
            validate_nominal_type(
                constructor,
                ty,
                module,
                export,
                definitions,
                type_parameters,
            )?;
        }
        let arguments = &ty[open + 1..ty.len() - 1];
        for argument in split_arguments(arguments) {
            validate_type_reference(argument, module, export, definitions, type_parameters)?;
        }
        return Ok(());
    }
    validate_nominal_type(ty, ty, module, export, definitions, type_parameters)
}

fn validate_nominal_type(
    name: &str,
    complete_type: &str,
    module: &BindingModule,
    export: &BindingExport,
    definitions: &DefinitionModules,
    type_parameters: &BTreeSet<&str>,
) -> Result<(), PythonBindingGenerationError> {
    if matches!(name, "Integer" | "Decimal" | "String" | "Bool" | "None")
        || type_parameters.contains(name)
        || definitions.resolve(module, name).is_some()
    {
        return Ok(());
    }
    Err(PythonBindingGenerationError::UnresolvedType {
        module: module.lab_path.clone(),
        export: export.source_name.clone(),
        type_name: complete_type.to_owned(),
    })
}

fn package_files(generated: &[GeneratedFile], root_package: bool) -> Vec<GeneratedFile> {
    let mut directories = BTreeSet::new();
    for file in generated {
        let mut parent = file.path.parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    if !root_package {
        directories.remove(Path::new("."));
    }
    directories
        .into_iter()
        .flat_map(|directory| {
            let documentation = format!(
                "\"\"\"Generated bindings for Lab modules under `{}`.\"\"\"\n\n# Generated by `lab bindings python`. Do not edit.\n",
                dotted_python_path(&directory)
            );
            [
                GeneratedFile {
                    path: directory.join("__init__.py"),
                    source: documentation.clone(),
                },
                GeneratedFile {
                    path: directory.join("__init__.pyi"),
                    source: documentation,
                },
            ]
        })
        .collect()
}

fn standard_module(
    module: &lab_language::manifest::Module,
    emit: bool,
    installed_path: bool,
) -> BindingModule {
    let python_path = standard_python_path(&module.path);
    BindingModule {
        lab_path: module.path.clone(),
        python_path: if installed_path {
            PathBuf::from("lab").join(python_path)
        } else {
            python_path
        },
        emit,
        standard: true,
        prelude: module.prelude,
        documentation: module.documentation.clone(),
        imports: module.imports.clone(),
        exports: module.exports.iter().map(standard_export).collect(),
    }
}

fn standard_export(export: &StandardExport) -> BindingExport {
    match export {
        StandardExport::ArtifactKind {
            definition,
            name,
            documentation,
            produces,
            roles,
            fields,
            ..
        } => base_export(definition, BindingKind::ArtifactKind, name, documentation).with(|item| {
            item.produces = Some(produces.clone());
            item.roles = roles.clone();
            item.fields = standard_fields(fields);
        }),
        StandardExport::Type {
            definition,
            name,
            documentation,
            parameters,
            roles,
            fields,
        } => base_export(definition, BindingKind::Type, name, documentation).with(|item| {
            item.parameters = standard_type_parameters(parameters);
            item.roles = roles.clone();
            item.fields = standard_fields(fields);
        }),
        StandardExport::Role {
            definition,
            name,
            documentation,
        } => base_export(definition, BindingKind::Role, name, documentation),
        StandardExport::Facet {
            definition,
            name,
            documentation,
            states,
            ..
        } => base_export(definition, BindingKind::Facet, name, documentation).with(|item| {
            item.facet_states = states.clone();
        }),
        StandardExport::Value {
            definition,
            name,
            documentation,
            r#type,
        } => base_export(definition, BindingKind::Value, name, documentation).with(|item| {
            item.results.push(Field {
                name: "value".to_owned(),
                ty: r#type.clone(),
                optional: false,
            });
        }),
        StandardExport::Function {
            definition,
            name,
            documentation,
            parameters,
            inputs,
            result,
        } => base_export(definition, BindingKind::Function, name, documentation).with(|item| {
            item.parameters = standard_type_parameters(parameters);
            item.inputs = inputs
                .iter()
                .enumerate()
                .map(|(index, ty)| Field {
                    name: format!("argument_{}", index + 1),
                    ty: ty.clone(),
                    optional: false,
                })
                .collect();
            item.results.push(Field {
                name: "result".to_owned(),
                ty: result.clone(),
                optional: false,
            });
        }),
        StandardExport::Constructor {
            definition,
            name,
            documentation,
            fields,
            result,
        } => base_export(definition, BindingKind::Constructor, name, documentation).with(|item| {
            item.fields = standard_fields(fields);
            item.results.push(Field {
                name: "result".to_owned(),
                ty: result.clone(),
                optional: false,
            });
        }),
        StandardExport::Action {
            definition,
            name,
            documentation,
            parameters,
            operation,
            phrase,
            operands,
            optional,
            results,
        } => base_export(definition, BindingKind::Action, name, documentation).with(|item| {
            item.parameters = standard_type_parameters(parameters);
            item.operation = Some(operation.clone());
            item.phrase = phrase.clone();
            item.optional_clauses = optional.clone();
            item.inputs = standard_fields(operands);
            let optional_names = optional
                .iter()
                .flatten()
                .filter_map(|word| {
                    word.strip_prefix('<')
                        .and_then(|word| word.strip_suffix('>'))
                })
                .collect::<BTreeSet<_>>();
            for field in &mut item.inputs {
                field.optional = optional_names.contains(field.name.as_str());
            }
            item.results = standard_fields(results);
        }),
        StandardExport::Workflow {
            definition,
            name,
            documentation,
            parameters,
            inputs,
            results,
        } => base_export(definition, BindingKind::Workflow, name, documentation).with(|item| {
            item.parameters = standard_type_parameters(parameters);
            item.inputs = standard_fields(inputs);
            item.results = standard_fields(results);
        }),
    }
}

fn standard_type_parameters(
    parameters: &[lab_language::manifest::TypeParameter],
) -> Vec<TypeParameter> {
    parameters
        .iter()
        .map(|parameter| TypeParameter {
            name: parameter.name.clone(),
            bound: parameter.bound.clone(),
        })
        .collect()
}

fn package_module(requested_package: &str, module: &PackageModule) -> BindingModule {
    let lab_root = module.package.replace('-', "_");
    let python_package = python_identifier(&lab_root);
    let path = module.interface.module.as_str();
    let suffix = path
        .strip_prefix(&lab_root)
        .and_then(|suffix| suffix.strip_prefix('.'));
    let mut python_path = PathBuf::from(&python_package);
    if let Some(suffix) = suffix {
        for segment in suffix.split('.') {
            python_path.push(python_identifier(segment));
        }
    } else if path != lab_root {
        for segment in path.split('.') {
            python_path.push(python_identifier(segment));
        }
    } else {
        python_path.push("_root");
    }
    BindingModule {
        lab_path: path.to_owned(),
        python_path,
        emit: module.package == requested_package,
        standard: false,
        prelude: false,
        documentation: module.interface.documentation.clone(),
        imports: module
            .imports
            .iter()
            .map(|import| import.module.clone())
            .collect(),
        exports: module
            .interface
            .exports
            .iter()
            .map(|(name, export)| {
                let mut item = base_export(
                    &export.definition,
                    match export.kind {
                        ExportKind::ArtifactKind => BindingKind::ArtifactKind,
                        ExportKind::Type => BindingKind::Type,
                        ExportKind::Role => BindingKind::Role,
                        ExportKind::Facet => BindingKind::Facet,
                        ExportKind::Value => BindingKind::Value,
                        ExportKind::Function => BindingKind::Function,
                        ExportKind::Constructor => BindingKind::Constructor,
                        ExportKind::Action => BindingKind::Action,
                        ExportKind::Workflow => BindingKind::Workflow,
                    },
                    name,
                    &export.documentation,
                );
                item.parameters = export
                    .parameters
                    .names
                    .iter()
                    .map(|name| TypeParameter {
                        name: name.clone(),
                        bound: export
                            .parameters
                            .bounds
                            .get(name)
                            .map(lab_language::CheckedType::display_name),
                    })
                    .collect();
                item.roles = export.roles.clone();
                item.fields = export
                    .fields
                    .iter()
                    .map(|(name, ty)| Field {
                        name: name.clone(),
                        ty: ty.display_name(),
                        optional: false,
                    })
                    .collect();
                // Ordinary package types come from source `record` declarations, so even a
                // fieldless record has a literal constructor. Native standard nominal types are
                // represented through the manifest path above and remain non-callable unless
                // they declare fields or an explicit constructor.
                if item.kind == BindingKind::Type {
                    item.kind = BindingKind::Constructor;
                }
                if let Some(schema) = &export.schema {
                    item.produces = Some(schema.produces.display_name());
                    item.fields = schema
                        .fields
                        .iter()
                        .map(|field| Field {
                            name: field.name.clone(),
                            ty: field.r#type.display_name(),
                            optional: field.optional,
                        })
                        .collect();
                }
                if let Some(facet) = &export.facet {
                    item.facet_states = facet
                        .states
                        .iter()
                        .map(|state| state.name.clone())
                        .collect();
                }
                if let Some(action) = &export.action {
                    item.operation = Some(action.operation.clone());
                    item.phrase = action
                        .phrase
                        .iter()
                        .map(|token| match token {
                            lab_language::CheckedPhraseToken::Word(word) => word.clone(),
                            lab_language::CheckedPhraseToken::Hole(name) => format!("<{name}>"),
                        })
                        .collect();
                    item.inputs = action
                        .operands
                        .iter()
                        .map(|operand| Field {
                            name: operand.name.clone(),
                            ty: operand.r#type.display_name(),
                            optional: false,
                        })
                        .collect();
                    item.results = action
                        .results
                        .iter()
                        .map(|result| Field {
                            name: result.name.clone(),
                            ty: result.r#type.display_name(),
                            optional: false,
                        })
                        .collect();
                }
                if let Some(callable) = &export.callable {
                    item.inputs = callable
                        .inputs
                        .iter()
                        .map(|field| Field {
                            name: field.name.clone(),
                            ty: field.r#type.display_name(),
                            optional: false,
                        })
                        .collect();
                    item.results = callable
                        .outputs
                        .iter()
                        .map(|field| Field {
                            name: field.name.clone(),
                            ty: field.r#type.display_name(),
                            optional: false,
                        })
                        .collect();
                } else if export.kind != ExportKind::Type
                    && let Some(ty) = &export.r#type
                {
                    item.results.push(Field {
                        name: "value".to_owned(),
                        ty: ty.display_name(),
                        optional: false,
                    });
                }
                item
            })
            .collect(),
    }
}

trait With: Sized {
    fn with(mut self, update: impl FnOnce(&mut Self)) -> Self {
        update(&mut self);
        self
    }
}

impl<T> With for T {}

fn base_export(
    definition: &lab_language::DefinitionId,
    kind: BindingKind,
    name: &str,
    documentation: &str,
) -> BindingExport {
    BindingExport {
        definition_module: definition.module.to_string(),
        definition_local: definition.local.clone(),
        kind,
        source_name: name.to_owned(),
        name: python_identifier(name),
        documentation: documentation.to_owned(),
        parameters: if kind == BindingKind::Facet {
            synthetic_type_parameters(1)
        } else {
            Vec::new()
        },
        fields: Vec::new(),
        roles: Vec::new(),
        produces: None,
        facet_states: Vec::new(),
        operation: None,
        phrase: Vec::new(),
        optional_clauses: Vec::new(),
        inputs: Vec::new(),
        results: Vec::new(),
    }
}

fn standard_fields(fields: &[lab_language::manifest::Field]) -> Vec<Field> {
    fields
        .iter()
        .map(|field| Field {
            name: field.name.clone(),
            ty: field.r#type.clone(),
            optional: field.optional,
        })
        .collect()
}

fn python_binding_name(export: &BindingExport) -> String {
    export.produces.as_ref().map_or_else(
        || export.name.clone(),
        |produces| python_identifier(produces),
    )
}

fn standard_python_path(path: &str) -> PathBuf {
    let mut segments = path.split('.').collect::<Vec<_>>();
    if segments.first() == Some(&"std") {
        segments.remove(0);
    }
    if segments == ["prelude"] {
        return PathBuf::from("_prelude");
    }
    if segments.first() == Some(&"lab") && segments.len() > 1 {
        segments.remove(0);
    }
    segments.iter().collect()
}

fn definition_literal(export: &BindingExport) -> String {
    tuple_literal(&[
        export.definition_module.clone(),
        export.definition_local.clone(),
    ])
}

fn tuple_literal(values: &[String]) -> String {
    match values {
        [] => "()".to_owned(),
        [only] => format!("({only:?},)"),
        many => format!(
            "({})",
            many.iter()
                .map(|value| format!("{value:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn nested_tuple_literal(values: &[Vec<String>]) -> String {
    match values {
        [] => "()".to_owned(),
        [only] => format!("({},)", tuple_literal(only)),
        many => format!(
            "({})",
            many.iter()
                .map(|value| tuple_literal(value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn documented(declaration: String, documentation: &str) -> String {
    if documentation.trim().is_empty() {
        declaration
    } else {
        format!("{declaration}\n{}", module_doc(documentation))
    }
}

fn module_doc(documentation: &str) -> String {
    let documentation = documentation.trim().replace("\"\"\"", "\\\"\\\"\\\"");
    if documentation.is_empty() {
        "\"\"\"Generated bindings for a Lab module.\"\"\"".to_owned()
    } else {
        format!("\"\"\"{documentation}\"\"\"")
    }
}

fn join_blocks(blocks: Vec<String>) -> String {
    format!("{}\n", blocks.join("\n\n"))
}

fn dotted_python_path(path: &Path) -> String {
    path.iter()
        .map(|segment| segment.to_string_lossy())
        .collect::<Vec<_>>()
        .join(".")
}

fn python_identifier(name: &str) -> String {
    let mut output = name
        .chars()
        .enumerate()
        .map(|(index, character)| {
            if character == '_'
                || character.is_ascii_alphanumeric() && (index != 0 || !character.is_ascii_digit())
            {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if output.is_empty() || is_keyword(&output) {
        output.push('_');
    }
    output
}

fn valid_identifier(name: &str) -> bool {
    !name.is_empty()
        && !is_keyword(name)
        && name.chars().enumerate().all(|(index, character)| {
            character == '_'
                || character.is_ascii_alphanumeric() && (index != 0 || !character.is_ascii_digit())
        })
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "case"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "match"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

fn pascal(name: &str) -> String {
    let mut output = String::new();
    for segment in name.split('_') {
        let mut characters = segment.chars();
        if let Some(first) = characters.next() {
            output.extend(first.to_uppercase());
            output.extend(characters);
        }
    }
    if output.is_empty() {
        "Binding".to_owned()
    } else {
        output
    }
}

fn type_words(ty: &str) -> impl Iterator<Item = &str> {
    ty.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|word| !word.is_empty())
}

fn split_top_level<'a>(value: &'a str, separator: &str) -> Option<Vec<&'a str>> {
    let mut depth = 0_i32;
    let mut start = 0;
    let mut parts = Vec::new();
    for (index, character) in value.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth -= 1,
            _ => {}
        }
        if depth == 0 && value[index..].starts_with(separator) {
            parts.push(&value[start..index]);
            start = index + separator.len();
        }
    }
    if parts.is_empty() {
        None
    } else {
        parts.push(&value[start..]);
        Some(parts)
    }
}

fn split_top_level_once<'a>(value: &'a str, separator: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0_i32;
    for (index, character) in value.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth -= 1,
            _ => {}
        }
        if depth == 0 && value[index..].starts_with(separator) {
            return Some((&value[..index], &value[index + separator.len()..]));
        }
    }
    None
}

fn split_arguments(value: &str) -> Vec<&str> {
    let mut depth = 0_i32;
    let mut start = 0;
    let mut parts = Vec::new();
    for (index, character) in value.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(value[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());
    parts
}

#[cfg(test)]
mod tests {
    use std::fs;

    use lab_language::{
        ModuleId, SemanticEnvironment, compile_module_in_environment, compile_module_with_id,
        standard_library_manifest,
    };

    use super::*;

    #[test]
    fn persistence_removes_only_stale_manifest_owned_files() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path();
        let first = [GeneratedFile {
            path: PathBuf::from("package/old.py"),
            source: "old = True\n".to_owned(),
        }];
        write_generated_files(output, &first).unwrap();
        fs::write(output.join("package/handwritten.py"), "keep = True\n").unwrap();

        let second = [GeneratedFile {
            path: PathBuf::from("package/new.py"),
            source: "new = True\n".to_owned(),
        }];
        write_generated_files(output, &second).unwrap();

        assert!(!output.join("package/old.py").exists());
        assert!(output.join("package/new.py").is_file());
        assert!(output.join("package/handwritten.py").is_file());
        let manifest: PythonBindingsManifest = serde_json::from_str(
            &fs::read_to_string(output.join(PYTHON_BINDINGS_MANIFEST)).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.files, vec![PathBuf::from("package/new.py")]);
    }

    #[test]
    fn persistence_refuses_unsafe_manifest_paths_before_writing() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join(PYTHON_BINDINGS_MANIFEST),
            r#"{"schema_version":1,"files":["../outside.py"]}"#,
        )
        .unwrap();
        let generated = [GeneratedFile {
            path: PathBuf::from("package/new.py"),
            source: "new = True\n".to_owned(),
        }];

        let error = write_generated_files(directory.path(), &generated).unwrap_err();

        assert!(matches!(error, PythonBindingWriteError::UnsafePath { .. }));
        assert!(!directory.path().join("package/new.py").exists());
    }

    #[test]
    fn persistence_refuses_to_overwrite_an_unowned_file() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("package")).unwrap();
        let target = directory.path().join("package/module.py");
        fs::write(&target, "handwritten = True\n").unwrap();
        let generated = [GeneratedFile {
            path: PathBuf::from("package/module.py"),
            source: "generated = True\n".to_owned(),
        }];

        let error = write_generated_files(directory.path(), &generated).unwrap_err();

        assert!(matches!(error, PythonBindingWriteError::UnownedFile { .. }));
        assert_eq!(fs::read_to_string(target).unwrap(), "handwritten = True\n");
    }

    #[cfg(unix)]
    #[test]
    fn persistence_refuses_an_intermediate_symbolic_link() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), directory.path().join("package")).unwrap();
        let generated = [GeneratedFile {
            path: PathBuf::from("package/module.py"),
            source: "generated = True\n".to_owned(),
        }];

        let error = write_generated_files(directory.path(), &generated).unwrap_err();

        assert!(matches!(
            error,
            PythonBindingWriteError::SymbolicLink { .. }
        ));
        assert!(!outside.path().join("module.py").exists());
    }

    #[test]
    fn package_actions_and_workflows_get_precise_stubs_and_stable_ids() {
        let samples = compile_module_with_id(
            ModuleId::new("thermals.samples"),
            "record Sample\n\nrecord Settings:\n  cycles: Integer\n  from: String\n\nrecord Outcome:\n  case Empty\n",
        )
        .unwrap();
        let environment = SemanticEnvironment::new([samples.interface.clone()]);
        let module = compile_module_in_environment(
            ModuleId::new("thermals.protocols"),
            r#"
use thermals.samples

action heat <sample> -> heated:
  sample: take Sample
  heated: Sample begins

action label <from> -> labeled:
  from: copy Sample
  labeled: Sample identified by from

workflow repeat(sample: Sample, cycles: Integer) -> Sample:
  heated <- heat sample
  return heated
"#,
            &environment,
        )
        .unwrap();
        let files = generate_package(
            "thermals",
            &[
                PackageModule {
                    package: "thermals".to_owned(),
                    imports: samples.imports.clone(),
                    interface: samples.interface,
                },
                PackageModule {
                    package: "thermals".to_owned(),
                    imports: module.imports.clone(),
                    interface: module.interface,
                },
            ],
        )
        .unwrap();
        let runtime = files
            .iter()
            .find(|file| file.path == Path::new("thermals/protocols.py"))
            .unwrap();
        let stub = files
            .iter()
            .find(|file| file.path == Path::new("thermals/protocols.pyi"))
            .unwrap();
        let samples_runtime = files
            .iter()
            .find(|file| file.path == Path::new("thermals/samples.py"))
            .unwrap();
        let samples_stub = files
            .iter()
            .find(|file| file.path == Path::new("thermals/samples.pyi"))
            .unwrap();

        assert!(
            runtime
                .source
                .contains("definition=(\"thermals.protocols\", \"heat\")")
        );
        assert!(runtime.source.contains("ImportedWorkflow("));
        assert!(runtime.source.contains("python_slots=(\"from_\",)"));
        assert!(stub.source.contains("import thermals.samples as _module_0"));
        assert!(
            stub.source
                .contains("sample: _module_0.Sample, cycles: int")
        );
        assert!(stub.source.contains("-> Effect[_module_0.Sample]"));
        assert!(stub.source.contains("from_: _module_0.Sample"));
        assert!(stub.source.contains("-> WorkflowCall[_module_0.Sample]"));
        assert!(
            samples_runtime
                .source
                .contains("class Settings(LabConstructor):")
        );
        assert!(
            samples_stub
                .source
                .contains("class Settings(LabConstructor):")
        );
        assert!(
            samples_runtime
                .source
                .contains("(\"from_\", \"from\", False)")
        );
        assert!(
            samples_stub
                .source
                .contains("def __new__(cls, *, cycles: int, from_: str) -> Settings")
        );
        assert!(
            samples_runtime
                .source
                .contains("class Empty(LabConstructor):")
        );
        assert!(samples_stub.source.contains("def __new__(cls) -> Outcome"));
    }

    #[test]
    fn standard_library_uses_the_same_renderer_and_emits_runtime_and_stubs() {
        let files = generate_standard_library(&standard_library_manifest()).unwrap();
        assert!(
            files
                .iter()
                .any(|file| file.path == Path::new("bio/designs.py"))
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == Path::new("bio/designs.pyi"))
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == Path::new("competence.pyi"))
        );
        let prelude = files
            .iter()
            .find(|file| file.path == Path::new("_prelude.py"))
            .unwrap();
        let prelude_stub = files
            .iter()
            .find(|file| file.path == Path::new("_prelude.pyi"))
            .unwrap();
        let designs_stub = files
            .iter()
            .find(|file| file.path == Path::new("bio/designs.pyi"))
            .unwrap();
        let plasmid_stub = files
            .iter()
            .find(|file| file.path == Path::new("plasmid.pyi"))
            .unwrap();
        assert!(prelude.source.contains("class Plasmid(LabConstructor):"));
        assert!(prelude.source.contains("class Screening(LabConstructor):"));
        assert!(prelude.source.contains("inputs=(\"argument_1\",)"));
        assert!(prelude.source.contains("python_inputs=(\"argument_1\",)"));
        assert!(
            !prelude.source.contains(" false") && !prelude.source.contains(" true"),
            "runtime bindings must use Python boolean literals"
        );
        assert!(
            !prelude.source.contains("__lab_uses__ = (\"std.prelude\",)"),
            "the implicit prelude must not explicitly import itself"
        );
        assert!(prelude_stub.source.contains("Generic[_Accepted_Value_1]"));
        assert!(prelude_stub.source.contains(") -> Accepted[Plasmid]: ..."));
        assert!(
            prelude_stub.source.contains("class Buffer(Solution): ..."),
            "a role already supplies the LabType base:\n{}",
            prelude_stub.source
        );
        assert!(
            designs_stub
                .source
                .contains("class Antibiotic(ArtifactKind, _module_1.SimpleChemical): ..."),
            "an artifact role already supplies the LabType base:\n{}",
            designs_stub.source
        );

        let first = designs_stub
            .source
            .lines()
            .position(|line| line.starts_with("_Both_First_1 = TypeVar("))
            .expect("Both.First has its declared type parameter name");
        let second = designs_stub
            .source
            .lines()
            .position(|line| line.starts_with("_Both_Second_2 = TypeVar("))
            .expect("Both.Second has its declared type parameter name");
        assert!(
            first < second,
            "type parameter declaration order must survive"
        );
        for parameter in ["_Both_First_1", "_Both_Second_2"] {
            let declaration = designs_stub
                .source
                .lines()
                .find(|line| line.starts_with(&format!("{parameter} = TypeVar(")))
                .unwrap();
            assert!(
                declaration.contains("bound=") && declaration.ends_with(".Signal)"),
                "the checked Signal bound must survive: {declaration}"
            );
        }
        assert!(
            designs_stub
                .source
                .contains("Generic[_Both_First_1, _Both_Second_2]")
        );

        for parameter in ["_Provision_T_1", "_Dispose_T_1"] {
            assert!(
                plasmid_stub
                    .source
                    .contains(&format!("{parameter} = TypeVar(\"{parameter}\")")),
                "{parameter} must be local to its action export"
            );
        }
        assert!(plasmid_stub.source.contains("item: _Provision_T_1"));
        assert!(plasmid_stub.source.contains("Material[_Provision_T_1]"));
        assert!(plasmid_stub.source.contains("from typing import Any,"));
        assert!(
            plasmid_stub
                .source
                .contains("plate: _module_0.Material[_module_1.inoculated[Any]]")
        );
        assert!(
            plasmid_stub
                .source
                .contains("material: _module_0.Material[_Dispose_T_1]")
        );
    }

    #[test]
    fn standard_manifest_callable_parameters_keep_order_bounds_and_usage() {
        use lab_language::manifest::{
            Export as ManifestExport, Field as ManifestField, Library, Module, TypeParameter,
        };

        let parameters = || {
            vec![
                TypeParameter {
                    name: "Right".to_owned(),
                    bound: Some("SecondRole".to_owned()),
                },
                TypeParameter {
                    name: "Left".to_owned(),
                    bound: Some("FirstRole".to_owned()),
                },
            ]
        };
        let definition = |name| lab_language::DefinitionId::exported("std.generics", name);
        let library = Library {
            modules: vec![Module {
                path: "std.generics".to_owned(),
                prelude: false,
                documentation: String::new(),
                imports: Vec::new(),
                exports: vec![
                    ManifestExport::Role {
                        definition: definition("FirstRole"),
                        name: "FirstRole".to_owned(),
                        documentation: String::new(),
                    },
                    ManifestExport::Role {
                        definition: definition("SecondRole"),
                        name: "SecondRole".to_owned(),
                        documentation: String::new(),
                    },
                    ManifestExport::Function {
                        definition: definition("select"),
                        name: "select".to_owned(),
                        documentation: String::new(),
                        parameters: parameters(),
                        inputs: vec!["Right".to_owned(), "Left".to_owned()],
                        result: "Right".to_owned(),
                    },
                    ManifestExport::Workflow {
                        definition: definition("preserve"),
                        name: "preserve".to_owned(),
                        documentation: String::new(),
                        parameters: parameters(),
                        inputs: vec![
                            ManifestField {
                                name: "right".to_owned(),
                                r#type: "Right".to_owned(),
                                optional: false,
                            },
                            ManifestField {
                                name: "left".to_owned(),
                                r#type: "Left".to_owned(),
                                optional: false,
                            },
                        ],
                        results: vec![ManifestField {
                            name: "result".to_owned(),
                            r#type: "Right".to_owned(),
                            optional: false,
                        }],
                    },
                ],
            }],
        };
        let files = generate_standard_library(&library).unwrap();
        let stub = files
            .iter()
            .find(|file| file.path == Path::new("generics.pyi"))
            .unwrap();

        for (export, first, second) in [
            ("Select", "_Select_Right_1", "_Select_Left_2"),
            ("Preserve", "_Preserve_Right_1", "_Preserve_Left_2"),
        ] {
            let first_declaration = stub.source.find(first).unwrap();
            let second_declaration = stub.source.find(second).unwrap();
            assert!(first_declaration < second_declaration);
            assert!(
                stub.source
                    .contains(&format!("{first} = TypeVar(\"{first}\", bound=SecondRole)")),
                "{export}.Right must retain its bound"
            );
            assert!(
                stub.source.contains(&format!(
                    "{second} = TypeVar(\"{second}\", bound=FirstRole)"
                )),
                "{export}.Left must retain its bound"
            );
        }
        assert!(stub.source.contains(
            "argument_1: _Select_Right_1, argument_2: _Select_Left_2) -> _Select_Right_1"
        ));
        assert!(stub.source.contains(
            "right: _Preserve_Right_1, left: _Preserve_Left_2) -> WorkflowCall[_Preserve_Right_1]"
        ));
    }

    #[test]
    fn python_sanitization_never_changes_the_rendered_lab_name() {
        let definition = lab_language::DefinitionId::exported("keywords", "from");
        let mut function = base_export(
            &definition,
            BindingKind::Function,
            "from",
            "A deliberately Python-hostile Lab export.",
        );
        function.inputs.push(Field {
            name: "class".to_owned(),
            ty: "String".to_owned(),
            optional: false,
        });
        function.results.push(Field {
            name: "result".to_owned(),
            ty: "String".to_owned(),
            optional: false,
        });
        let module = BindingModule {
            lab_path: "keywords".to_owned(),
            python_path: PathBuf::from("keywords"),
            emit: true,
            standard: false,
            prelude: false,
            documentation: String::new(),
            imports: Vec::new(),
            exports: vec![function],
        };

        let runtime = render_runtime(&module);
        assert!(runtime.contains("from_ = Function("));
        assert!(runtime.contains("name=\"from\""));
        assert!(runtime.contains("inputs=(\"class\",)"));
        assert!(runtime.contains("python_inputs=(\"class_\",)"));
    }

    #[test]
    fn generic_bounds_roles_and_parameter_order_survive_generation() {
        let checked = compile_module_with_id(
            ModuleId::new("roles.models"),
            r#"
role PairMember

record Pair<Zeta: PairMember, Alpha: PairMember> is PairMember:
  first: Zeta
  second: Alpha
"#,
        )
        .unwrap();
        let files = generate_package(
            "roles",
            &[PackageModule {
                package: "roles".to_owned(),
                imports: checked.imports,
                interface: checked.interface,
            }],
        )
        .unwrap();
        let runtime = files
            .iter()
            .find(|file| file.path == Path::new("roles/models.py"))
            .unwrap();
        let stub = files
            .iter()
            .find(|file| file.path == Path::new("roles/models.pyi"))
            .unwrap();

        assert!(runtime.source.contains("__lab_roles__ = (\"PairMember\",)"));
        assert!(stub.source.contains("bound=PairMember"));
        assert!(stub.source.contains(
            "class Pair(LabConstructor, PairMember, Generic[_Pair_Zeta_1, _Pair_Alpha_2])"
        ));
        assert!(
            stub.source
                .contains("first: _Pair_Zeta_1, second: _Pair_Alpha_2")
        );
        assert!(
            stub.source.contains("-> Pair[_Pair_Zeta_1, _Pair_Alpha_2]"),
            "{}",
            stub.source
        );
    }

    #[test]
    fn generation_rejects_all_top_level_python_name_collisions() {
        let definition = |name: &str| lab_language::DefinitionId::exported("collision", name);
        let module = |exports: Vec<BindingExport>| BindingModule {
            lab_path: "collision".to_owned(),
            python_path: PathBuf::from("collision"),
            emit: true,
            standard: false,
            prelude: false,
            documentation: String::new(),
            imports: Vec::new(),
            exports,
        };
        let value = |name: &str| base_export(&definition(name), BindingKind::Value, name, "");

        for exports in [
            vec![value("from"), value("from_")],
            vec![
                base_export(&definition("phase"), BindingKind::Facet, "phase", "")
                    .with(|export| export.facet_states = vec!["from".to_owned()]),
                value("from_"),
            ],
            vec![
                base_export(
                    &definition("sample"),
                    BindingKind::ArtifactKind,
                    "sample",
                    "",
                )
                .with(|export| export.produces = Some("from".to_owned())),
                value("from_"),
            ],
            vec![
                base_export(&definition("heat"), BindingKind::Action, "heat", ""),
                value("_HeatAction"),
            ],
        ] {
            assert!(matches!(
                generate(vec![module(exports)], false),
                Err(PythonBindingGenerationError::NameCollision { .. })
            ));
        }
    }

    #[test]
    fn generation_rejects_module_and_generated_file_collisions_before_writing() {
        let empty_module = |lab_path: &str, python_path: &str| BindingModule {
            lab_path: lab_path.to_owned(),
            python_path: PathBuf::from(python_path),
            emit: true,
            standard: false,
            prelude: false,
            documentation: String::new(),
            imports: Vec::new(),
            exports: Vec::new(),
        };
        assert!(matches!(
            generate(
                vec![
                    empty_module("example.from", "example/from_"),
                    empty_module("example.from_", "example/from_"),
                ],
                true,
            ),
            Err(PythonBindingGenerationError::ModulePathCollision { .. })
        ));
        assert!(matches!(
            generate(
                vec![empty_module("example.__init__", "example/__init__")],
                true,
            ),
            Err(PythonBindingGenerationError::FilePathCollision { .. })
        ));
    }

    #[test]
    fn generation_rejects_an_unresolved_nominal_type_instead_of_erasing_it() {
        let definition = lab_language::DefinitionId::exported("example", "measure");
        let mut function = base_export(
            &definition,
            BindingKind::Function,
            "measure",
            "An invalid interface fixture.",
        );
        function.inputs.push(Field {
            name: "sample".to_owned(),
            ty: "MissingSample".to_owned(),
            optional: false,
        });
        function.results.push(Field {
            name: "result".to_owned(),
            ty: "Integer".to_owned(),
            optional: false,
        });
        let module = BindingModule {
            lab_path: "example".to_owned(),
            python_path: PathBuf::from("example"),
            emit: true,
            standard: false,
            prelude: false,
            documentation: String::new(),
            imports: Vec::new(),
            exports: vec![function],
        };

        assert_eq!(
            generate(vec![module], false),
            Err(PythonBindingGenerationError::UnresolvedType {
                module: "example".to_owned(),
                export: "measure".to_owned(),
                type_name: "MissingSample".to_owned(),
            })
        );
    }

    #[test]
    fn package_types_resolve_through_exact_imports_with_standard_context() {
        let first =
            compile_module_with_id(ModuleId::new("dep_a.types"), "record Sample\n").unwrap();
        let selected =
            compile_module_with_id(ModuleId::new("dep_b.types"), "record Sample\n").unwrap();
        let environment =
            SemanticEnvironment::new([first.interface.clone(), selected.interface.clone()]);
        let protocol = compile_module_in_environment(
            ModuleId::new("assay.protocol"),
            r#"
use dep_b.types

action retain <sample> -> retained:
  sample: take Material<Sample>
  retained: Material<Sample> continues from sample
"#,
            &environment,
        )
        .unwrap();
        let files = generate_package(
            "assay",
            &[
                PackageModule {
                    package: "dep-a".to_owned(),
                    imports: first.imports,
                    interface: first.interface,
                },
                PackageModule {
                    package: "dep-b".to_owned(),
                    imports: selected.imports,
                    interface: selected.interface,
                },
                PackageModule {
                    package: "assay".to_owned(),
                    imports: protocol.imports,
                    interface: protocol.interface,
                },
            ],
        )
        .unwrap();
        assert!(
            files
                .iter()
                .all(|file| !file.path.starts_with("dep_a") && !file.path.starts_with("dep_b"))
        );
        let stub = files
            .iter()
            .find(|file| file.path == Path::new("assay/protocol.pyi"))
            .unwrap();
        assert!(stub.source.contains("import dep_b.types as _module_0"));
        assert!(!stub.source.contains("dep_a.types"));
        assert!(
            stub.source.contains("import lab._prelude as _module_1"),
            "{}",
            stub.source
        );
        assert!(
            stub.source
                .contains("sample: _module_1.Material[_module_0.Sample]")
        );
        assert!(
            stub.source
                .contains("-> Effect[_module_1.Material[_module_0.Sample]]")
        );
    }
}

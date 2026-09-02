//! Python extension module for Lab.

use std::path::Path;

use lab_compiler::method::{MethodDefinition, MethodRegistry, standard_method_definitions};
use lab_compiler::program::PortableLairProgram;
use lab_language::{
    CheckedModule, Diagnostic, ModuleId, SemanticEnvironment, SourceId,
    analyze_module_in_environment, compile_module, render_diagnostic, standard_library_manifest,
};
use lab_project::{FacilityPlanningResult, LabProject, plan_modules_for_package};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::Serialize;

/// Renders an error with its whole cause chain.
///
/// `FacilityProjectError` and friends deliberately keep a terse summary at the top and the real
/// explanation in `source`. Formatting only the outermost error hands a Python caller a verdict
/// with no way to act on it.
fn py_error(context: &str, error: &dyn std::error::Error) -> PyErr {
    let mut message = format!("{context}: {error}");
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(&format!("\n  caused by: {cause}"));
        source = cause.source();
    }
    PyValueError::new_err(message)
}

/// Parse, resolve, and type-check a Lab source module.

#[pyfunction]
fn compile_lab_module(source: &str) -> PyResult<String> {
    let module =
        compile_module(source).map_err(|error| PyValueError::new_err(error.to_string()))?;
    serde_json::to_string(&module).map_err(|error| PyValueError::new_err(error.to_string()))
}

/// One analyzed module: its checked IR where checking succeeded, and every
/// diagnostic it produced.
///
/// A failed module still yields diagnostics rather than an exception, so a
/// caller reports every module's problems in one pass instead of stopping at
/// the first.
#[derive(Serialize)]
struct AnalyzedModule {
    module: String,
    checked: Option<CheckedModule>,
    diagnostics: Vec<AnalyzedDiagnostic>,
}

/// A diagnostic together with the excerpt a reader meets, underlined against
/// the source it came from. Rendering happens here because the compiler owns
/// the excerpt format and the spans that drive it.
#[derive(Serialize)]
struct AnalyzedDiagnostic {
    #[serde(flatten)]
    diagnostic: Diagnostic,
    rendered: String,
}

/// Analyze named modules that resolve imports against one another.
///
/// Each module is checked against the interfaces of the modules before it, so
/// the caller supplies them in dependency order. A module that fails to check
/// contributes no interface, and the modules importing it report the
/// unresolved import rather than a cascade of missing names.
#[pyfunction]
fn analyze_lab_modules(modules: Vec<(String, String)>) -> PyResult<String> {
    let mut environment = SemanticEnvironment::default();
    let mut analyzed = Vec::with_capacity(modules.len());
    for (name, source) in &modules {
        let analysis = analyze_module_in_environment(
            SourceId::new(name.clone()),
            ModuleId::new(name.clone()),
            source,
            &environment,
        );
        if let Some(checked) = &analysis.checked {
            environment.insert(name.clone(), checked.interface.clone());
        }
        analyzed.push(AnalyzedModule {
            module: name.clone(),
            diagnostics: analysis
                .diagnostics
                .iter()
                .map(|diagnostic| AnalyzedDiagnostic {
                    rendered: render_diagnostic(source, diagnostic),
                    diagnostic: diagnostic.clone(),
                })
                .collect(),
            checked: analysis.checked,
        });
    }
    serde_json::to_string(&analyzed).map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Describe the bundled standard library: every module, and the words it
/// supplies. The Python mirror of the standard library is generated from this,
/// so the two cannot disagree about what a package exports.
#[pyfunction]
fn lab_standard_library() -> PyResult<String> {
    serde_json::to_string(&standard_library_manifest())
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Describe every adapter implementation and profile schema in this compiler build.
#[pyfunction]
fn lab_adapter_catalog() -> PyResult<String> {
    let catalog = lab_adapters::adapter_catalog()
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    serde_json::to_string(&catalog).map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Validate and canonicalize one operational adapter profile through its explicit driver.
#[pyfunction]
fn validate_lab_adapter_profile(driver: &str, name: &str, contents: &str) -> PyResult<String> {
    let profile = lab_adapters::validate_adapter_profile(driver, name, contents)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    serde_json::to_string(&profile).map_err(|error| PyValueError::new_err(error.to_string()))
}

fn parse_method_definitions(definitions_json: &str) -> PyResult<Vec<MethodDefinition>> {
    serde_json::from_str::<Vec<MethodDefinition>>(definitions_json)
        .map_err(|error| py_error("invalid Method definitions", &error))
}

fn validate_method_catalog(
    mut definitions: Vec<MethodDefinition>,
) -> PyResult<Vec<MethodDefinition>> {
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    MethodRegistry::new(definitions.clone())
        .map_err(|error| py_error("invalid Method catalog", &error))?;
    Ok(definitions)
}

fn method_definitions(
    definitions_json: &str,
    include_standard: bool,
) -> PyResult<Vec<MethodDefinition>> {
    let mut definitions = if include_standard {
        standard_method_definitions()
    } else {
        Vec::new()
    };
    definitions.extend(parse_method_definitions(definitions_json)?);
    validate_method_catalog(definitions)
}

fn method_registry(definitions_json: &str, include_standard: bool) -> PyResult<MethodRegistry> {
    MethodRegistry::new(method_definitions(definitions_json, include_standard)?)
        .map_err(|error| py_error("invalid Method catalog", &error))
}

fn project_method_registry(
    project: &LabProject,
    definitions_json: &str,
    include_standard: bool,
) -> PyResult<MethodRegistry> {
    let mut definitions = if include_standard {
        standard_method_definitions()
    } else {
        Vec::new()
    };
    definitions.extend(
        project
            .package_method_definitions()
            .map_err(|error| py_error("failed to load package Method catalogs", &error))?,
    );
    definitions.extend(parse_method_definitions(definitions_json)?);
    MethodRegistry::new(validate_method_catalog(definitions)?)
        .map_err(|error| py_error("invalid Method catalog", &error))
}

/// Validate Python-authored portable Method definitions against the Rust contract.
#[pyfunction]
fn validate_method_definitions(definitions_json: &str, include_standard: bool) -> PyResult<String> {
    serde_json::to_string(&method_definitions(definitions_json, include_standard)?)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

fn compile_named_modules(modules: &[(String, String)]) -> PyResult<Vec<CheckedModule>> {
    let mut environment = SemanticEnvironment::default();
    let mut checked = Vec::with_capacity(modules.len());
    for (name, source) in modules {
        let analysis = analyze_module_in_environment(
            SourceId::new(name.clone()),
            ModuleId::new(name.clone()),
            source,
            &environment,
        );
        let Some(module) = analysis.checked else {
            let diagnostics = analysis
                .diagnostics
                .iter()
                .map(|diagnostic| render_diagnostic(source, diagnostic))
                .collect::<Vec<_>>()
                .join("\n\n");
            return Err(PyValueError::new_err(format!(
                "Lab module '{name}' did not check:\n\n{diagnostics}"
            )));
        };
        environment.insert(name.clone(), module.interface.clone());
        checked.push(module);
    }
    Ok(checked)
}

#[derive(Serialize)]
struct RefinedProgram {
    schema_version: &'static str,
    refined_lair: String,
    planning_problem: lab_compiler::planning::PlanningProblem,
}

/// Refine checked Python-emitted Lab modules through the shared Rust Method pipeline.
#[pyfunction]
fn refine_lab_modules(
    modules: Vec<(String, String)>,
    definitions_json: &str,
    include_standard: bool,
) -> PyResult<String> {
    let registry = method_registry(definitions_json, include_standard)?;
    let checked = compile_named_modules(&modules)?;
    let module_refs = checked.iter().collect::<Vec<_>>();
    let refined = PortableLairProgram::lower_program(&module_refs)
        .map_err(|error| py_error("failed to lower Lab Intent", &error))?
        .refine_methods(&registry)
        .map_err(|error| py_error("failed to refine Lab Intent", &error))?;
    let planning_problem = refined
        .planning_problem()
        .map_err(|error| py_error("failed to project planning", &error))?;
    serde_json::to_string(&RefinedProgram {
        schema_version: "lab.python-refinement.v1",
        refined_lair: refined.ir(),
        planning_problem,
    })
    .map_err(|error| PyValueError::new_err(error.to_string()))
}

#[derive(Serialize)]
struct PythonInventorySelection<'a> {
    document: &'a Path,
    sha256: &'a str,
    facility: &'a str,
}

#[derive(Serialize)]
struct PythonFacilityPlan<'a> {
    schema_version: &'static str,
    package: &'a str,
    version: &'a str,
    inventory: PythonInventorySelection<'a>,
    adapter_bindings: Option<&'a lab_facility::AdapterBindingSnapshot>,
    refined_lair: &'a str,
    planning_problem: &'a lab_compiler::planning::PlanningProblem,
    facility_solution: &'a lab_compiler::planning::FacilityPlanningSolution,
    allocated_lair: String,
    material_inventory: &'a lab_compiler::planning::MaterialLotInventory,
    adapter_invocations: &'a lab_adapters::AdapterInvocationPlan,
}

fn serialize_facility_plan(planned: &FacilityPlanningResult) -> PyResult<String> {
    serde_json::to_string(&PythonFacilityPlan {
        schema_version: "lab.python-facility-plan.v2",
        package: &planned.package,
        version: &planned.version,
        inventory: PythonInventorySelection {
            document: planned.inventory.source_path(),
            sha256: planned.inventory.source_sha256(),
            facility: planned.inventory.facility().as_str(),
        },
        adapter_bindings: planned.adapter_bindings.as_ref(),
        refined_lair: &planned.refined_lair,
        planning_problem: planned.problem(),
        facility_solution: planned.solution(),
        allocated_lair: planned.allocated.ir(),
        material_inventory: &planned.material_inventory,
        adapter_invocations: &planned.adapter_invocations,
    })
    .map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Compile and facility-plan the default package in a Lab project.
#[pyfunction]
fn plan_lab_project(
    path: &str,
    definitions_json: &str,
    include_standard: bool,
) -> PyResult<String> {
    let project = LabProject::discover(path)
        .map_err(|error| py_error("failed to load Lab project", &error))?;
    let compiled = project
        .compile()
        .map_err(|error| py_error("failed to compile Lab project", &error))?;
    let registry = project_method_registry(&project, definitions_json, include_standard)?;
    let planned = project
        .plan_facility(&compiled, &registry)
        .map_err(|error| py_error("failed to plan Lab project", &error))?;
    serialize_facility_plan(&planned)
}

/// Facility-plan checked Python-emitted modules using a Lab package as operational context.
#[pyfunction]
fn plan_lab_modules(
    modules: Vec<(String, String)>,
    package_path: &str,
    definitions_json: &str,
    include_standard: bool,
) -> PyResult<String> {
    let checked = compile_named_modules(&modules)?;
    let module_refs = checked.iter().collect::<Vec<_>>();
    let project = LabProject::discover(package_path)
        .map_err(|error| py_error("failed to load Lab project", &error))?;
    let registry = project_method_registry(&project, definitions_json, include_standard)?;
    let planned = plan_modules_for_package(project.default_package(), &module_refs, &registry)
        .map_err(|error| py_error("failed to plan Lab program", &error))?;
    serialize_facility_plan(&planned)
}

#[pymodule]
pub fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(compile_lab_module, module)?)?;
    module.add_function(wrap_pyfunction!(analyze_lab_modules, module)?)?;
    module.add_function(wrap_pyfunction!(lab_standard_library, module)?)?;
    module.add_function(wrap_pyfunction!(lab_adapter_catalog, module)?)?;
    module.add_function(wrap_pyfunction!(validate_lab_adapter_profile, module)?)?;
    module.add_function(wrap_pyfunction!(validate_method_definitions, module)?)?;
    module.add_function(wrap_pyfunction!(refine_lab_modules, module)?)?;
    module.add_function(wrap_pyfunction!(plan_lab_project, module)?)?;
    module.add_function(wrap_pyfunction!(plan_lab_modules, module)?)?;
    Ok(())
}

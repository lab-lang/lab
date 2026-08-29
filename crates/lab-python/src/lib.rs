//! Python extension module for Lab.

use lab_compiler::{
    CheckedModule, Diagnostic, ModuleId, PortableLairProgram, SemanticEnvironment, SourceId,
    analyze_module_in_environment, compile_module, render_diagnostic, standard_library_manifest,
    standard_method_definitions,
};
use lab_method::{MethodDefinition, MethodRegistry};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::Serialize;

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

fn method_definitions(
    definitions_json: &str,
    include_standard: bool,
) -> PyResult<Vec<MethodDefinition>> {
    let mut definitions = if include_standard {
        standard_method_definitions()
    } else {
        Vec::new()
    };
    let custom = serde_json::from_str::<Vec<MethodDefinition>>(definitions_json)
        .map_err(|error| PyValueError::new_err(format!("invalid Method definitions: {error}")))?;
    definitions.extend(custom);
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    MethodRegistry::new(definitions.clone())
        .map_err(|error| PyValueError::new_err(format!("invalid Method catalog: {error}")))?;
    Ok(definitions)
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
    let definitions = method_definitions(definitions_json, include_standard)?;
    let registry = MethodRegistry::new(definitions)
        .expect("method_definitions returned only a validated complete catalog");
    let checked = compile_named_modules(&modules)?;
    let module_refs = checked.iter().collect::<Vec<_>>();
    let refined = PortableLairProgram::lower_program(&module_refs)
        .map_err(|error| PyValueError::new_err(format!("failed to lower Lab Intent: {error}")))?
        .refine_methods(&registry)
        .map_err(|error| PyValueError::new_err(format!("failed to refine Lab Intent: {error}")))?;
    let planning_problem = refined
        .planning_problem()
        .map_err(|error| PyValueError::new_err(format!("failed to project planning: {error}")))?;
    serde_json::to_string(&RefinedProgram {
        schema_version: "lab.python-refinement.v1",
        refined_lair: refined.ir(),
        planning_problem,
    })
    .map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pymodule]
pub fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(compile_lab_module, module)?)?;
    module.add_function(wrap_pyfunction!(analyze_lab_modules, module)?)?;
    module.add_function(wrap_pyfunction!(lab_standard_library, module)?)?;
    module.add_function(wrap_pyfunction!(validate_method_definitions, module)?)?;
    module.add_function(wrap_pyfunction!(refine_lab_modules, module)?)?;
    Ok(())
}

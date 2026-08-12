//! Python extension module for Lab.

use lab_compiler::{
    CheckedModule, Diagnostic, ModuleId, SemanticEnvironment, SourceId,
    analyze_module_in_environment, compile_module, render_diagnostic, standard_library_manifest,
};
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

#[pymodule]
pub fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(compile_lab_module, module)?)?;
    module.add_function(wrap_pyfunction!(analyze_lab_modules, module)?)?;
    module.add_function(wrap_pyfunction!(lab_standard_library, module)?)?;
    Ok(())
}

//! PyO3 boundary for the Lab Python SDK.

use lab_sdk::{LabProfile, compile_lab_lang as compile_source};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Compile Lab Lang for Lab's reference laboratory profile.
///
/// The public Python wrapper converts the JSON plan into native Python data.
#[pyfunction]
fn compile_lab_lang(source: &str) -> PyResult<(String, String)> {
    let compilation = compile_source(source, &LabProfile::reference())
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let plan = serde_json::to_string(compilation.plan())
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok((compilation.ir(), plan))
}

#[pymodule]
pub fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(compile_lab_lang, module)?)?;
    Ok(())
}

//! PyO3 boundary for the Lab Python SDK.

use lab_sdk::compile_lab_module as compile_source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Parse, resolve, and type-check a Lab source module.
#[pyfunction]
fn compile_lab_module(source: &str) -> PyResult<String> {
    let module =
        compile_source(source).map_err(|error| PyValueError::new_err(error.to_string()))?;
    serde_json::to_string(&module).map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pymodule]
pub fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(compile_lab_module, module)?)?;
    Ok(())
}

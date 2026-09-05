//! Regenerate the checked-in standard-library bindings without importing Python.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: generate_standard OUTPUT_DIRECTORY")?;
    for file in
        lab_python_bindings::generate_standard_library(&lab_language::standard_library_manifest())?
    {
        let target = output.join(file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, file.source)?;
    }
    Ok(())
}

//! Complete registration and callbacks for byonoy.absorbance96.

use lab_compiler::procedure::ProcedureContractRegistry;

use crate::{AdapterInvocation, AdapterInvocationPlan};

use lab_adapter_api::{
    AdapterInvocationLowering, AdapterLoweringError, AdapterProfileContractError,
    AdapterRegistration, ValidatedAdapterProfile,
};

use crate::backend::adapters::{
    EmptyAdapterProfile, declared_program_feasible, descriptor, schema_value, validate_empty_for,
};

pub fn registration() -> Result<AdapterRegistration, AdapterProfileContractError> {
    let descriptor = descriptor(
        "byonoy.absorbance96",
        "Byonoy Absorbance 96",
        Some("Byonoy"),
        ["hid", "plate-reader"],
        Vec::new(),
        schema_value::<EmptyAdapterProfile>()?,
        validate_byonoy_profile("byonoy.absorbance96", "")?,
    )?;
    Ok(AdapterRegistration::new(
        descriptor,
        validate_byonoy_profile,
        declared_program_feasible,
        unsupported_invocation,
    ))
}

fn validate_byonoy_profile(
    name: &str,
    contents: &str,
) -> Result<ValidatedAdapterProfile, AdapterProfileContractError> {
    validate_empty_for("byonoy.absorbance96", name, contents)
}

pub(in crate::backend) fn unsupported_invocation(
    profile: &ValidatedAdapterProfile,
    _plan: &AdapterInvocationPlan,
    _invocation: &AdapterInvocation,
    _contracts: &ProcedureContractRegistry,
) -> Result<AdapterInvocationLowering, AdapterLoweringError> {
    Err(AdapterLoweringError::UnsupportedInvocation {
        driver: profile.driver.clone(),
    })
}

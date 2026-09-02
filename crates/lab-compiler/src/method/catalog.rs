use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::method::{MethodDefinition, MethodRegistry, MethodRegistryError};

/// The exact serialization contract for portable Method documents consumed by Lab packages.
pub const METHOD_CATALOG_SCHEMA_VERSION: &str = "lab.method-catalog.v1";

/// A versioned, portable collection of facility-independent Method definitions.
///
/// This document is deliberately independent of `lab.toml`: packages name one or more catalog
/// documents, while this shared contract carries the semantic definitions used by Rust and Python.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodCatalogDocument {
    pub schema_version: String,
    #[serde(default)]
    pub methods: Vec<MethodDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum MethodCatalogError {
    #[error(
        "unsupported Method catalog schema '{actual}'; expected '{METHOD_CATALOG_SCHEMA_VERSION}'"
    )]
    UnsupportedSchema { actual: String },
    #[error("invalid Method definitions: {0}")]
    InvalidDefinitions(#[from] MethodRegistryError),
}

impl MethodCatalogDocument {
    /// Construct and validate one catalog under the current serialization contract.
    pub fn new(methods: Vec<MethodDefinition>) -> Result<Self, MethodCatalogError> {
        let document = Self {
            schema_version: METHOD_CATALOG_SCHEMA_VERSION.to_owned(),
            methods,
        };
        document.validate()?;
        Ok(document)
    }

    /// Validate the document version and every Method graph without changing its order.
    pub fn validate(&self) -> Result<(), MethodCatalogError> {
        if self.schema_version != METHOD_CATALOG_SCHEMA_VERSION {
            return Err(MethodCatalogError::UnsupportedSchema {
                actual: self.schema_version.clone(),
            });
        }
        MethodRegistry::new(self.methods.clone())?;
        Ok(())
    }

    /// Validate the document and return its portable definitions for registry composition.
    pub fn into_methods(self) -> Result<Vec<MethodDefinition>, MethodCatalogError> {
        self.validate()?;
        Ok(self.methods)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_catalog_round_trips_with_an_explicit_version() {
        let catalog = MethodCatalogDocument::new(Vec::new()).unwrap();
        let json = serde_json::to_string_pretty(&catalog).unwrap();
        let reparsed: MethodCatalogDocument = serde_json::from_str(&json).unwrap();

        assert_eq!(reparsed.schema_version, METHOD_CATALOG_SCHEMA_VERSION);
        assert!(reparsed.methods.is_empty());
        reparsed.validate().unwrap();
    }

    #[test]
    fn an_unknown_schema_fails_closed_before_projection() {
        let catalog = MethodCatalogDocument {
            schema_version: "lab.method-catalog.v99".to_owned(),
            methods: Vec::new(),
        };

        assert!(matches!(
            catalog.validate(),
            Err(MethodCatalogError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn unknown_document_fields_are_rejected() {
        let error = serde_json::from_str::<MethodCatalogDocument>(
            r#"{"schema_version":"lab.method-catalog.v1","methods":[],"methodz":[]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }
}

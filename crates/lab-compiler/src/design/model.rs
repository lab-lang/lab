//! Lossless, package-neutral artifact designs.
//!
//! The source frontend has already resolved and type-checked every field in
//! this document. The compiler therefore preserves it as data instead of
//! translating package vocabulary into Rust variants.

use lab_language::{
    CheckedAcceptance, CheckedArtifactFacet, CheckedProperty, CheckedType, DefinitionId,
    TypedExpression,
};
use serde::{Deserialize, Serialize};

pub const ARTIFACT_DESIGN_SCHEMA_VERSION: &str = "lab.artifact-design.v1";

/// One artifact declaration at the Design/Intent boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDesign {
    pub schema_version: String,
    /// Exact identity of the artifact instance declaration.
    pub definition: DefinitionId,
    pub name: String,
    /// Source word used to declare this kind.
    pub artifact: String,
    /// Every exact kind declaration contributing to the checked merged schema.
    pub artifact_definitions: Vec<DefinitionId>,
    pub produces: CheckedType,
    /// Exact identity of the nominal produced type.
    pub type_definition: DefinitionId,
    pub facets: Vec<CheckedArtifactFacet>,
    pub sbol_identity: Option<String>,
    pub properties: Vec<CheckedProperty>,
    pub requirements: Vec<TypedExpression>,
    pub acceptance: Vec<CheckedAcceptance>,
}

impl ArtifactDesign {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ARTIFACT_DESIGN_SCHEMA_VERSION {
            return Err(format!(
                "expected artifact design schema '{}', found '{}'",
                ARTIFACT_DESIGN_SCHEMA_VERSION, self.schema_version
            ));
        }
        if self.name.is_empty() || self.artifact.is_empty() {
            return Err("artifact design name and source kind must be non-empty".to_owned());
        }
        if self.artifact_definitions.is_empty() {
            return Err("artifact design must retain at least one kind definition".to_owned());
        }
        if self.definition.module.as_str().is_empty()
            || self.definition.local.is_empty()
            || self.type_definition.module.as_str().is_empty()
            || self.type_definition.local.is_empty()
            || self.artifact_definitions.iter().any(|definition| {
                definition.module.as_str().is_empty() || definition.local.is_empty()
            })
        {
            return Err("artifact design definition identities must be non-empty".to_owned());
        }
        if self
            .sbol_identity
            .as_ref()
            .is_some_and(|identity| lab_capability::AbsoluteIri::new(identity).is_err())
        {
            return Err("artifact design SBOL identity must be an absolute IRI".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn synthetic_artifact_design(name: &str) -> ArtifactDesign {
    let module = "compiler.synthetic";
    ArtifactDesign {
        schema_version: ARTIFACT_DESIGN_SCHEMA_VERSION.to_owned(),
        definition: DefinitionId::exported(module, name),
        name: name.to_owned(),
        artifact: "artifact".to_owned(),
        artifact_definitions: vec![DefinitionId::exported(module, "artifact")],
        produces: CheckedType::Named {
            name: "Artifact".to_owned(),
            arguments: vec![],
        },
        type_definition: DefinitionId::exported(module, "Artifact"),
        facets: vec![],
        sbol_identity: None,
        properties: vec![],
        requirements: vec![],
        acceptance: vec![],
    }
}

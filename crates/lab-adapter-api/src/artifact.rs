//! In-memory artifacts emitted by one adapter invocation.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One generated file in an adapter artifact bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedArtifact {
    path: String,
    media_type: String,
    contents: Vec<u8>,
}

impl GeneratedArtifact {
    pub fn text(
        path: impl Into<String>,
        media_type: impl Into<String>,
        contents: impl Into<String>,
    ) -> Result<Self, ArtifactError> {
        Self::bytes(path, media_type, contents.into().into_bytes())
    }

    pub fn bytes(
        path: impl Into<String>,
        media_type: impl Into<String>,
        contents: Vec<u8>,
    ) -> Result<Self, ArtifactError> {
        let path = path.into();
        validate_package_path(&path)?;
        Ok(Self {
            path,
            media_type: media_type.into(),
            contents,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    pub fn text_contents(&self) -> Result<&str, ArtifactError> {
        std::str::from_utf8(&self.contents).map_err(|_| ArtifactError::NotUtf8 {
            path: self.path.clone(),
        })
    }
}

/// A collision-checked set of files emitted by one adapter lowerer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactBundle {
    artifacts: BTreeMap<String, GeneratedArtifact>,
}

impl ArtifactBundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, artifact: GeneratedArtifact) -> Result<(), ArtifactError> {
        let path = artifact.path.clone();
        match self.artifacts.entry(path.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(artifact);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(ArtifactError::DuplicatePath(path))
            }
        }
    }

    pub fn insert_text(
        &mut self,
        path: impl Into<String>,
        media_type: impl Into<String>,
        contents: impl Into<String>,
    ) -> Result<(), ArtifactError> {
        self.insert(GeneratedArtifact::text(path, media_type, contents)?)
    }

    pub fn get(&self, path: &str) -> Option<&GeneratedArtifact> {
        self.artifacts.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = &GeneratedArtifact> {
        self.artifacts.values()
    }

    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.artifacts.len()
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ArtifactError {
    #[error("artifact path must be a non-empty relative package path, found '{0}'")]
    InvalidPath(String),
    #[error("artifact bundle contains duplicate path '{0}'")]
    DuplicatePath(String),
    #[error("artifact '{path}' does not contain UTF-8 text")]
    NotUtf8 { path: String },
}

fn validate_package_path(path: &str) -> Result<(), ArtifactError> {
    let path_value = Path::new(path);
    if path.is_empty()
        || path_value.is_absolute()
        || path_value
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ArtifactError::InvalidPath(path.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_cannot_escape_and_cannot_collide() {
        assert!(matches!(
            GeneratedArtifact::text("../protocol.py", "text/x-python", "pass"),
            Err(ArtifactError::InvalidPath(_))
        ));
        let mut bundle = ArtifactBundle::new();
        bundle
            .insert_text("protocol.py", "text/x-python", "original")
            .unwrap();
        assert!(matches!(
            bundle.insert_text("protocol.py", "text/x-python", "replacement"),
            Err(ArtifactError::DuplicatePath(_))
        ));
        assert_eq!(
            bundle.get("protocol.py").unwrap().text_contents().unwrap(),
            "original"
        );
        assert!(matches!(
            GeneratedArtifact::text(".", "text/plain", "invalid"),
            Err(ArtifactError::InvalidPath(_))
        ));
    }
}

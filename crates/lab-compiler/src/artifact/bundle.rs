use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
        if self.artifacts.insert(path.clone(), artifact).is_some() {
            return Err(ArtifactError::DuplicatePath(path));
        }
        Ok(())
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
        || path_value.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ArtifactError::InvalidPath(path.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_escape_the_package() {
        assert_eq!(
            GeneratedArtifact::text("../protocol.py", "text/x-python", "pass"),
            Err(ArtifactError::InvalidPath("../protocol.py".into()))
        );
    }

    #[test]
    fn rejects_duplicate_artifact_paths() {
        let mut bundle = ArtifactBundle::new();
        bundle
            .insert_text("protocol.py", "text/x-python", "pass")
            .unwrap();
        assert_eq!(
            bundle.insert_text("protocol.py", "text/x-python", "pass"),
            Err(ArtifactError::DuplicatePath("protocol.py".into()))
        );
    }
}

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::PackageError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub package: PackageMetadata,
    #[serde(default)]
    pub build: BuildMetadata,
    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadata {
    pub name: String,
    pub version: String,
    #[serde(default = "default_edition")]
    pub edition: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildMetadata {
    pub entry: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    Version(String),
    Detailed(DependencyDetail),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyDetail {
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    pub registry: Option<String>,
}

impl PackageManifest {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub(crate) fn validate(&self) -> Result<(), PackageError> {
        if !valid_package_name(&self.package.name) {
            return Err(PackageError::InvalidName(self.package.name.clone()));
        }
        semver::Version::parse(&self.package.version).map_err(|source| {
            PackageError::InvalidVersion {
                version: self.package.version.clone(),
                source,
            }
        })?;
        if self.package.edition != "2026" {
            return Err(PackageError::UnsupportedEdition(
                self.package.edition.clone(),
            ));
        }
        for (name, dependency) in &self.dependencies {
            if !valid_package_name(name) {
                return Err(PackageError::InvalidDependency {
                    name: name.clone(),
                    message: "dependency names use the same characters as package names".to_owned(),
                });
            }
            match dependency {
                DependencySpec::Version(requirement) => {
                    semver::VersionReq::parse(requirement).map_err(|error| {
                        PackageError::InvalidDependency {
                            name: name.clone(),
                            message: format!(
                                "invalid version requirement '{requirement}': {error}"
                            ),
                        }
                    })?;
                }
                DependencySpec::Detailed(detail) => {
                    if detail.path.is_none() && detail.version.is_none() {
                        return Err(PackageError::InvalidDependency {
                            name: name.clone(),
                            message: "expected 'path' or 'version'".to_owned(),
                        });
                    }
                    if detail.path.is_some() && detail.registry.is_some() {
                        return Err(PackageError::InvalidDependency {
                            name: name.clone(),
                            message: "a path dependency cannot also select a registry".to_owned(),
                        });
                    }
                    if let Some(requirement) = &detail.version {
                        semver::VersionReq::parse(requirement).map_err(|error| {
                            PackageError::InvalidDependency {
                                name: name.clone(),
                                message: format!(
                                    "invalid version requirement '{requirement}': {error}"
                                ),
                            }
                        })?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn default_edition() -> String {
    "2026".to_owned()
}

fn valid_package_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_dependencies_and_defaults() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[build]
entry = "src/programs/main.lab"

[dependencies]
parts = "1.2"
policies = { path = "../policies" }
"#,
        )
        .unwrap();

        assert_eq!(manifest.package.edition, "2026");
        assert_eq!(manifest.dependencies.len(), 2);
        assert!(matches!(
            manifest.dependencies["parts"],
            DependencySpec::Version(_)
        ));
        manifest.validate().unwrap();
    }

    #[test]
    fn rejects_incoherent_dependency_sources() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
parts = { path = "../parts", registry = "private" }
"#,
        )
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PackageError::InvalidDependency { .. })
        ));
    }
}

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::PackageError;

/// One `lab.toml`. A manifest describes either a package or a workspace of
/// member packages, never both: a workspace root owns membership and nothing
/// else, so member packages stay ordinary self-contained packages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabManifest {
    Package(PackageManifest),
    Workspace(WorkspaceManifest),
}

impl LabManifest {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let table = text.parse::<toml::Table>()?;
        if table.contains_key("workspace") {
            Ok(Self::Workspace(WorkspaceManifest::parse(text)?))
        } else {
            Ok(Self::Package(PackageManifest::parse(text)?))
        }
    }

    pub(crate) fn validate(&self) -> Result<(), PackageError> {
        match self {
            Self::Package(manifest) => manifest.validate(),
            Self::Workspace(manifest) => manifest.validate(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceManifest {
    pub workspace: WorkspaceMetadata,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMetadata {
    /// Member package directories, relative to the workspace root.
    #[serde(default)]
    pub members: Vec<PathBuf>,
    /// Member selected when a command that acts on one package is given the
    /// workspace root. Required once a workspace has more than one member.
    #[serde(default, rename = "default-member")]
    pub default_member: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub package: PackageMetadata,
    #[serde(default)]
    pub build: BuildMetadata,
    #[serde(default)]
    pub inventory: InventoryMetadata,
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
    /// Target profile a build compiles for when none is named on the command
    /// line, resolved by filename under `targets/`. A package without one
    /// builds portable module IR and stops.
    pub target: Option<String>,
    /// Facility a simulation runs against when none is named on the command
    /// line, resolved by filename under `facilities/`. A facility describes
    /// the lab — stations, storage, transport — and is shared by every
    /// package that runs there, so it lives in its own file and the
    /// manifest carries only this pointer.
    pub facility: Option<String>,
}

/// What a target build may draw on before it plans anything: the materials an
/// operator has on hand and the artifacts already realized. Names are the
/// symbolic identities `src/` declares, so the inventory reads as a statement
/// about this package's stock rather than as an opaque data file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryMetadata {
    #[serde(default)]
    pub materials: BTreeSet<String>,
    #[serde(default)]
    pub artifacts: BTreeSet<String>,
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

impl WorkspaceManifest {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub(crate) fn validate(&self) -> Result<(), PackageError> {
        if self.workspace.members.is_empty() {
            return Err(PackageError::EmptyWorkspace);
        }
        for member in &self.workspace.members {
            if member.is_absolute() {
                return Err(PackageError::InvalidWorkspaceMember {
                    member: member.clone(),
                    message: "member paths are relative to the workspace root".to_owned(),
                });
            }
        }
        if let Some(default) = &self.workspace.default_member
            && !self.workspace.members.contains(default)
        {
            return Err(PackageError::InvalidWorkspaceMember {
                member: default.clone(),
                message: "default-member is not one of the declared members".to_owned(),
            });
        }
        if self.workspace.members.len() > 1 && self.workspace.default_member.is_none() {
            return Err(PackageError::AmbiguousDefaultMember);
        }
        Ok(())
    }

    /// The member a single-package command acts on.
    pub fn default_member(&self) -> &PathBuf {
        self.workspace
            .default_member
            .as_ref()
            .or_else(|| self.workspace.members.first())
            .expect("workspace validation rejects an empty member list")
    }
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
        // A target is a filename under `targets/`, so it must not be able to
        // reach outside that directory.
        if let Some(target) = &self.build.target
            && !valid_target_name(target)
        {
            return Err(PackageError::InvalidTarget(target.clone()));
        }
        // A facility is a filename under `facilities/`, held to the same
        // rule as targets: it must not be able to reach outside.
        if let Some(facility) = &self.build.facility
            && !valid_target_name(facility)
        {
            return Err(PackageError::InvalidFacility(facility.clone()));
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

fn valid_target_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
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
    use crate::manifest::*;

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
    fn distinguishes_a_workspace_root_from_a_package() {
        let workspace = LabManifest::parse(
            r#"[workspace]
members = ["packages/catalog", "packages/device"]
default-member = "packages/device"
"#,
        )
        .unwrap();
        let LabManifest::Workspace(workspace) = workspace else {
            panic!("a manifest with a [workspace] table is a workspace root");
        };
        assert_eq!(workspace.workspace.members.len(), 2);
        assert_eq!(
            workspace.default_member(),
            &PathBuf::from("packages/device")
        );
        workspace.validate().unwrap();

        let package =
            LabManifest::parse("[package]\nname = \"device\"\nversion = \"0.1.0\"\n").unwrap();
        assert!(matches!(package, LabManifest::Package(_)));
    }

    #[test]
    fn rejects_a_default_member_that_is_not_a_member() {
        let manifest = WorkspaceManifest::parse(
            "[workspace]\nmembers = [\"packages/catalog\"]\ndefault-member = \"packages/device\"\n",
        )
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PackageError::InvalidWorkspaceMember { .. })
        ));
    }

    #[test]
    fn reads_the_default_target_and_rejects_one_that_is_not_a_profile_name() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[build]
entry = "src/programs/main.lab"
target = "opentrons-ot2"
"#,
        )
        .unwrap();
        assert_eq!(manifest.build.target.as_deref(), Some("opentrons-ot2"));
        manifest.validate().unwrap();

        let escaping = PackageManifest::parse(
            "[package]\nname = \"tet-reporter\"\nversion = \"0.1.0\"\n\n[build]\ntarget = \"../benches/ot2\"\n",
        )
        .unwrap();
        assert!(matches!(
            escaping.validate(),
            Err(PackageError::InvalidTarget(_))
        ));
    }

    #[test]
    fn reads_the_default_facility_and_holds_it_to_the_target_name_rule() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[build]
facility = "main-bench"
"#,
        )
        .unwrap();
        assert_eq!(manifest.build.facility.as_deref(), Some("main-bench"));
        manifest.validate().unwrap();

        let escaping = PackageManifest::parse(
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n\n[build]\nfacility = \"../elsewhere\"\n",
        )
        .unwrap();
        assert!(matches!(
            escaping.validate(),
            Err(PackageError::InvalidFacility(_))
        ));
    }

    #[test]
    fn reads_the_inventory_a_target_build_resolves_against() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[inventory]
materials = ["BsaI", "pSB1C3", "BsaI"]
artifacts = ["composite_plasmid_1"]
"#,
        )
        .unwrap();
        assert_eq!(manifest.inventory.materials.len(), 2, "names are a set");
        assert!(manifest.inventory.materials.contains("pSB1C3"));
        assert_eq!(
            manifest.inventory.artifacts.iter().collect::<Vec<_>>(),
            ["composite_plasmid_1"]
        );
        manifest.validate().unwrap();

        let empty =
            PackageManifest::parse("[package]\nname = \"t\"\nversion = \"0.1.0\"\n").unwrap();
        assert_eq!(empty.inventory, InventoryMetadata::default());

        let misspelled = PackageManifest::parse(
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n\n[inventory]\nmaterial = [\"BsaI\"]\n",
        );
        assert!(
            misspelled.is_err(),
            "a misspelled key must not silently empty the inventory"
        );
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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, PathBuf};

use serde::{Deserialize, Serialize};

use crate::PackageError;

/// One `lab.toml`. A manifest describes either a package or a workspace of
/// member packages, never both: a workspace root owns membership and nothing
/// else, so member packages stay ordinary self-contained packages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabManifest {
    Package(Box<PackageManifest>),
    Workspace(WorkspaceManifest),
}

impl LabManifest {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let table = text.parse::<toml::Table>()?;
        if table.contains_key("workspace") {
            Ok(Self::Workspace(WorkspaceManifest::parse(text)?))
        } else {
            Ok(Self::Package(Box::new(PackageManifest::parse(text)?)))
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
    pub methods: MethodCatalogMetadata,
    #[serde(default)]
    pub planning: PlanningMetadata,
    #[serde(default)]
    pub inventory: InventoryMetadata,
    #[serde(default)]
    pub execution: ExecutionMetadata,
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

/// Portable Method documents contributed by this package.
///
/// Each JSON document uses the versioned LAIR Method catalog contract. Dependencies contribute
/// their documents before their consumers, and the project composes the complete set with Lab's
/// standard Methods before refinement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodCatalogMetadata {
    #[serde(default)]
    pub documents: Vec<PathBuf>,
}

/// Explicit policy for resolving method and facility choices.
///
/// Method definitions live in versioned catalog documents. Planning configuration only selects
/// among the resulting alternatives by stable source operation, choice, and Method identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningMetadata {
    #[serde(default, rename = "adapter-requirement")]
    pub adapter_requirement: PlanningAdapterRequirement,
    #[serde(default)]
    pub methods: Vec<MethodPinMetadata>,
    /// Which Asset satisfies a requirement when a facility offers more than one that could.
    #[serde(default)]
    pub assets: Vec<AssetPinMetadata>,
}

/// Selects one Asset for a capability kind or for one exact requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPinMetadata {
    /// Pin every requirement demanding this capability kind.
    #[serde(default, rename = "capability-kind")]
    pub capability_kind: Option<String>,
    /// Pin one exact requirement ID from the emitted planning problem.
    #[serde(default)]
    pub requirement: Option<String>,
    /// Exact Asset IRI selected for the matching requirement or requirements.
    pub asset: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanningAdapterRequirement {
    /// Freeze a compatible configured adapter when one exists.
    #[default]
    Optional,
    /// Require a configured planning adapter for every non-manual offering.
    NonManual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodPinMetadata {
    /// Pin every reachable occurrence of this exact frontend Intent operation.
    #[serde(default, rename = "source-operation")]
    pub source_operation: Option<String>,
    /// Pin one exact choice ID from the emitted planning problem.
    #[serde(default)]
    pub choice: Option<String>,
    /// Exact method identity selected for the matching choice or choices.
    pub method: String,
}

/// The facility catalog a package may plan against.
///
/// `document` selects the portable SBOLInventory graph and `facility` disambiguates
/// that graph when it contains several facilities.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryMetadata {
    pub document: Option<PathBuf>,
    pub facility: Option<String>,
}

/// Local operational bindings from exact SBOLInventory Assets to Lab adapters.
///
/// These records do not describe the facility. Manufacturer, model, capabilities, qualification,
/// and control mode remain facts in the inventory graph. A binding only states which installed
/// Lab implementation and non-secret profile may operate one exact catalog Asset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionMetadata {
    #[serde(default)]
    pub adapters: Vec<AdapterBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterBinding {
    pub asset: String,
    pub driver: String,
    pub profile: PathBuf,
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
        self.methods.validate()?;
        self.planning.validate()?;
        self.inventory.validate()?;
        self.execution.validate(&self.inventory)?;
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

impl MethodCatalogMetadata {
    fn validate(&self) -> Result<(), PackageError> {
        let mut documents = BTreeSet::new();
        for document in &self.documents {
            if !valid_relative_path(document) {
                return Err(PackageError::InvalidMethods(format!(
                    "document '{}' must be a package-relative path without '..'",
                    document.display()
                )));
            }
            if document
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("json")
            {
                return Err(PackageError::InvalidMethods(format!(
                    "document '{}' must use the versioned JSON Method catalog format",
                    document.display()
                )));
            }
            if !documents.insert(document) {
                return Err(PackageError::InvalidMethods(format!(
                    "document '{}' is declared more than once",
                    document.display()
                )));
            }
        }
        Ok(())
    }
}

impl PlanningMetadata {
    fn validate(&self) -> Result<(), PackageError> {
        let mut selectors = BTreeSet::new();
        for pin in &self.methods {
            let selector = match (&pin.source_operation, &pin.choice) {
                (Some(source_operation), None) if valid_local_id(source_operation) => {
                    format!("source-operation:{source_operation}")
                }
                (None, Some(choice)) if valid_local_id(choice) => format!("choice:{choice}"),
                (Some(_), Some(_)) => {
                    return Err(PackageError::InvalidPlanning(
                        "a method pin must declare exactly one of 'source-operation' or 'choice'"
                            .to_owned(),
                    ));
                }
                _ => {
                    return Err(PackageError::InvalidPlanning(
                        "a method pin needs one non-empty 'source-operation' or 'choice' without whitespace"
                            .to_owned(),
                    ));
                }
            };
            if !valid_absolute_iri(&pin.method) {
                return Err(PackageError::InvalidPlanning(format!(
                    "method '{}' must be an absolute IRI",
                    pin.method
                )));
            }
            if !selectors.insert(selector.clone()) {
                return Err(PackageError::InvalidPlanning(format!(
                    "method selector '{selector}' is declared more than once"
                )));
            }
        }
        let mut asset_selectors = BTreeSet::new();
        for pin in &self.assets {
            let selector = match (&pin.capability_kind, &pin.requirement) {
                (Some(capability_kind), None) if valid_absolute_iri(capability_kind) => {
                    format!("capability-kind:{capability_kind}")
                }
                (None, Some(requirement)) if valid_local_id(requirement) => {
                    format!("requirement:{requirement}")
                }
                (Some(_), Some(_)) => {
                    return Err(PackageError::InvalidPlanning(
                        "an asset pin must declare exactly one of 'capability-kind' or 'requirement'"
                            .to_owned(),
                    ));
                }
                (None, None) => "any-requirement".to_owned(),
                _ => {
                    return Err(PackageError::InvalidPlanning(
                        "an asset pin needs an absolute 'capability-kind' IRI or a non-empty 'requirement', or neither to bind every requirement the Asset can serve"
                            .to_owned(),
                    ));
                }
            };
            if !valid_absolute_iri(&pin.asset) {
                return Err(PackageError::InvalidPlanning(format!(
                    "asset '{}' must be an absolute IRI",
                    pin.asset
                )));
            }
            if !asset_selectors.insert(selector.clone()) {
                return Err(PackageError::InvalidPlanning(format!(
                    "asset selector '{selector}' is declared more than once"
                )));
            }
        }
        Ok(())
    }
}

impl InventoryMetadata {
    fn validate(&self) -> Result<(), PackageError> {
        if let Some(document) = &self.document {
            let invalid = document.as_os_str().is_empty()
                || document.is_absolute()
                || document.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                });
            if invalid {
                return Err(PackageError::InvalidInventory(format!(
                    "document '{}' must be a package-relative path without '..'",
                    document.display()
                )));
            }
        } else if self.facility.is_some() {
            return Err(PackageError::InvalidInventory(
                "'facility' requires an inventory 'document'".to_owned(),
            ));
        }
        Ok(())
    }
}

impl ExecutionMetadata {
    fn validate(&self, inventory: &InventoryMetadata) -> Result<(), PackageError> {
        if !self.adapters.is_empty() && inventory.document.is_none() {
            return Err(PackageError::InvalidExecution(
                "adapter bindings require an SBOLInventory 'document'".to_owned(),
            ));
        }

        let mut bindings = BTreeSet::new();
        for adapter in &self.adapters {
            if !valid_absolute_iri(&adapter.asset) {
                return Err(PackageError::InvalidExecution(format!(
                    "adapter asset '{}' must be an absolute IRI",
                    adapter.asset
                )));
            }
            if !valid_adapter_id(&adapter.driver) {
                return Err(PackageError::InvalidExecution(format!(
                    "adapter driver '{}' must be a lowercase dotted identifier",
                    adapter.driver
                )));
            }
            if !valid_relative_path(&adapter.profile) {
                return Err(PackageError::InvalidExecution(format!(
                    "adapter profile '{}' must be a package-relative path without '..'",
                    adapter.profile.display()
                )));
            }
            if !bindings.insert((&adapter.asset, &adapter.driver)) {
                return Err(PackageError::InvalidExecution(format!(
                    "asset '{}' binds adapter '{}' more than once",
                    adapter.asset, adapter.driver
                )));
            }
        }
        Ok(())
    }
}

fn default_edition() -> String {
    "2026".to_owned()
}

fn valid_adapter_id(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
                })
        })
}

fn valid_local_id(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

fn valid_absolute_iri(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    !rest.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        && scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
        && !value.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(
                    character,
                    '<' | '>' | '"' | '{' | '}' | '|' | '\\' | '^' | '`'
                )
        })
}

fn valid_relative_path(path: &std::path::Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
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
    fn build_metadata_rejects_the_removed_target_selector() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[build]
entry = "src/programs/main.lab"
target = "opentrons-ot2"
"#,
        );
        assert!(
            manifest.is_err(),
            "facility allocation replaces build targets"
        );
    }

    #[test]
    fn rejects_the_removed_symbolic_inventory() {
        let materials = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[inventory]
materials = ["BsaI", "pSB1C3", "BsaI"]
artifacts = ["composite_plasmid_1"]
"#,
        );
        assert!(
            materials.is_err(),
            "symbolic inventory is not physical evidence"
        );

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
    fn reads_an_sbol_inventory_document_and_optional_facility() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[inventory]
document = "inventory/ebef.ttl"
facility = "https://example.org/ebef/facility"
"#,
        )
        .unwrap();

        assert_eq!(
            manifest.inventory.document.as_deref(),
            Some(std::path::Path::new("inventory/ebef.ttl"))
        );
        assert_eq!(
            manifest.inventory.facility.as_deref(),
            Some("https://example.org/ebef/facility")
        );
        manifest.validate().unwrap();
    }

    #[test]
    fn reads_exact_method_pins_and_adapter_policy() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "golden-gate"
version = "0.1.0"

[planning]
adapter-requirement = "non-manual"

[[planning.methods]]
source-operation = "std.bio.build.realize"
method = "https://www.lab-compiler.org/ns/method#automated-golden-gate"

[[planning.methods]]
choice = "choice-17"
method = "https://www.lab-compiler.org/ns/method#controlled-recovery"
"#,
        )
        .unwrap();

        assert_eq!(
            manifest.planning.adapter_requirement,
            PlanningAdapterRequirement::NonManual
        );
        assert_eq!(manifest.planning.methods.len(), 2);
        assert_eq!(
            manifest.planning.methods[0].source_operation.as_deref(),
            Some("std.bio.build.realize")
        );
        assert_eq!(
            manifest.planning.methods[1].choice.as_deref(),
            Some("choice-17")
        );
        manifest.validate().unwrap();
    }

    #[test]
    fn reads_portable_method_catalog_documents_separately_from_planning_policy() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "golden-gate"
version = "0.1.0"

[methods]
documents = ["methods/liquid-handling.json", "methods/incubation.json"]

[[planning.methods]]
source-operation = "std.bio.build.realize"
method = "https://example.org/method/automated-golden-gate"
"#,
        )
        .unwrap();

        assert_eq!(
            manifest.methods.documents,
            [
                PathBuf::from("methods/liquid-handling.json"),
                PathBuf::from("methods/incubation.json")
            ]
        );
        assert_eq!(manifest.planning.methods.len(), 1);
        manifest.validate().unwrap();
    }

    #[test]
    fn rejects_escaping_duplicate_or_non_json_method_documents() {
        for documents in [
            "[\"../methods.json\"]",
            "[\"methods/catalog.toml\"]",
            "[\"methods/catalog.json\", \"methods/catalog.json\"]",
        ] {
            let manifest = PackageManifest::parse(&format!(
                "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[methods]\ndocuments = {documents}\n"
            ))
            .unwrap();

            assert!(matches!(
                manifest.validate(),
                Err(PackageError::InvalidMethods(_))
            ));
        }
    }

    #[test]
    fn rejects_ambiguous_duplicate_or_non_iri_method_pins() {
        let both = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[[planning.methods]]
source-operation = "std.bio.build.realize"
choice = "choice-17"
method = "https://example.org/method"
"#,
        )
        .unwrap();
        assert!(matches!(
            both.validate(),
            Err(PackageError::InvalidPlanning(_))
        ));

        let duplicate = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[[planning.methods]]
source-operation = "std.bio.build.realize"
method = "https://example.org/manual"

[[planning.methods]]
source-operation = "std.bio.build.realize"
method = "https://example.org/automated"
"#,
        )
        .unwrap();
        assert!(matches!(
            duplicate.validate(),
            Err(PackageError::InvalidPlanning(_))
        ));

        let relative = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[[planning.methods]]
choice = "choice-17"
method = "automated-golden-gate"
"#,
        )
        .unwrap();
        assert!(matches!(
            relative.validate(),
            Err(PackageError::InvalidPlanning(_))
        ));
    }

    #[test]
    fn reads_explicit_adapter_bindings_to_exact_assets() {
        let manifest = PackageManifest::parse(
            r#"[package]
name = "tet-reporter"
version = "0.1.0"

[inventory]
document = "inventory/facility.ttl"

[[execution.adapters]]
asset = "https://example.org/facility/star-1"
driver = "hamilton.star"
profile = "adapters/star-1.toml"

[[execution.adapters]]
asset = "https://example.org/facility/cycler-1"
driver = "inheco.odtc"
profile = "adapters/cycler-1.toml"
"#,
        )
        .unwrap();

        assert_eq!(manifest.execution.adapters.len(), 2);
        assert_eq!(
            manifest.execution.adapters[0].asset,
            "https://example.org/facility/star-1"
        );
        assert_eq!(manifest.execution.adapters[0].driver, "hamilton.star");
        assert_eq!(
            manifest.execution.adapters[0].profile,
            PathBuf::from("adapters/star-1.toml")
        );
        manifest.validate().unwrap();
    }

    #[test]
    fn rejects_non_portable_or_duplicate_adapter_bindings() {
        let without_inventory = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[[execution.adapters]]
asset = "https://example.org/facility/star-1"
driver = "hamilton.star"
profile = "adapters/star-1.toml"
"#,
        )
        .unwrap();
        assert!(matches!(
            without_inventory.validate(),
            Err(PackageError::InvalidExecution(_))
        ));

        let escaping_profile = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[inventory]
document = "inventory/facility.ttl"

[[execution.adapters]]
asset = "https://example.org/facility/star-1"
driver = "hamilton.star"
profile = "../private/star-1.toml"
"#,
        )
        .unwrap();
        assert!(matches!(
            escaping_profile.validate(),
            Err(PackageError::InvalidExecution(_))
        ));

        let duplicate = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[inventory]
document = "inventory/facility.ttl"

[[execution.adapters]]
asset = "https://example.org/facility/star-1"
driver = "hamilton.star"
profile = "adapters/star-a.toml"

[[execution.adapters]]
asset = "https://example.org/facility/star-1"
driver = "hamilton.star"
profile = "adapters/star-b.toml"
"#,
        )
        .unwrap();
        assert!(matches!(
            duplicate.validate(),
            Err(PackageError::InvalidExecution(_))
        ));

        let invalid_driver = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[inventory]
document = "inventory/facility.ttl"

[[execution.adapters]]
asset = "https://example.org/facility/star-1"
driver = "Hamilton STAR"
profile = "adapters/star.toml"
"#,
        )
        .unwrap();
        assert!(matches!(
            invalid_driver.validate(),
            Err(PackageError::InvalidExecution(_))
        ));
    }

    #[test]
    fn rejects_ambiguous_or_non_portable_inventory_configuration() {
        let removed_symbols = PackageManifest::parse(
            r#"[package]
name = "test"
version = "0.1.0"

[inventory]
document = "inventory/catalog.ttl"
materials = ["BsaI"]
"#,
        );
        assert!(removed_symbols.is_err());

        let escaping = PackageManifest::parse(
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[inventory]\ndocument = \"../catalog.ttl\"\n",
        )
        .unwrap();
        assert!(matches!(
            escaping.validate(),
            Err(PackageError::InvalidInventory(_))
        ));

        let selector_only = PackageManifest::parse(
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[inventory]\nfacility = \"https://example.org/facility\"\n",
        )
        .unwrap();
        assert!(matches!(
            selector_only.validate(),
            Err(PackageError::InvalidInventory(_))
        ));
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

//! Exact record of the adapter lowerings derived from one facility allocation.

use std::path::PathBuf;

use lab_runfmt::{
    ExecutionAdapterBinding, ExecutionLoweringBundle, ReviewedLoweringArtifact,
    ReviewedLoweringArtifactRole,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FACILITY_LOWERING_SCHEMA_VERSION: &str = "lab.facility-lowering.v2";

/// Device artifacts emitted only after capability requirements have been allocated to a facility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweringManifest {
    pub schema_version: String,
    pub inventory_sha256: String,
    pub facility: String,
    pub routes: Vec<FacilityLoweringRoute>,
}

/// One exact Asset and adapter implementation selected by allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweringRoute {
    pub id: String,
    pub asset: String,
    pub driver: String,
    pub profile_path: PathBuf,
    pub profile_sha256: String,
    pub scope: FacilityLoweringScope,
    pub requirements: Vec<FacilityLoweredRequirement>,
    pub output: PathBuf,
    pub artifacts: Vec<FacilityLoweredArtifact>,
}

/// Whether route artifacts cover an entire compatibility backend or exact invocation requirements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacilityLoweringScope {
    WholeProgram,
    Invocation,
}

/// A semantic requirement whose allocated route caused this adapter lowering to exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweredRequirement {
    pub requirement_instance: String,
    pub capability_kind: String,
    pub offering: String,
}

/// One immutable artifact emitted by an allocated adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityLoweredArtifact {
    pub path: PathBuf,
    pub media_type: String,
    pub sha256: String,
    pub role: FacilityLoweredArtifactRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacilityLoweredArtifactRole {
    AutomationProtocol,
    OperatorDocument,
    Support,
}

/// Projects compiler lowering records into the generic reviewed-plan contract.
pub fn reviewed_lowering_bundles(
    manifest: &FacilityLoweringManifest,
) -> Result<Vec<ExecutionLoweringBundle>, FacilityLoweringProjectionError> {
    if manifest.schema_version != FACILITY_LOWERING_SCHEMA_VERSION {
        return Err(FacilityLoweringProjectionError::WrongSchema {
            found: manifest.schema_version.clone(),
        });
    }
    manifest
        .routes
        .iter()
        .filter(|route| route.scope == FacilityLoweringScope::WholeProgram)
        .map(|route| {
            let profile_path = utf8_path(&route.profile_path, &route.id, "adapter profile")?;
            let artifacts = route
                .artifacts
                .iter()
                .map(|artifact| {
                    let path = route.output.join(&artifact.path);
                    Ok(ReviewedLoweringArtifact {
                        path: utf8_path(&path, &route.id, "artifact")?,
                        media_type: artifact.media_type.clone(),
                        sha256: artifact.sha256.clone(),
                        role: match artifact.role {
                            FacilityLoweredArtifactRole::AutomationProtocol => {
                                ReviewedLoweringArtifactRole::DeviceProtocol
                            }
                            FacilityLoweredArtifactRole::OperatorDocument => {
                                ReviewedLoweringArtifactRole::OperatorDocument
                            }
                            FacilityLoweredArtifactRole::Support => {
                                ReviewedLoweringArtifactRole::Support
                            }
                        },
                        format: artifact.format.clone(),
                    })
                })
                .collect::<Result<Vec<_>, FacilityLoweringProjectionError>>()?;
            Ok(ExecutionLoweringBundle {
                id: route.id.clone(),
                asset: route.asset.clone(),
                adapter: ExecutionAdapterBinding {
                    driver: route.driver.clone(),
                    profile_path,
                    profile_sha256: route.profile_sha256.clone(),
                },
                requirements: route
                    .requirements
                    .iter()
                    .map(|requirement| requirement.requirement_instance.clone())
                    .collect(),
                artifacts,
            })
        })
        .collect()
}

fn utf8_path(
    path: &std::path::Path,
    route: &str,
    kind: &'static str,
) -> Result<String, FacilityLoweringProjectionError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| FacilityLoweringProjectionError::NonUtf8Path {
            route: route.to_owned(),
            kind,
        })
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FacilityLoweringProjectionError {
    #[error(
        "facility lowering declares schema `{found}`, expected `{FACILITY_LOWERING_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("facility lowering route `{route}` has a non-UTF-8 {kind} path")]
    NonUtf8Path { route: String, kind: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_facility_lowering_becomes_one_exact_reviewed_child_bundle() {
        let mut manifest = FacilityLoweringManifest {
            schema_version: FACILITY_LOWERING_SCHEMA_VERSION.to_owned(),
            inventory_sha256: "a".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            routes: vec![FacilityLoweringRoute {
                id: "opentrons-ot2-a1b2c3d4e5f6".to_owned(),
                asset: "https://example.org/ot2".to_owned(),
                driver: "opentrons.ot2".to_owned(),
                profile_path: PathBuf::from("adapters/opentrons.ot2-profile.toml"),
                profile_sha256: "b".repeat(64),
                scope: FacilityLoweringScope::WholeProgram,
                requirements: vec![FacilityLoweredRequirement {
                    requirement_instance: "example::main/body[0]".to_owned(),
                    capability_kind: "https://example.org/LiquidHandling".to_owned(),
                    offering: "https://example.org/ot2/liquid-handling".to_owned(),
                }],
                output: PathBuf::from("assets/ot2"),
                artifacts: vec![FacilityLoweredArtifact {
                    path: PathBuf::from("wave-001/protocol.py"),
                    media_type: "text/x-python".to_owned(),
                    sha256: "c".repeat(64),
                    role: FacilityLoweredArtifactRole::AutomationProtocol,
                    format: Some("opentrons.python-protocol".to_owned()),
                }],
            }],
        };
        let mut invocation_route = manifest.routes[0].clone();
        invocation_route.id = "simulator-c6d7e8f90123".to_owned();
        invocation_route.driver = "lab.simulator".to_owned();
        invocation_route.scope = FacilityLoweringScope::Invocation;
        manifest.routes.push(invocation_route);

        let reviewed = reviewed_lowering_bundles(&manifest).unwrap();
        assert_eq!(reviewed.len(), 1);
        assert_eq!(reviewed[0].id, manifest.routes[0].id);
        assert_eq!(reviewed[0].requirements, ["example::main/body[0]"]);
        assert_eq!(
            reviewed[0].artifacts[0].path,
            "assets/ot2/wave-001/protocol.py"
        );
        assert_eq!(
            reviewed[0].artifacts[0].role,
            ReviewedLoweringArtifactRole::DeviceProtocol
        );
    }
}

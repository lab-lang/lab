//! STAR implementation of the compiler backend contracts.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::backend::{Backend, BackendDescriptor, BackendEmitter, BackendTarget};
use crate::{ArtifactBundle, ProtocolLairProgram};

use crate::backend::hamilton::star::emit::emit_program;
use crate::backend::hamilton::star::plan::{
    StarEmissionError, StarExecutionPlan, StarPlanningError, plan_build,
};
use crate::backend::hamilton::star::profile::StarTargetProfile;

/// The STAR backend bound to one bench. Planning reads every carrier,
/// labware, and tip decision from the profile it carries.
#[derive(Clone, Debug, Default)]
pub struct StarBackend {
    profile: StarTargetProfile,
}

impl StarBackend {
    pub fn new(profile: StarTargetProfile) -> Self {
        Self { profile }
    }

    pub fn profile(&self) -> &StarTargetProfile {
        &self.profile
    }
}

#[derive(Debug, Error)]
pub enum StarCompileError {
    #[error(transparent)]
    Planning(#[from] StarPlanningError),
}

impl Backend<ProtocolLairProgram> for StarBackend {
    type Program = StarExecutionPlan;
    type Error = StarCompileError;

    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            id: "hamilton".into(),
            display_name: "Hamilton firmware protocol".into(),
            manufacturer: Some("Hamilton".into()),
            targets: vec![BackendTarget {
                id: "star".into(),
                display_name: "Hamilton STAR/STARlet".into(),
                capabilities: BTreeSet::from([
                    "liquid_transfer".into(),
                    "eight_channel".into(),
                    "firmware_protocol".into(),
                    "live_run".into(),
                ]),
            }],
        }
    }

    fn compile(&self, protocol: &ProtocolLairProgram) -> Result<Self::Program, Self::Error> {
        Ok(plan_build(protocol, &self.profile)?)
    }
}

impl BackendEmitter<StarExecutionPlan> for StarBackend {
    type Error = StarEmissionError;

    fn emit(&self, program: &StarExecutionPlan) -> Result<ArtifactBundle, Self::Error> {
        Ok(emit_program(program)?.artifacts().clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::{Backend, BackendEmitter};
    use crate::test_support::golden_gate_protocol;

    use crate::backend::hamilton::star::backend::*;

    #[test]
    fn compiles_the_example_and_emits_run_documents() {
        let protocol = golden_gate_protocol();
        let backend = StarBackend::default();
        let program = backend.compile(&protocol).unwrap();
        assert_eq!(backend.descriptor().id, "hamilton");
        assert_eq!(backend.descriptor().targets[0].id, "star");
        assert_eq!(program.assemblies.len(), 2);
        // One plasmid feeds two chassis, so four strains come from two
        // assemblies rather than one strain per assembly.
        assert_eq!(program.strains.len(), 4);

        let artifacts = backend.emit(&program).unwrap();
        for path in [
            "assembly_run.star.json",
            "transformation_mix_run.star.json",
            "transformation_recovery_run.star.json",
            "plating_run.star.json",
            "automation_manifest.json",
            "manual_protocol.md",
        ] {
            assert!(artifacts.get(path).is_some(), "missing {path}");
        }

        let assembly: serde_json::Value = serde_json::from_str(
            artifacts
                .get("assembly_run.star.json")
                .unwrap()
                .text_contents()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(assembly["format"], "lab.star-run.v0");
        assert_eq!(assembly["machine"], "starlet");
        let steps = assembly["steps"]
            .as_array()
            .expect("a run document carries its steps");
        assert!(
            steps.len() > 4,
            "the assembly run has tip definitions, retracts, and liquid steps"
        );
        for step in steps {
            let frame = step["frame"].as_str().expect("every step carries a frame");
            assert!(
                hamilton_star::RawCommand::parse(frame).is_ok(),
                "every emitted frame replays through the driver crate: {frame}"
            );
        }
        // The frame sequence is pinned at its load-bearing points: the tip
        // definition (50 µL filter tip: 42.4 mm above the 8 mm fitting
        // depth, 60 µL ceiling), the first pickup (rack site 1 A1, the
        // catalog-derived 224.9 mm pickup window), and the first dispense's
        // liquid-class correction (2.0 µL of water commands 2.8 µL, the
        // vendored curve's calibration point).
        assert_eq!(
            steps[0]["frame"], "C0TTtt00tf1tl0424tv00600tg2tu0",
            "the small tip defines as firmware type 0"
        );
        assert_eq!(
            steps[1]["frame"], "C0ZA",
            "runs open with a Z-safety retract"
        );
        assert_eq!(
            steps[2]["frame"], "C0TPxp01179 00000&yp1458 0000&tm1 0&tt00tp2249tz1825th2450td0",
            "the first pickup draws rack A1 through the catalog geometry"
        );
        let first_dispense = steps
            .iter()
            .find(|step| step["code"] == "DS")
            .expect("the assembly run dispenses");
        assert!(
            first_dispense["frame"]
                .as_str()
                .expect("frames are strings")
                .contains("dv00028"),
            "2.0 µL of water corrects to 2.8 µL through the vendored curve"
        );
    }
}

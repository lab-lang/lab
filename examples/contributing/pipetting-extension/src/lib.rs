//! A complete contribution using only Lab's public Procedure-authoring interface.

use lab_capability::ProcedureContractId;
use lab_compiler::procedure::pipetting::{MixSettings, PipettingBuilder, TransferSettings};
use lab_compiler::procedure::{
    FluidPathPolicy, ProcedureProgram, ProcedureProgramBuildContext, ProcedureProgramBuilderId,
    ProcedureProgramBuilderRegistration,
};

pub const BUILDER: &str = "https://example.org/procedure-builder#HomogenizeV1";

pub fn registration() -> ProcedureProgramBuilderRegistration {
    ProcedureProgramBuilderRegistration::new(
        ProcedureProgramBuilderId::new(BUILDER).expect("absolute builder IRI"),
        ProcedureContractId::new(lab_compiler::procedure::vocabulary::PIPETTING_PROGRAM_V1)
            .expect("absolute contract IRI"),
        homogenize,
    )
}

fn homogenize(task: &ProcedureProgramBuildContext<'_>) -> Result<ProcedureProgram, String> {
    task.require_io(1, 1)?;
    task.require_material_roles(&[])?;
    let volume = task.volume_parameter("sample_volume")?;
    let mut program = PipettingBuilder::new();
    let source = program.input("sample", 0, 1, volume.clone())?;
    let product = program.product("prepared", task.output(0)?, 1)?;
    {
        let mut path =
            program.continuous_path("sample-path", FluidPathPolicy::IsolatedDestinations)?;
        path.transfer(
            "move-sample",
            source.position(0)?,
            product.position(0)?,
            &TransferSettings {
                volume,
                technique: Default::default(),
            },
        )?;
        path.mix(
            "mix-sample",
            product.position(0)?,
            &MixSettings {
                cycles: task.integer_parameter("mix_cycles", None)?,
                volume: task.volume_parameter("mix_volume")?,
                technique: Default::default(),
            },
        )?;
    }
    Ok(ProcedureProgram::from_pipetting(&program.finish()?))
}

use lab_compiler::procedure::pipetting::{
    Location, MixSettings, PipettingBuilder, PipettingStep, SerialDilutionSettings,
    TransferSettings, serial_dilution,
};
use lab_compiler::procedure::{ProcedureLocalId, Volume};

fn id(name: &str) -> ProcedureLocalId {
    ProcedureLocalId::new(name).unwrap()
}
fn volume(value: &str) -> Volume {
    Volume::parse_microlitres(value).unwrap()
}
fn at(vessel: &str, position: u32) -> Location {
    Location {
        vessel: id(vessel),
        position,
    }
}
fn settings(start: &str) -> SerialDilutionSettings {
    SerialDilutionSettings {
        replicates: 2,
        stages: 3,
        input_volume_each: volume(start),
        diluent_volume: volume("90"),
        diluent_load: Some(volume("550")),
        retained_diluent: Some(volume("10")),
        transfer: TransferSettings {
            volume: volume("10"),
            technique: Default::default(),
        },
        mixing: MixSettings {
            cycles: 3,
            volume: volume("50"),
            technique: Default::default(),
        },
    }
}

#[test]
fn dilution_reuses_an_algorithm_with_explicit_inputs_and_exact_liquid_accounting() {
    for (material, start, remaining) in [("buffer", "35", "25"), ("medium", "100.5", "90.5")] {
        let program = serial_dilution(0, id(material), id("dilutions"), settings(start)).unwrap();
        let ledger = program.liquid_ledger();
        assert_eq!(
            ledger
                .final_volume(&at("culture-input", 0))
                .unwrap()
                .to_string(),
            remaining
        );
        assert_eq!(
            ledger
                .final_volume(&at("medium-source", 0))
                .unwrap()
                .to_string(),
            "10"
        );
        for position in 0..6 {
            assert_eq!(
                ledger
                    .final_volume(&at("dilution-plate", position))
                    .unwrap()
                    .to_string(),
                if position < 4 { "90" } else { "100" }
            );
        }
        assert_eq!(program.as_program().materials[0].id, id(material));
        assert_eq!(program.as_program().steps.len(), 13);
        // Stage-major wells, replicate-major execution, and one tip path per replicate.
        for replicate in 0..2 {
            for stage in 0..3 {
                let index = 1 + (replicate * 3 + stage) as usize * 2;
                let PipettingStep::Transfer {
                    source,
                    destination,
                    fluid_path_group,
                    ..
                } = &program.as_program().steps[index]
                else {
                    panic!("transfer must precede mix")
                };
                assert_eq!(destination, &at("dilution-plate", stage * 2 + replicate));
                assert_eq!(
                    source,
                    &if stage == 0 {
                        at("culture-input", replicate)
                    } else {
                        at("dilution-plate", (stage - 1) * 2 + replicate)
                    }
                );
                assert_eq!(
                    fluid_path_group.as_ref().unwrap(),
                    &id(&format!("series-{replicate:04}"))
                );
                assert!(matches!(
                    program.as_program().steps[index + 1],
                    PipettingStep::Mix { .. }
                ));
            }
        }
    }
}

#[test]
fn authoring_rejects_bad_extent_duplicate_vessels_and_insufficient_liquid() {
    let mut builder = PipettingBuilder::new();
    let input = builder.input("sample", 0, 2, volume("10")).unwrap();
    assert!(input.position(2).is_err());
    assert!(builder.input("sample", 0, 2, volume("10")).is_err());
    assert!(builder.product("empty", id("product"), 0).is_err());
    assert!(serial_dilution(0, id("buffer"), id("product"), settings("9")).is_err());
    let mut insufficient_diluent = settings("35");
    insufficient_diluent.diluent_load = Some(volume("549"));
    assert!(serial_dilution(0, id("buffer"), id("product"), insufficient_diluent).is_err());
}

//! Resolve simultaneous vessel constraints onto the two supported temperature modules.
use lab_compiler::procedure::{PipettingProgramV1, ProcedureLocalId};
use std::collections::BTreeMap;

pub(super) fn temperatures(
    program: &PipettingProgramV1,
    resource: impl Fn(&ProcedureLocalId) -> &'static str,
) -> Result<BTreeMap<String, f64>, String> {
    let mut ranges = BTreeMap::<String, (f64, f64)>::new();
    for vessel in &program.vessels {
        if let Some(range) = &vessel.temperature {
            let name = resource(&vessel.id);
            if !matches!(name, "sources" | "work") {
                return Err(format!(
                    "vessel `{}` requires controlled temperature on passive {name} labware",
                    vessel.id
                ));
            }
            let minimum = range
                .minimum
                .value()
                .to_string()
                .parse::<f64>()
                .map_err(|e| e.to_string())?;
            let maximum = range
                .maximum
                .value()
                .to_string()
                .parse::<f64>()
                .map_err(|e| e.to_string())?;
            let limits = ranges
                .entry(name.into())
                .or_insert((4.0, if name == "sources" { 95.0 } else { 99.0 }));
            limits.0 = limits.0.max(minimum);
            limits.1 = limits.1.min(maximum);
            if limits.0 > limits.1 {
                return Err(format!(
                    "vessel `{}` temperature cannot be maintained by the shared {name} module",
                    vessel.id
                ));
            }
        }
    }
    Ok(ranges
        .into_iter()
        .map(|(name, (minimum, _))| (name, minimum))
        .collect())
}

/// Apply the adapter's reviewed capacities without changing the emitted semantic program.
pub(super) fn working_volumes(
    program: &PipettingProgramV1,
    maximum: impl Fn(&ProcedureLocalId) -> u32,
) -> Result<(), String> {
    use lab_compiler::procedure::{Location, Volume};
    let validated = program.clone().validate().map_err(|e| e.to_string())?;
    let mut physical = program.clone();
    for vessel in &mut physical.vessels {
        let capacity = Volume::parse_microlitres(maximum(&vessel.id).to_string())
            .map_err(|e| e.to_string())?;
        if maximum(&vessel.id) == 0 {
            return Err(format!(
                "vessel `{}` has a zero physical working volume",
                vessel.id
            ));
        }
        for position in 0..vessel.positions {
            let location = Location {
                vessel: vessel.id.clone(),
                position,
            };
            let initial = vessel
                .initial_volume_each
                .as_ref()
                .map(|v| v.value())
                .or_else(|| validated.liquid_ledger().required_initial_volume(&location));
            if initial.is_some_and(|v| v > capacity.value()) {
                return Err(format!(
                    "vessel `{}[{position}]` needs more starting liquid than its {} uL physical working volume",
                    vessel.id,
                    capacity.value()
                ));
            }
        }
        if vessel
            .working_capacity_each
            .as_ref()
            .is_none_or(|v| v.value() > capacity.value())
        {
            vessel.working_capacity_each = Some(capacity);
        }
    }
    physical
        .validate()
        .map(|_| ())
        .map_err(|e| format!("physical working volume: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_compiler::procedure::{
        PipettingConstraints, Temperature, TemperatureRange, Vessel, VesselRole,
    };

    fn vessel(name: &str, minimum: &str, maximum: &str) -> Vessel {
        Vessel {
            id: ProcedureLocalId::new(name).unwrap(),
            role: VesselRole::ProcedureInput { input: 0 },
            positions: 1,
            initial_volume_each: None,
            working_capacity_each: None,
            dead_volume_each: None,
            temperature: Some(
                TemperatureRange::new(
                    Temperature::parse_degrees_celsius(minimum).unwrap(),
                    Temperature::parse_degrees_celsius(maximum).unwrap(),
                )
                .unwrap(),
            ),
        }
    }

    #[test]
    fn physical_capacity_checks_starting_loads_and_intermediate_fills() {
        use lab_compiler::procedure::pipetting::{PipettingBuilder, TransferSettings};
        use lab_compiler::procedure::{FluidPathPolicy, TransferTechnique, Volume};
        let volume = |v| Volume::parse_microlitres(v).unwrap();
        let mut builder = PipettingBuilder::new();
        let input = builder.input("sample", 0, 1, volume("200")).unwrap();
        let product = builder
            .product("product", ProcedureLocalId::new("out").unwrap(), 1)
            .unwrap();
        builder
            .distribute(
                "fill",
                input.position(0).unwrap(),
                product.positions(),
                &TransferSettings {
                    volume: volume("150"),
                    technique: TransferTechnique::default(),
                },
                FluidPathPolicy::IsolatedDestinations,
            )
            .unwrap();
        let program = builder.finish().unwrap();
        assert!(
            working_volumes(program.as_program(), |_| 100)
                .unwrap_err()
                .contains("starting liquid")
        );
        assert!(
            working_volumes(program.as_program(), |id| if id.as_str() == "sample" {
                200
            } else {
                100
            })
            .unwrap_err()
            .contains("working volume")
        );
        assert!(working_volumes(program.as_program(), |_| 200).is_ok());
        assert!(
            program
                .as_program()
                .vessels
                .iter()
                .all(|v| v.working_capacity_each.is_none()),
            "device limits do not alter the canonical program"
        );
    }

    #[test]
    fn shared_modules_require_an_intersection_and_device_supported_temperature() {
        let mut program = PipettingProgramV1::new(
            vec![],
            vec![],
            vec![vessel("cells", "4", "8"), vessel("reagent", "6", "10")],
            vec![],
            PipettingConstraints::default(),
        );
        assert_eq!(
            temperatures(&program, |_| "sources").unwrap()["sources"],
            6.0
        );
        program.vessels[1] = vessel("reagent", "20", "25");
        assert!(temperatures(&program, |_| "sources").is_err());
        assert!(
            temperatures(&program, |id| if id.as_str() == "cells" {
                "work"
            } else {
                "sources"
            })
            .is_ok()
        );
        program.vessels = vec![vessel("too-cold", "0", "3")];
        assert!(temperatures(&program, |_| "work").is_err());
    }
}

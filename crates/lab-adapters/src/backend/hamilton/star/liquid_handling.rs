//! Explicit liquid classification for calibrated STAR planning.

use std::collections::BTreeMap;

use lab_capability::ExactDecimal;
use lab_compiler::allocation::AllocatedProcedureTask;
use lab_compiler::procedure::{
    Location, PipettingProgramV1, PipettingStep, ProcedureLocalId, VesselRole, Volume,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::plan::StarWell;

/// Classify externally supplied materials and Procedure inputs. Categories are the
/// open vocabulary of the selected liquid-class library, never biological operation names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StarLiquidHandling {
    /// Explicit fallback for unlisted inputs. Omit in an authored table to require
    /// every loaded liquid to be classified. The bundled reference profile uses aqueous.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_liquid: Option<String>,
    /// Exact allocated material symbol -> calibrated liquid category.
    #[serde(default)]
    pub materials: BTreeMap<String, String>,
    /// Logical Procedure-input vessel ID -> category of the prepared input contents.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
}

impl Default for StarLiquidHandling {
    fn default() -> Self {
        Self {
            default_liquid: Some("aqueous".to_owned()),
            materials: BTreeMap::new(),
            inputs: BTreeMap::new(),
        }
    }
}

#[derive(Clone)]
struct Contents {
    category: Option<String>,
    volume: Option<ExactDecimal>,
}

type WellKey = (String, String);

impl StarLiquidHandling {
    pub fn validate(&self) -> Result<(), String> {
        for value in self
            .default_liquid
            .iter()
            .chain(self.materials.keys())
            .chain(self.materials.values())
            .chain(self.inputs.keys())
            .chain(self.inputs.values())
        {
            if value.trim().is_empty() || value.trim() != value {
                return Err(
                    "liquid classifications require non-empty names without surrounding whitespace"
                        .to_owned(),
                );
            }
        }
        Ok(())
    }

    /// Replay classifications in exact operation order. Transfers into an empty well
    /// preserve the source category. Combining distinct categories becomes unclassified;
    /// subsequent aspiration or mixing is refused rather than guessing a mixture class.
    pub(super) fn resolve(
        &self,
        task: &AllocatedProcedureTask,
        program: &PipettingProgramV1,
        locations: &BTreeMap<ProcedureLocalId, Vec<StarWell>>,
    ) -> Result<BTreeMap<ProcedureLocalId, Vec<String>>, String> {
        let zero = ExactDecimal::parse("0").expect("zero");
        let key = |at: &Location| -> Result<WellKey, String> {
            let well = locations
                .get(&at.vessel)
                .and_then(|wells| wells.get(at.position as usize))
                .ok_or_else(|| {
                    format!(
                        "liquid classification has no allocation for {}[{}]",
                        at.vessel, at.position
                    )
                })?;
            Ok((well.resource.clone(), well.well.clone()))
        };
        let mut contents = BTreeMap::<WellKey, Contents>::new();
        for vessel in &program.vessels {
            let loaded = matches!(
                vessel.role,
                VesselRole::ProcedureInput { .. }
                    | VesselRole::InputOutput { .. }
                    | VesselRole::MaterialSource { .. }
                    | VesselRole::MaterialProduct { .. }
            ) || vessel.initial_volume_each.is_some();
            let category = match &vessel.role {
                VesselRole::MaterialSource { material }
                | VesselRole::MaterialProduct { material, .. } => {
                    let binding = task
                        .materials
                        .iter()
                        .find(|binding| binding.input.as_str() == material.as_str())
                        .ok_or_else(|| {
                            format!("material `{material}` has no allocated liquid binding")
                        })?;
                    self.materials
                        .get(&binding.symbol)
                        .or(self.default_liquid.as_ref())
                        .cloned()
                }
                _ if loaded => self
                    .inputs
                    .get(vessel.id.as_str())
                    .or(self.default_liquid.as_ref())
                    .cloned(),
                _ => None,
            };
            let volume = vessel
                .initial_volume_each
                .as_ref()
                .map(|v| v.value().clone())
                .or_else(|| (!loaded).then(|| zero.clone()));
            for position in 0..vessel.positions {
                contents.insert(
                    key(&Location {
                        vessel: vessel.id.clone(),
                        position,
                    })?,
                    Contents {
                        category: category.clone(),
                        volume: volume.clone(),
                    },
                );
            }
        }
        let mut resolved = BTreeMap::new();
        for step in &program.steps {
            let categories = match step {
                PipettingStep::Transfer {
                    source,
                    destination,
                    volume,
                    ..
                } => {
                    vec![move_liquid(
                        &mut contents,
                        key(source)?,
                        key(destination)?,
                        volume,
                        step.id(),
                    )?]
                }
                PipettingStep::Distribute {
                    source,
                    destinations,
                    volume_each,
                    ..
                } => {
                    let mut category = None;
                    for destination in destinations {
                        category = Some(move_liquid(
                            &mut contents,
                            key(source)?,
                            key(destination)?,
                            volume_each,
                            step.id(),
                        )?);
                    }
                    category.into_iter().collect()
                }
                PipettingStep::Mix { targets, .. } => targets
                    .iter()
                    .map(|at| category(&contents, &key(at)?, step.id()))
                    .collect::<Result<Vec<_>, _>>()?,
                PipettingStep::Barrier { .. } => Vec::new(),
            };
            resolved.insert(step.id().clone(), categories);
        }
        Ok(resolved)
    }
}

fn category(
    contents: &BTreeMap<WellKey, Contents>,
    key: &WellKey,
    step: &ProcedureLocalId,
) -> Result<String, String> {
    contents.get(key).and_then(|contents| contents.category.clone()).ok_or_else(|| format!("STAR step `{step}` cannot aspirate or mix unclassified contents at {}/{}; classify the loaded input or supply a separately classified prepared mixture", key.0, key.1))
}

fn move_liquid(
    contents: &mut BTreeMap<WellKey, Contents>,
    source: WellKey,
    destination: WellKey,
    volume: &Volume,
    step: &ProcedureLocalId,
) -> Result<String, String> {
    let liquid = category(contents, &source, step)?;
    let from = contents
        .get_mut(&source)
        .expect("classified source is present");
    if let Some(available) = &mut from.volume {
        *available = available.subtracted_by(volume.value());
        if available.is_zero() {
            from.category = None;
        }
    }
    let to = contents
        .get_mut(&destination)
        .ok_or_else(|| format!("STAR step `{step}` has no destination contents"))?;
    if to.volume.as_ref().is_some_and(ExactDecimal::is_zero) {
        to.category = Some(liquid.clone());
    } else if to.category.as_deref() != Some(&liquid) {
        to.category = None;
    }
    if let Some(available) = &mut to.volume {
        *available = available.added_to(volume.value());
    }
    Ok(liquid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(well: &str) -> WellKey {
        ("plate".into(), well.into())
    }
    fn contents(category: Option<&str>, volume: &str) -> Contents {
        Contents {
            category: category.map(str::to_owned),
            volume: Some(ExactDecimal::parse(volume).unwrap()),
        }
    }

    #[test]
    fn classification_follows_emptying_refilling_and_mixtures_without_guessing() {
        let mut wells = BTreeMap::from([
            (at("A1"), contents(Some("aqueous"), "20")),
            (at("A2"), contents(Some("glycerol_50_percent"), "20")),
            (at("A3"), contents(None, "0")),
        ]);
        let step = ProcedureLocalId::new("transfer").unwrap();
        let volume = Volume::parse_microlitres("10").unwrap();
        assert_eq!(
            move_liquid(&mut wells, at("A1"), at("A3"), &volume, &step).unwrap(),
            "aqueous"
        );
        assert_eq!(category(&wells, &at("A3"), &step).unwrap(), "aqueous");
        move_liquid(&mut wells, at("A3"), at("A1"), &volume, &step).unwrap();
        assert!(category(&wells, &at("A3"), &step).is_err());
        move_liquid(&mut wells, at("A2"), at("A3"), &volume, &step).unwrap();
        assert_eq!(
            category(&wells, &at("A3"), &step).unwrap(),
            "glycerol_50_percent"
        );
        move_liquid(&mut wells, at("A1"), at("A3"), &volume, &step).unwrap();
        assert!(
            category(&wells, &at("A3"), &step)
                .unwrap_err()
                .contains("unclassified")
        );
        assert!(move_liquid(&mut wells, at("A3"), at("A2"), &volume, &step).is_err());
    }

    #[test]
    fn authored_strict_classification_has_no_implicit_fallback() {
        let strict: StarLiquidHandling =
            toml::from_str("[materials]\nglycerol = 'glycerol_50_percent'\n").unwrap();
        assert_eq!(strict.default_liquid, None);
        assert_eq!(
            StarLiquidHandling::default().default_liquid.as_deref(),
            Some("aqueous")
        );
        assert!(toml::from_str::<StarLiquidHandling>("default_liqiud = 'aqueous'").is_err());
    }
}

//! The vendor MethodSet XML dialect: rendering thermal profiles and
//! constant holds as uploadable method documents, with the validation
//! the device itself does not do.
//!
//! Timestamps and method names are caller inputs — nothing here reads a
//! clock — so a given profile always renders to the same bytes.

use thiserror::Error;

/// The ambient temperature below which a plateau condenses moisture on
/// the block.
pub const AMBIENT_CELSIUS: f64 = 20.0;

/// The longest total sub-ambient hold the condensation limit allows.
pub const MAX_SUB_AMBIENT_HOLD_SECONDS: f64 = 7200.0;

/// The device's maximum ramp slope; unset slopes render as this value.
pub const MAX_SLOPE_C_PER_S: f64 = 4.4;

/// The block temperature range the device accepts.
pub const BLOCK_MIN_CELSIUS: f64 = 4.0;
pub const BLOCK_MAX_CELSIUS: f64 = 99.0;

/// The lid temperature range the device accepts.
pub const LID_MIN_CELSIUS: f64 = 30.0;
pub const LID_MAX_CELSIUS: f64 = 115.0;

/// A thermal program: ordered stages, each a cycled group of plateau
/// steps, rendered as one `Method` whose loops encode the repeats.
#[derive(Clone, Debug, PartialEq)]
pub struct ThermalProgram {
    pub stages: Vec<ProgramStage>,
}

/// A group of steps executed in order and repeated as a block; the last
/// step of a repeated stage carries the `GotoNumber`/`LoopNumber` pair.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramStage {
    pub steps: Vec<ProgramStep>,
    /// Total executions of the block; 1 means run once.
    pub repeats: u32,
}

/// One plateau: ramp to a temperature, hold it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramStep {
    pub plateau_celsius: f64,
    pub hold_seconds: f64,
    /// Ramp slope toward this plateau; `None` renders the device maximum.
    pub slope_c_per_s: Option<f64>,
    /// Lid temperature during this step; `None` renders the method's
    /// start lid.
    pub lid_celsius: Option<f64>,
}

/// The error raised when a method document cannot be rendered.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum MethodSetError {
    #[error("a thermal program must contain at least one stage with at least one step")]
    EmptyProgram,
    #[error("stage {stage} repeats {repeats} times; a stage runs at least once")]
    ZeroRepeats { stage: usize, repeats: u32 },
    #[error(
        "step {step} of stage {stage} targets {celsius} °C, outside the block range {min}–{max} °C"
    )]
    StepBlockOutOfRange {
        stage: usize,
        step: usize,
        celsius: f64,
        min: f64,
        max: f64,
    },
    #[error(
        "step {step} of stage {stage} sets the lid to {celsius} °C, outside the lid range {min}–{max} °C"
    )]
    StepLidOutOfRange {
        stage: usize,
        step: usize,
        celsius: f64,
        min: f64,
        max: f64,
    },
    #[error(
        "step {step} of stage {stage} asks for {slope} °C/s, above the device maximum {max} °C/s"
    )]
    SlopeOutOfRange {
        stage: usize,
        step: usize,
        slope: f64,
        max: f64,
    },
    #[error(
        "step {step} of stage {stage} asks for a slope of {slope} °C/s; a slope must be positive"
    )]
    NonPositiveSlope {
        stage: usize,
        step: usize,
        slope: f64,
    },
    #[error("step {step} of stage {stage} holds for {seconds} s; a hold cannot be negative")]
    NegativeHold {
        stage: usize,
        step: usize,
        seconds: f64,
    },
    #[error(
        "method name {name:?} is empty or uses characters outside ASCII letters, digits, '_' and '-'; the device rejects other names"
    )]
    InvalidMethodName { name: String },
    #[error(
        "timestamp {timestamp:?} is not strict ISO-8601 with a timezone offset; the device expects e.g. 2026-08-09T12:00:00.000-08:00"
    )]
    InvalidTimestamp { timestamp: String },
    #[error(
        "the profile holds plateaus below the {ambient} °C ambient for {seconds} s in total; condensation limits sub-ambient holds to {max} s"
    )]
    SubAmbientHoldTooLong {
        ambient: f64,
        seconds: f64,
        max: f64,
    },
    #[error("{context} targets {celsius} °C, outside the block range {min}–{max} °C")]
    BlockOutOfRange {
        context: &'static str,
        celsius: f64,
        min: f64,
        max: f64,
    },
    #[error("{context} sets the lid to {celsius} °C, outside the lid range {min}–{max} °C")]
    LidOutOfRange {
        context: &'static str,
        celsius: f64,
        min: f64,
        max: f64,
    },
    #[error("fill volume {volume} µL is negative; the fluid-quantity class needs a real volume")]
    NegativeFillVolume { volume: f64 },
}

/// Method-level settings a thermal profile does not carry.
#[derive(Clone, Debug, PartialEq)]
pub struct MethodSettings {
    /// The per-well fill volume, classifying the vendor `FluidQuantity`:
    /// under 30 µL is class 0, under 75 µL class 1, otherwise class 2.
    pub fill_volume_ul: f64,
    /// Whether the device keeps the final temperature after the method
    /// ends.
    pub post_heating: bool,
    pub start_block_celsius: f64,
    /// The lid temperature at method start and for every step without a
    /// per-step lid of its own.
    pub start_lid_celsius: f64,
}

impl Default for MethodSettings {
    fn default() -> MethodSettings {
        MethodSettings {
            fill_volume_ul: 20.0,
            post_heating: true,
            start_block_celsius: 25.0,
            start_lid_celsius: 105.0,
        }
    }
}

/// The vendor `FluidQuantity` class for a per-well fill volume.
pub fn fluid_quantity_class(volume_ul: f64) -> u8 {
    if volume_ul < 30.0 {
        0
    } else if volume_ul < 75.0 {
        1
    } else {
        2
    }
}

/// Renders a full profile as a MethodSet document with one `Method`.
/// Validation covers the generic thermal envelope plus the MethodSet
/// rules: sub-ambient plateaus held longer than the condensation limit
/// are rejected, and unset ramps render as the device's maximum slope.
pub fn render_method(
    method_name: &str,
    creator: &str,
    timestamp: &str,
    program: &ThermalProgram,
    settings: &MethodSettings,
) -> Result<String, MethodSetError> {
    validate_method_name(method_name)?;
    validate_timestamp(timestamp)?;
    validate_program(program)?;
    validate_sub_ambient_hold(program)?;
    if settings.fill_volume_ul < 0.0 {
        return Err(MethodSetError::NegativeFillVolume {
            volume: settings.fill_volume_ul,
        });
    }
    check_block("the start block temperature", settings.start_block_celsius)?;
    check_lid("the start lid temperature", settings.start_lid_celsius)?;

    let mut steps = String::new();
    let mut step_number = 1u32;
    for stage in &program.stages {
        let stage_start = step_number;
        for (index, step) in stage.steps.iter().enumerate() {
            let last_in_stage = index + 1 == stage.steps.len();
            let (goto, loops) = if last_in_stage && stage.repeats > 1 {
                (stage_start, stage.repeats - 1)
            } else {
                (0, 0)
            };
            let slope = step.slope_c_per_s.unwrap_or(MAX_SLOPE_C_PER_S);
            let lid = step.lid_celsius.unwrap_or(settings.start_lid_celsius);
            steps.push_str(&format!(
                "<Step><Number>{number}</Number><Slope>{slope}</Slope>\
                 <PlateauTemperature>{plateau}</PlateauTemperature>\
                 <PlateauTime>{hold}</PlateauTime>\
                 <OverShootSlope1>0.1</OverShootSlope1>\
                 <OverShootTemperature>0</OverShootTemperature>\
                 <OverShootTime>0</OverShootTime>\
                 <OverShootSlope2>0.1</OverShootSlope2>\
                 <GotoNumber>{goto}</GotoNumber><LoopNumber>{loops}</LoopNumber>\
                 <PIDNumber>1</PIDNumber><LidTemp>{lid}</LidTemp></Step>",
                number = step_number,
                slope = number(slope),
                plateau = number(step.plateau_celsius),
                hold = number(step.hold_seconds),
                lid = number(lid),
            ));
            step_number += 1;
        }
    }

    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><MethodSet>\
         <DeleteAllMethods>false</DeleteAllMethods>\
         <Method methodName=\"{method_name}\" creator=\"{creator}\" dateTime=\"{timestamp}\">\
         <Variant>960000</Variant><PlateType>0</PlateType>\
         <FluidQuantity>{fluid}</FluidQuantity><PostHeating>{post}</PostHeating>\
         <StartBlockTemperature>{start_block}</StartBlockTemperature>\
         <StartLidTemperature>{start_lid}</StartLidTemperature>{steps}{pid_set}\
         </Method></MethodSet>",
        creator = quick_xml::escape::escape(creator),
        fluid = fluid_quantity_class(settings.fill_volume_ul),
        post = if settings.post_heating {
            "true"
        } else {
            "false"
        },
        start_block = number(settings.start_block_celsius),
        start_lid = number(settings.start_lid_celsius),
        pid_set = PID_SET,
    ))
}

/// Renders a constant hold as a MethodSet document with one `PreMethod`.
/// A PreMethod equilibrates block and lid together — several minutes on
/// this device — and then holds until a method runs or `StopMethod`
/// intervenes. With `dynamic_duration` the device finishes as soon as
/// both are equilibrated; without it the hold runs a fixed ten minutes.
pub fn render_pre_method(
    method_name: &str,
    creator: &str,
    timestamp: &str,
    block_celsius: f64,
    lid_celsius: f64,
    dynamic_duration: bool,
) -> Result<String, MethodSetError> {
    validate_method_name(method_name)?;
    validate_timestamp(timestamp)?;
    check_block("the constant hold", block_celsius)?;
    check_lid("the constant hold", lid_celsius)?;
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><MethodSet>\
         <DeleteAllMethods>false</DeleteAllMethods>\
         <PreMethod methodName=\"{method_name}\" creator=\"{creator}\" dateTime=\"{timestamp}\">\
         <TargetBlockTemperature>{block}</TargetBlockTemperature>\
         <TargetLidTemp>{lid}</TargetLidTemp>\
         <DynamicPreMethodDuration>{dynamic}</DynamicPreMethodDuration>\
         </PreMethod></MethodSet>",
        creator = quick_xml::escape::escape(creator),
        block = number(block_celsius),
        lid = number(lid_celsius),
        dynamic = if dynamic_duration { "true" } else { "false" },
    ))
}

/// Wraps a MethodSet document in the `ParameterSet` envelope
/// `SetParameters` uploads: the document is escaped once here and once
/// more when the SOAP envelope embeds the parameter text.
pub fn parameter_set(method_set_xml: &str) -> String {
    format!(
        "<ParameterSet><Parameter name=\"MethodsXML\"><String>{}</String></Parameter></ParameterSet>",
        quick_xml::escape::partial_escape(method_set_xml)
    )
}

const PID_SET: &str = "<PIDSet><PID number=\"1\"><PHeating>60</PHeating>\
                       <PCooling>80</PCooling><IHeating>250</IHeating>\
                       <ICooling>100</ICooling><DHeating>10</DHeating>\
                       <DCooling>10</DCooling><PLid>100</PLid><ILid>70</ILid>\
                       </PID></PIDSet>";

/// Renders a number the way the dialect expects: whole values without a
/// decimal point, fractional values as written.
fn number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e12 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn validate_method_name(name: &str) -> Result<(), MethodSetError> {
    let acceptable = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if acceptable {
        Ok(())
    } else {
        Err(MethodSetError::InvalidMethodName {
            name: name.to_string(),
        })
    }
}

fn validate_timestamp(timestamp: &str) -> Result<(), MethodSetError> {
    if timestamp_is_valid(timestamp) {
        Ok(())
    } else {
        Err(MethodSetError::InvalidTimestamp {
            timestamp: timestamp.to_string(),
        })
    }
}

/// Accepts `YYYY-MM-DDTHH:MM:SS`, optional fractional seconds, and a
/// mandatory timezone designator: `Z` or `±HH:MM`.
fn timestamp_is_valid(timestamp: &str) -> bool {
    let bytes = timestamp.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    let digits =
        |range: std::ops::Range<usize>| bytes[range].iter().all(|byte| byte.is_ascii_digit());
    let date_and_time = digits(0..4)
        && bytes[4] == b'-'
        && digits(5..7)
        && bytes[7] == b'-'
        && digits(8..10)
        && bytes[10] == b'T'
        && digits(11..13)
        && bytes[13] == b':'
        && digits(14..16)
        && bytes[16] == b':'
        && digits(17..19);
    if !date_and_time {
        return false;
    }
    let mut index = 19;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == fraction_start {
            return false;
        }
    }
    match bytes.get(index) {
        Some(b'Z') => index + 1 == bytes.len(),
        Some(b'+') | Some(b'-') => {
            bytes.len() == index + 6
                && digits(index + 1..index + 3)
                && bytes[index + 3] == b':'
                && digits(index + 4..index + 6)
        }
        _ => false,
    }
}

/// Validates a program against the device envelope: block and lid ranges
/// per plateau, positive slopes inside the maximum, non-negative holds,
/// and a non-empty, non-zero-repeat shape.
fn validate_program(program: &ThermalProgram) -> Result<(), MethodSetError> {
    if program.stages.is_empty() || program.stages.iter().any(|stage| stage.steps.is_empty()) {
        return Err(MethodSetError::EmptyProgram);
    }
    for (stage_index, stage) in program.stages.iter().enumerate() {
        if stage.repeats == 0 {
            return Err(MethodSetError::ZeroRepeats {
                stage: stage_index,
                repeats: stage.repeats,
            });
        }
        for (step_index, step) in stage.steps.iter().enumerate() {
            if !(BLOCK_MIN_CELSIUS..=BLOCK_MAX_CELSIUS).contains(&step.plateau_celsius) {
                return Err(MethodSetError::StepBlockOutOfRange {
                    stage: stage_index,
                    step: step_index,
                    celsius: step.plateau_celsius,
                    min: BLOCK_MIN_CELSIUS,
                    max: BLOCK_MAX_CELSIUS,
                });
            }
            if step.hold_seconds < 0.0 {
                return Err(MethodSetError::NegativeHold {
                    stage: stage_index,
                    step: step_index,
                    seconds: step.hold_seconds,
                });
            }
            if let Some(slope) = step.slope_c_per_s {
                if slope <= 0.0 {
                    return Err(MethodSetError::NonPositiveSlope {
                        stage: stage_index,
                        step: step_index,
                        slope,
                    });
                }
                if slope > MAX_SLOPE_C_PER_S {
                    return Err(MethodSetError::SlopeOutOfRange {
                        stage: stage_index,
                        step: step_index,
                        slope,
                        max: MAX_SLOPE_C_PER_S,
                    });
                }
            }
            if let Some(lid) = step.lid_celsius
                && !(LID_MIN_CELSIUS..=LID_MAX_CELSIUS).contains(&lid)
            {
                return Err(MethodSetError::StepLidOutOfRange {
                    stage: stage_index,
                    step: step_index,
                    celsius: lid,
                    min: LID_MIN_CELSIUS,
                    max: LID_MAX_CELSIUS,
                });
            }
        }
    }
    Ok(())
}

fn validate_sub_ambient_hold(program: &ThermalProgram) -> Result<(), MethodSetError> {
    let total: f64 = program
        .stages
        .iter()
        .map(|stage| {
            stage
                .steps
                .iter()
                .filter(|step| step.plateau_celsius < AMBIENT_CELSIUS)
                .map(|step| step.hold_seconds * f64::from(stage.repeats))
                .sum::<f64>()
        })
        .sum();
    if total > MAX_SUB_AMBIENT_HOLD_SECONDS {
        Err(MethodSetError::SubAmbientHoldTooLong {
            ambient: AMBIENT_CELSIUS,
            seconds: total,
            max: MAX_SUB_AMBIENT_HOLD_SECONDS,
        })
    } else {
        Ok(())
    }
}

fn check_block(context: &'static str, celsius: f64) -> Result<(), MethodSetError> {
    if !(BLOCK_MIN_CELSIUS..=BLOCK_MAX_CELSIUS).contains(&celsius) {
        Err(MethodSetError::BlockOutOfRange {
            context,
            celsius,
            min: BLOCK_MIN_CELSIUS,
            max: BLOCK_MAX_CELSIUS,
        })
    } else {
        Ok(())
    }
}

fn check_lid(context: &'static str, celsius: f64) -> Result<(), MethodSetError> {
    if !(LID_MIN_CELSIUS..=LID_MAX_CELSIUS).contains(&celsius) {
        Err(MethodSetError::LidOutOfRange {
            context,
            celsius,
            min: LID_MIN_CELSIUS,
            max: LID_MAX_CELSIUS,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(plateau_celsius: f64, hold_seconds: f64) -> ProgramStep {
        ProgramStep {
            plateau_celsius,
            hold_seconds,
            slope_c_per_s: None,
            lid_celsius: None,
        }
    }

    /// 30 cycles of 37 °C for 90 s then 16 °C for 180 s, closing with a
    /// single 60 °C 300 s ligation hold.
    fn golden_gate_profile() -> ThermalProgram {
        ThermalProgram {
            stages: vec![
                ProgramStage {
                    steps: vec![step(37.0, 90.0), step(16.0, 180.0)],
                    repeats: 30,
                },
                ProgramStage {
                    steps: vec![step(60.0, 300.0)],
                    repeats: 1,
                },
            ],
        }
    }

    const TIMESTAMP: &str = "2026-08-09T12:00:00.000-08:00";

    #[test]
    fn a_golden_gate_method_renders_its_loops_and_defaults_exactly() {
        let xml = render_method(
            "lab_profile_001",
            "lab",
            TIMESTAMP,
            &golden_gate_profile(),
            &MethodSettings::default(),
        )
        .expect("a routine assembly profile renders");
        assert_eq!(
            xml,
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><MethodSet>\
             <DeleteAllMethods>false</DeleteAllMethods>\
             <Method methodName=\"lab_profile_001\" creator=\"lab\" \
             dateTime=\"2026-08-09T12:00:00.000-08:00\">\
             <Variant>960000</Variant><PlateType>0</PlateType>\
             <FluidQuantity>0</FluidQuantity><PostHeating>true</PostHeating>\
             <StartBlockTemperature>25</StartBlockTemperature>\
             <StartLidTemperature>105</StartLidTemperature>\
             <Step><Number>1</Number><Slope>4.4</Slope>\
             <PlateauTemperature>37</PlateauTemperature><PlateauTime>90</PlateauTime>\
             <OverShootSlope1>0.1</OverShootSlope1><OverShootTemperature>0</OverShootTemperature>\
             <OverShootTime>0</OverShootTime><OverShootSlope2>0.1</OverShootSlope2>\
             <GotoNumber>0</GotoNumber><LoopNumber>0</LoopNumber>\
             <PIDNumber>1</PIDNumber><LidTemp>105</LidTemp></Step>\
             <Step><Number>2</Number><Slope>4.4</Slope>\
             <PlateauTemperature>16</PlateauTemperature><PlateauTime>180</PlateauTime>\
             <OverShootSlope1>0.1</OverShootSlope1><OverShootTemperature>0</OverShootTemperature>\
             <OverShootTime>0</OverShootTime><OverShootSlope2>0.1</OverShootSlope2>\
             <GotoNumber>1</GotoNumber><LoopNumber>29</LoopNumber>\
             <PIDNumber>1</PIDNumber><LidTemp>105</LidTemp></Step>\
             <Step><Number>3</Number><Slope>4.4</Slope>\
             <PlateauTemperature>60</PlateauTemperature><PlateauTime>300</PlateauTime>\
             <OverShootSlope1>0.1</OverShootSlope1><OverShootTemperature>0</OverShootTemperature>\
             <OverShootTime>0</OverShootTime><OverShootSlope2>0.1</OverShootSlope2>\
             <GotoNumber>0</GotoNumber><LoopNumber>0</LoopNumber>\
             <PIDNumber>1</PIDNumber><LidTemp>105</LidTemp></Step>\
             <PIDSet><PID number=\"1\"><PHeating>60</PHeating><PCooling>80</PCooling>\
             <IHeating>250</IHeating><ICooling>100</ICooling><DHeating>10</DHeating>\
             <DCooling>10</DCooling><PLid>100</PLid><ILid>70</ILid></PID></PIDSet>\
             </Method></MethodSet>",
            "the cycled stage loops from its last step back to its first with repeats - 1 extra passes"
        );
    }

    #[test]
    fn a_stated_ramp_and_per_step_lid_render_verbatim() {
        let mut profile = ThermalProgram {
            stages: vec![ProgramStage {
                steps: vec![step(72.0, 30.0)],
                repeats: 1,
            }],
        };
        profile.stages[0].steps[0].slope_c_per_s = Some(2.5);
        profile.stages[0].steps[0].lid_celsius = Some(110.0);
        let xml = render_method(
            "lab_profile_002",
            "lab",
            TIMESTAMP,
            &profile,
            &MethodSettings::default(),
        )
        .expect("a stated ramp inside the envelope renders");
        assert!(
            xml.contains("<Slope>2.5</Slope>"),
            "the stated ramp overrides the maximum-slope default: {xml}"
        );
        assert!(
            xml.contains("<LidTemp>110</LidTemp>"),
            "the per-step lid overrides the start lid: {xml}"
        );
    }

    #[test]
    fn a_pre_method_renders_exactly() {
        let xml = render_pre_method("lab_hold_95", "lab", TIMESTAMP, 95.0, 105.0, true)
            .expect("a hold inside both envelopes renders");
        assert_eq!(
            xml,
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><MethodSet>\
             <DeleteAllMethods>false</DeleteAllMethods>\
             <PreMethod methodName=\"lab_hold_95\" creator=\"lab\" \
             dateTime=\"2026-08-09T12:00:00.000-08:00\">\
             <TargetBlockTemperature>95</TargetBlockTemperature>\
             <TargetLidTemp>105</TargetLidTemp>\
             <DynamicPreMethodDuration>true</DynamicPreMethodDuration>\
             </PreMethod></MethodSet>"
        );
    }

    #[test]
    fn a_method_set_survives_the_triple_nesting_round_trip() {
        let method_xml = render_method(
            "lab_profile_003",
            "lab",
            TIMESTAMP,
            &golden_gate_profile(),
            &MethodSettings::default(),
        )
        .expect("the profile renders");
        let params = parameter_set(&method_xml);
        let envelope = crate::soap::Command::SetParameters {
            params_xml: params.clone(),
        }
        .envelope(42);

        // Unwind the nesting the way the device does: the envelope's text
        // yields the ParameterSet, whose String text yields the MethodSet.
        let request = crate::soap::SyncResponse::parse(&envelope)
            .expect_err("a request envelope is not a response");
        assert!(
            matches!(request, crate::soap::SoapError::MissingElement { .. }),
            "the request has no returnCode, confirming we parsed the request itself"
        );
        let recovered_params = extract_leaf(&envelope, "paramsXML");
        assert_eq!(
            recovered_params, params,
            "one unescape recovers the ParameterSet"
        );
        let recovered_method = extract_leaf(&recovered_params, "String");
        assert_eq!(
            recovered_method, method_xml,
            "the MethodSet survives escape, embed, and unescape byte for byte"
        );
    }

    /// Pulls one leaf element's unescaped text out of a document, the
    /// same way the parsing layer does.
    fn extract_leaf(xml: &str, element: &str) -> String {
        use quick_xml::events::Event;
        let mut reader = quick_xml::Reader::from_str(xml);
        let mut inside = false;
        let mut text = String::new();
        loop {
            match reader
                .read_event()
                .expect("the test document is well-formed")
            {
                Event::Eof => break,
                Event::Start(start) if start.local_name().as_ref() == element.as_bytes() => {
                    inside = true;
                }
                Event::End(end) if end.local_name().as_ref() == element.as_bytes() => break,
                Event::Text(t) if inside => {
                    text.push_str(&t.xml10_content().expect("the text decodes"));
                }
                Event::GeneralRef(reference) if inside => {
                    if let Some(character) =
                        reference.resolve_char_ref().expect("the reference decodes")
                    {
                        text.push(character);
                    } else {
                        let name = reference.xml10_content().expect("the reference decodes");
                        text.push_str(
                            quick_xml::escape::resolve_predefined_entity(&name)
                                .expect("the entity is predefined"),
                        );
                    }
                }
                _ => {}
            }
        }
        text
    }

    #[test]
    fn a_block_temperature_above_the_ceiling_is_rejected_with_the_range() {
        let mut profile = golden_gate_profile();
        profile.stages[0].steps[0].plateau_celsius = 105.0;
        let error = render_method(
            "lab_profile_004",
            "lab",
            TIMESTAMP,
            &profile,
            &MethodSettings::default(),
        )
        .expect_err("105 °C is above the 99 °C block ceiling");
        assert_eq!(
            error,
            MethodSetError::StepBlockOutOfRange {
                stage: 0,
                step: 0,
                celsius: 105.0,
                min: 4.0,
                max: 99.0,
            }
        );
    }

    #[test]
    fn a_ramp_above_the_device_maximum_is_rejected_with_the_maximum() {
        let mut profile = golden_gate_profile();
        profile.stages[0].steps[0].slope_c_per_s = Some(5.0);
        let error = render_method(
            "lab_profile_005",
            "lab",
            TIMESTAMP,
            &profile,
            &MethodSettings::default(),
        )
        .expect_err("5 °C/s is above the 4.4 °C/s maximum");
        assert_eq!(
            error,
            MethodSetError::SlopeOutOfRange {
                stage: 0,
                step: 0,
                slope: 5.0,
                max: MAX_SLOPE_C_PER_S,
            }
        );
    }

    #[test]
    fn a_long_sub_ambient_hold_is_rejected_with_the_condensation_limit() {
        let profile = ThermalProgram {
            stages: vec![ProgramStage {
                steps: vec![step(4.0, 3600.0)],
                repeats: 3,
            }],
        };
        let error = render_method(
            "lab_profile_006",
            "lab",
            TIMESTAMP,
            &profile,
            &MethodSettings::default(),
        )
        .expect_err("three hours at 4 °C condenses");
        assert_eq!(
            error,
            MethodSetError::SubAmbientHoldTooLong {
                ambient: AMBIENT_CELSIUS,
                seconds: 10800.0,
                max: MAX_SUB_AMBIENT_HOLD_SECONDS,
            }
        );
        // The golden-gate profile's 16 °C plateaus total 90 minutes and
        // stay inside the limit.
        render_method(
            "lab_profile_007",
            "lab",
            TIMESTAMP,
            &golden_gate_profile(),
            &MethodSettings::default(),
        )
        .expect("ninety sub-ambient minutes stay inside the two-hour limit");
    }

    #[test]
    fn timestamps_without_offsets_are_rejected() {
        for bad in [
            "2026-08-09T12:00:00",
            "2026-08-09 12:00:00-08:00",
            "2026-08-09T12:00:00.-08:00",
            "2026-08-09T12:00:00-0800",
            "yesterday",
        ] {
            let error = render_pre_method("lab_hold_1", "lab", bad, 37.0, 105.0, true)
                .expect_err("a timestamp without a strict offset is rejected");
            assert_eq!(
                error,
                MethodSetError::InvalidTimestamp {
                    timestamp: bad.to_string()
                }
            );
        }
        for good in [
            "2026-08-09T12:00:00-08:00",
            "2026-08-09T12:00:00.000+00:00",
            "2026-08-09T12:00:00.503368Z",
        ] {
            render_pre_method("lab_hold_1", "lab", good, 37.0, 105.0, true)
                .expect("a strict ISO-8601 timestamp with an offset is accepted");
        }
    }

    #[test]
    fn method_names_outside_the_safe_alphabet_are_rejected() {
        for bad in ["", "with space", "angle<bracket", "ünïcode"] {
            let error = render_pre_method(bad, "lab", TIMESTAMP, 37.0, 105.0, true)
                .expect_err("an unsafe method name is rejected");
            assert_eq!(
                error,
                MethodSetError::InvalidMethodName {
                    name: bad.to_string()
                }
            );
        }
    }

    #[test]
    fn holds_outside_the_device_envelope_are_rejected() {
        let error = render_pre_method("lab_hold_2", "lab", TIMESTAMP, 3.0, 105.0, true)
            .expect_err("3 °C is under the 4 °C block floor");
        assert_eq!(
            error,
            MethodSetError::BlockOutOfRange {
                context: "the constant hold",
                celsius: 3.0,
                min: 4.0,
                max: 99.0,
            }
        );
        let error = render_pre_method("lab_hold_3", "lab", TIMESTAMP, 37.0, 120.0, true)
            .expect_err("120 °C is above the 115 °C lid ceiling");
        assert_eq!(
            error,
            MethodSetError::LidOutOfRange {
                context: "the constant hold",
                celsius: 120.0,
                min: 30.0,
                max: 115.0,
            }
        );
    }

    #[test]
    fn fluid_quantity_classes_follow_the_documented_volume_bands() {
        assert_eq!(fluid_quantity_class(10.0), 0);
        assert_eq!(fluid_quantity_class(29.9), 0);
        assert_eq!(fluid_quantity_class(30.0), 1);
        assert_eq!(fluid_quantity_class(74.9), 1);
        assert_eq!(fluid_quantity_class(75.0), 2);
        assert_eq!(fluid_quantity_class(100.0), 2);
    }

    #[test]
    fn numbers_render_whole_values_without_a_decimal_point() {
        assert_eq!(number(95.0), "95");
        assert_eq!(number(4.4), "4.4");
        assert_eq!(number(0.1), "0.1");
        assert_eq!(number(-20.0), "-20");
    }
}

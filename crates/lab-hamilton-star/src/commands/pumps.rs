//! Pump-station and heater-shaker commands. Encode-only.
//!
//! Pump stations are addressed through the master; heater-shakers route
//! through their temperature-carrier module `T{index}`.

use crate::commands::Command;
use crate::errors::{CommandError, check_range};
use crate::framing::{FrameBuilder, Module};
use crate::response::{FieldSpec, ResponseParseError, parse_fields};

fn check_station(station: u32) -> Result<(), CommandError> {
    check_range(
        "ep",
        "pump station number",
        "",
        f64::from(station),
        1.0,
        3.0,
    )?;
    Ok(())
}

/// `ET` — request the pump settings of a station (fmt `et#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestPumpSettings {
    station: u32,
}

impl RequestPumpSettings {
    pub fn new(station: u32) -> Result<RequestPumpSettings, CommandError> {
        check_station(station)?;
        Ok(RequestPumpSettings { station })
    }
}

impl Command for RequestPumpSettings {
    const CODE: &'static str = "ET";
    type Response = u8;

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("ep", 1, self.station)
    }
    fn parse_response(payload: &str) -> Result<u8, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("et", 1)])?;
        Ok(fields.int("et").unwrap_or(0).clamp(0, 9) as u8)
    }
}

/// `EJ` — initialize the wash-station valves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitializeValves {
    station: u32,
}

impl InitializeValves {
    pub fn new(station: u32) -> Result<InitializeValves, CommandError> {
        check_station(station)?;
        Ok(InitializeValves { station })
    }
}

impl Command for InitializeValves {
    const CODE: &'static str = "EJ";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("ep", 1, self.station)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `EH` — fill a wash chamber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillChamber {
    station: u32,
    /// `ed`: drain before refilling.
    pub drain_before_refill: bool,
    /// `ek`: the wash-fluid-to-chamber connection code (0: fluid 1 →
    /// chamber 2, 1: fluid 1 → chamber 1, 2: fluid 2 → chamber 1, 3:
    /// fluid 2 → chamber 2).
    pub connection: u32,
    /// `eu`: waste-chamber suck time after the sensor change, seconds.
    pub suck_time: u32,
}

impl FillChamber {
    pub fn new(station: u32, connection: u32) -> Result<FillChamber, CommandError> {
        check_station(station)?;
        check_range(
            "ek",
            "wash-fluid connection code",
            "",
            f64::from(connection),
            0.0,
            3.0,
        )?;
        Ok(FillChamber {
            station,
            drain_before_refill: false,
            connection,
            suck_time: 0,
        })
    }
}

impl Command for FillChamber {
    const CODE: &'static str = "EH";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("ep", 1, self.station)
            .flag("ed", self.drain_before_refill)
            .uint("ek", 1, self.connection)
            .uint("eu", 2, self.suck_time)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `EL` — drain a wash station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainStation {
    station: u32,
}

impl DrainStation {
    pub fn new(station: u32) -> Result<DrainStation, CommandError> {
        check_station(station)?;
        Ok(DrainStation { station })
    }
}

impl Command for DrainStation {
    const CODE: &'static str = "EL";
    type Response = ();

    fn module(&self) -> Module {
        Module::Master
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder.uint("ep", 1, self.station)
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

fn carrier_module(index: u8) -> Module {
    Module::TemperatureCarrier(index)
}

/// `QU` on `T{index}` — query the device type on a temperature carrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterShakerQueryType {
    pub carrier: u8,
}

impl Command for HeaterShakerQueryType {
    const CODE: &'static str = "QU";
    type Response = String;

    fn module(&self) -> Module {
        carrier_module(self.carrier)
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<String, ResponseParseError> {
        Ok(payload.trim().to_string())
    }
}

/// `LI` on `T{index}` — initialize a heater-shaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterShakerInitialize {
    pub carrier: u8,
}

impl Command for HeaterShakerInitialize {
    const CODE: &'static str = "LI";
    type Response = ();

    fn module(&self) -> Module {
        carrier_module(self.carrier)
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `TA` on `T{index}` — set the target temperature. The `tb`/`tc` values
/// are the firmware's expected control constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterShakerSetTemperature {
    pub carrier: u8,
    /// `ta`: target temperature, 0.1 °C.
    temperature: u32,
}

impl HeaterShakerSetTemperature {
    pub fn new(
        carrier: u8,
        temperature_tenth_c: u32,
    ) -> Result<HeaterShakerSetTemperature, CommandError> {
        check_range(
            "ta",
            "target temperature",
            "0.1 °C",
            f64::from(temperature_tenth_c),
            0.0,
            1050.0,
        )?;
        Ok(HeaterShakerSetTemperature {
            carrier,
            temperature: temperature_tenth_c,
        })
    }
}

impl Command for HeaterShakerSetTemperature {
    const CODE: &'static str = "TA";
    type Response = ();

    fn module(&self) -> Module {
        carrier_module(self.carrier)
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
            .uint("ta", 4, self.temperature)
            .text("tb", "1800")
            .text("tc", "0020")
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

/// `RT` on `T{index}` — read the temperatures. The reply carries
/// plus-separated sensor values in 0.1 °C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterShakerReadTemperature {
    pub carrier: u8,
}

impl Command for HeaterShakerReadTemperature {
    const CODE: &'static str = "RT";
    /// The sensor temperatures in 0.1 °C.
    type Response = Vec<i64>;

    fn module(&self) -> Module {
        carrier_module(self.carrier)
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<Vec<i64>, ResponseParseError> {
        Ok(payload
            .split('+')
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.trim().parse().ok())
            .collect())
    }
}

/// `QD` on `T{index}` — query whether the target temperature is reached
/// (fmt `qd#`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterShakerTemperatureReached {
    pub carrier: u8,
}

impl Command for HeaterShakerTemperatureReached {
    const CODE: &'static str = "QD";
    type Response = bool;

    fn module(&self) -> Module {
        carrier_module(self.carrier)
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(payload: &str) -> Result<bool, ResponseParseError> {
        let fields = parse_fields(payload, &[FieldSpec::int("qd", 1)])?;
        Ok(fields.int("qd") == Some(1))
    }
}

/// `TO` on `T{index}` — stop temperature control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterShakerStopTemperature {
    pub carrier: u8,
}

impl Command for HeaterShakerStopTemperature {
    const CODE: &'static str = "TO";
    type Response = ();

    fn module(&self) -> Module {
        carrier_module(self.carrier)
    }
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder {
        builder
    }
    fn parse_response(_payload: &str) -> Result<(), ResponseParseError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::CommandId;

    #[test]
    fn heater_shakers_address_their_temperature_carrier() {
        let command = HeaterShakerSetTemperature::new(2, 370).expect("37.0 °C is in range");
        assert_eq!(
            command.to_wire(CommandId::new(5)),
            "T2TAid0005ta0370tb1800tc0020",
            "the carrier index selects the T-module and tb/tc are the fixed control constants"
        );
    }

    #[test]
    fn pump_stations_beyond_three_are_rejected() {
        let error = RequestPumpSettings::new(4).expect_err("stations run 1–3");
        assert!(
            error.to_string().contains("ep"),
            "the error names the parameter: {error}"
        );
    }

    #[test]
    fn heater_shaker_temperatures_parse_from_plus_separated_values() {
        let temps = HeaterShakerReadTemperature::parse_response("rt+0370+0368")
            .expect("a temperature reply parses");
        assert_eq!(temps, vec![370, 368], "both sensors report in 0.1 °C");
    }
}

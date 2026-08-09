//! Typed firmware commands, one module per firmware subsystem.
//!
//! Every command is a struct with a validated constructor, a two-character
//! code, a target module, and a typed response. Constructors reject
//! out-of-range values with errors naming the parameter, its unit, and the
//! permitted range, because these commands move a heavy, fast machine — no
//! value is guessed silently.

use std::time::Duration;

use crate::framing::{CommandId, FrameBuilder, Module};
use crate::response::ResponseParseError;

pub mod autoload;
pub mod channel_direct;
pub mod core96;
pub mod gripper;
pub mod iswap;
pub mod pipetting;
pub mod pumps;
pub mod system;

/// A typed firmware command.
pub trait Command {
    /// The two-character command code.
    const CODE: &'static str;
    /// Whether the firmware replies at all. `NS` (trigger next step) and
    /// `AB` (not-stop on) are sent without id and produce no reply; the
    /// session must not wait on them.
    const EXPECTS_REPLY: bool = true;
    /// The typed reply.
    type Response;

    /// The module the command is addressed to.
    fn module(&self) -> Module;

    /// Appends this command's parameters, in wire order, with their declared
    /// widths.
    fn encode_parameters(&self, builder: FrameBuilder) -> FrameBuilder;

    /// Parses the reply payload (with the error section already removed).
    fn parse_response(payload: &str) -> Result<Self::Response, ResponseParseError>;

    /// The full wire frame, with the id first when one is given.
    fn to_wire(&self, id: Option<CommandId>) -> String {
        let builder = match id {
            Some(id) => FrameBuilder::with_id(self.module(), Self::CODE, id),
            None => FrameBuilder::new(self.module(), Self::CODE),
        };
        self.encode_parameters(builder).build()
    }
}

/// The read timeout for a command code. Liquid operations and axis searches
/// hold the reply until the motion completes, so they need far more than the
/// 30-second default.
pub fn read_timeout(code: &str) -> Duration {
    let seconds = match code {
        "TP" | "TR" | "DI" | "YL" => 120,
        "AS" | "DS" | "EA" | "ED" | "VI" => 300,
        "EI" => 60,
        "EV" => 20,
        // The 120–240 s band for Z searches: the ceiling, so a slow search
        // is never cut off.
        "ZL" | "ZE" => 240,
        _ => 30,
    };
    Duration::from_secs(seconds)
}

/// Whether a command code is a read-only query (`R*`/`Q*`). Queries are
/// exempt from the session's module locking and run fully parallel.
pub fn is_query(code: &str) -> bool {
    code.starts_with('R') || code.starts_with('Q')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liquid_operations_get_the_five_minute_timeout() {
        assert_eq!(
            read_timeout("AS"),
            Duration::from_secs(300),
            "an aspirate holds its reply until the liquid operation finishes"
        );
        assert_eq!(
            read_timeout("RT"),
            Duration::from_secs(30),
            "queries answer quickly"
        );
    }

    #[test]
    fn query_codes_start_with_r_or_q() {
        assert!(is_query("RT"), "RT reads tip presence");
        assert!(is_query("QW"), "QW reads initialization status");
        assert!(!is_query("AS"), "AS moves the machine");
    }
}

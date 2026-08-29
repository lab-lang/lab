//! Hamilton STAR session construction for exact Asset-bound execution.

#[cfg(feature = "hardware")]
use anyhow::Context;
#[cfg(any(feature = "hardware", test))]
use anyhow::Result;
#[cfg(any(feature = "hardware", test))]
use hamilton_star::{RawCommand, Star};

/// Executes one reviewed frame, retracting to Z-safety if the firmware rejects it.
#[cfg(any(feature = "hardware", test))]
pub(crate) fn execute_frame(star: &Star, command: &RawCommand) -> Result<()> {
    if let Err(error) = star.execute_raw(command) {
        let retract =
            RawCommand::parse("C0ZA").expect("the retract frame is a constant well-formed frame");
        let _ = star.execute_raw(&retract);
        return Err(error.into());
    }
    Ok(())
}

/// Opens the first Hamilton STAR on USB and runs the documented setup choreography.
///
/// The caller may invoke this only after a reviewed facility plan has bound the
/// document to an exact Asset and passed complete preflight validation.
#[cfg(feature = "hardware")]
pub(crate) fn open_usb_star(autoload_park_track: Option<u32>) -> Result<Star> {
    let star = Star::open_usb().context(
        "no Hamilton STAR answered on USB; use --dry-run to review the facility plan without hardware",
    )?;
    star.initialize(hamilton_star::InitializeOptions {
        autoload_park_track,
        ..hamilton_star::InitializeOptions::default()
    })
    .context("the setup choreography failed; the machine is not in a known state")?;
    Ok(star)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hamilton_star::{MockTransport, Transport};

    use super::*;

    #[test]
    fn reviewed_frames_reach_the_star_in_order() {
        let transport = Arc::new(MockTransport::new());
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            vec![format!("{}id{id}er00/00", &command[..4])]
        });
        let star = Star::new(transport.clone() as Arc<dyn Transport>).expect("mock opens");
        let define_tip = RawCommand::parse("C0TTtt00tf1tl0519tv03600tg2tu0").unwrap();
        let retract = RawCommand::parse("C0ZA").unwrap();

        execute_frame(&star, &define_tip).unwrap();
        execute_frame(&star, &retract).unwrap();

        let written = transport.written();
        assert_eq!(written.len(), 2);
        assert!(written[0].starts_with("C0TTid"));
        assert!(written[1].starts_with("C0ZAid"));
    }

    #[test]
    fn a_firmware_error_retracts_to_z_safety() {
        let transport = Arc::new(MockTransport::new());
        transport.set_responder(|command| {
            let id = command.get(6..10).unwrap_or("0000").to_string();
            if &command[2..4] == "TP" {
                vec![format!("C0TPid{id}er07/00")]
            } else {
                vec![format!("{}id{id}er00/00", &command[..4])]
            }
        });
        let star = Star::new(transport.clone() as Arc<dyn Transport>).expect("mock opens");
        let pickup = RawCommand::parse(
            "C0TPxp01179 01179 00000&yp2418 2328 0000&tm1 1 0&tt01tp2244tz2164th2450td0",
        )
        .unwrap();

        let error = execute_frame(&star, &pickup).expect_err("the pickup is rejected");

        assert!(error.to_string().contains("already fitted"), "{error}");
        assert!(transport.written().last().unwrap().starts_with("C0ZAid"));
    }
}

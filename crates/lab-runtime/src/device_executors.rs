//! Live executors for reviewed device documents.
//!
//! Construction is inert. USB and network sessions open only from [`DocumentExecutor::execute`],
//! after the complete facility plan has passed preflight, the registry has resolved every exact
//! binding, and the operator has accepted the pre-run gate.

#[cfg(feature = "hardware")]
use std::net::SocketAddr;

#[cfg(feature = "hardware")]
use anyhow::{Context, Result, bail};
#[cfg(feature = "hardware")]
use hamilton_star::RawCommand;
#[cfg(feature = "hardware")]
use lab_instruments::Thermocycler as _;

#[cfg(feature = "hardware")]
use crate::events::{EventSink, ProgramExtent, RunEvent};
#[cfg(feature = "hardware")]
use crate::execution::{DocumentExecutor, LoadedReviewedDocument};

/// Replays `lab.star-run.v0` on the exact Asset binding registered by the caller.
#[cfg(feature = "hardware")]
pub struct HamiltonStarExecutor {
    asset: String,
    autoload_park_track: Option<u32>,
    session: Option<hamilton_star::Star>,
}

#[cfg(feature = "hardware")]
impl HamiltonStarExecutor {
    pub fn new(asset: impl Into<String>, autoload_park_track: Option<u32>) -> Self {
        Self {
            asset: asset.into(),
            autoload_park_track,
            session: None,
        }
    }

    fn session(&mut self, events: &mut dyn EventSink) -> Result<&hamilton_star::Star> {
        if self.session.is_none() {
            events.emit(RunEvent::Connecting {
                station: self.asset.clone(),
                detail: "the first Hamilton STAR on USB".to_owned(),
            });
            self.session = Some(crate::star::open_usb_star(self.autoload_park_track)?);
            events.emit(RunEvent::Connected {
                station: self.asset.clone(),
            });
        }
        Ok(self
            .session
            .as_ref()
            .expect("the Hamilton STAR session was just opened"))
    }
}

#[cfg(feature = "hardware")]
impl DocumentExecutor for HamiltonStarExecutor {
    fn execute(
        &mut self,
        loaded: &LoadedReviewedDocument,
        events: &mut dyn EventSink,
    ) -> Result<()> {
        let LoadedReviewedDocument::Star { document, commands } = loaded else {
            bail!("the Hamilton STAR executor received a non-STAR document");
        };
        events.emit(RunEvent::ProgramStarted {
            station: self.asset.clone(),
            title: document.title.clone(),
            extent: ProgramExtent::Frames {
                frames: commands.len(),
            },
        });
        let asset = self.asset.clone();
        let session = self.session(events)?;
        for (index, (step, command)) in document.steps.iter().zip(commands).enumerate() {
            events.emit(RunEvent::Frame {
                station: asset.clone(),
                index: index + 1,
                description: step.description.clone(),
            });
            if let Err(error) = session.execute_raw(command) {
                let retract = RawCommand::parse("C0ZA")
                    .expect("the retract frame is a constant well-formed frame");
                let _ = session.execute_raw(&retract);
                bail!(
                    "firmware error at frame {}: {error}; channels were retracted to Z-safety",
                    index + 1
                );
            }
        }
        Ok(())
    }
}

/// Runs `lab.thermocycle-run.v0` on one exact network-addressed Inheco ODTC Asset.
#[cfg(feature = "hardware")]
pub struct OdtcExecutor {
    asset: String,
    address: SocketAddr,
    session: Option<lab_instruments::OdtcStation>,
}

#[cfg(feature = "hardware")]
impl OdtcExecutor {
    pub fn new(asset: impl Into<String>, address: SocketAddr) -> Self {
        Self {
            asset: asset.into(),
            address,
            session: None,
        }
    }

    fn session(&mut self, events: &mut dyn EventSink) -> Result<&mut lab_instruments::OdtcStation> {
        if self.session.is_none() {
            events.emit(RunEvent::Connecting {
                station: self.asset.clone(),
                detail: self.address.to_string(),
            });
            self.session = Some(
                lab_instruments::OdtcStation::connect(self.address).with_context(|| {
                    format!(
                        "the Inheco ODTC Asset '{}' did not answer at {}",
                        self.asset, self.address
                    )
                })?,
            );
            events.emit(RunEvent::Connected {
                station: self.asset.clone(),
            });
        }
        Ok(self
            .session
            .as_mut()
            .expect("the Inheco ODTC session was just opened"))
    }
}

#[cfg(feature = "hardware")]
impl DocumentExecutor for OdtcExecutor {
    fn execute(
        &mut self,
        loaded: &LoadedReviewedDocument,
        events: &mut dyn EventSink,
    ) -> Result<()> {
        let LoadedReviewedDocument::Thermocycle(document) = loaded else {
            bail!("the Inheco ODTC executor received a non-thermocycle document");
        };
        events.emit(RunEvent::ProgramStarted {
            station: self.asset.clone(),
            title: document.title.clone(),
            extent: ProgramExtent::Plateaus {
                plateaus: document.profile.total_steps(),
                final_hold_celsius: document.final_hold_celsius,
            },
        });
        let asset = self.asset.clone();
        let session = self.session(events)?;
        let handle = session
            .run_profile(&document.profile)
            .with_context(|| format!("could not start '{}' on {asset}", document.id))?;
        events.emit(RunEvent::ThermalRunning {
            station: asset.clone(),
        });
        session
            .await_completion(handle)
            .with_context(|| format!("'{}' did not complete on {asset}", document.id))?;
        for warning in session.take_warnings() {
            events.emit(RunEvent::ThermalWarning {
                station: asset.clone(),
                warning,
            });
        }
        if let Some(celsius) = document.final_hold_celsius {
            session
                .hold_block(celsius, None)
                .with_context(|| format!("could not hold {celsius} C on {asset}"))?;
            events.emit(RunEvent::ThermalHold {
                station: asset,
                celsius,
            });
        }
        Ok(())
    }
}

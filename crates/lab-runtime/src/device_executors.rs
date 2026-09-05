//! Live executors for reviewed device documents.
//!
//! Construction is inert. USB and network sessions open only from [`DocumentExecutor::execute`],
//! after the complete facility plan has passed preflight, the registry has resolved every exact
//! binding, and the operator has accepted the pre-run gate.

#[cfg(feature = "hardware")]
use std::net::SocketAddr;

use anyhow::Result;
#[cfg(feature = "hardware")]
use anyhow::{Context, bail};
#[cfg(feature = "hardware")]
use lab_instruments::Thermocycler as _;

use crate::events::EventSink;
#[cfg(feature = "hardware")]
use crate::events::{ProgramExtent, RunEvent};
use crate::execution::{DocumentExecutor, LoadedReviewedDocument};

/// A no-hardware executor for an already validated reviewed document.
///
/// Semantic behavior belongs to the document producer or a future domain simulator. This
/// executor deliberately performs no device I/O and exists only in an explicitly selected
/// simulation registry.
#[derive(Default)]
pub struct ReviewedDocumentSimulationExecutor;

impl DocumentExecutor for ReviewedDocumentSimulationExecutor {
    fn execute(
        &mut self,
        _loaded: &LoadedReviewedDocument,
        _events: &mut dyn EventSink,
    ) -> Result<()> {
        Ok(())
    }
}

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
                asset: self.asset.clone(),
                detail: "the first Hamilton STAR on USB".to_owned(),
            });
            self.session = Some(crate::star::open_usb_star(self.autoload_park_track)?);
            events.emit(RunEvent::Connected {
                asset: self.asset.clone(),
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
            asset: self.asset.clone(),
            title: document.title.clone(),
            extent: ProgramExtent::Frames {
                frames: commands.len(),
            },
        });
        let asset = self.asset.clone();
        let session = self.session(events)?;
        for (index, (step, command)) in document.steps.iter().zip(commands).enumerate() {
            events.emit(RunEvent::Frame {
                asset: asset.clone(),
                index: index + 1,
                description: step.description.clone(),
            });
            if let Err(error) = crate::star::execute_frame(session, command) {
                bail!(
                    "firmware error at frame {}: {error}; channels were retracted to Z-safety",
                    index + 1
                );
            }
        }
        Ok(())
    }
}

/// Runs `lab.thermocycle-run.v1` on one exact network-addressed Inheco ODTC Asset.
#[cfg(feature = "hardware")]
pub struct OdtcExecutor {
    asset: String,
    address: SocketAddr,
}

#[cfg(feature = "hardware")]
impl OdtcExecutor {
    pub fn new(asset: impl Into<String>, address: SocketAddr) -> Self {
        Self {
            asset: asset.into(),
            address,
        }
    }

    fn connect_for_run(
        &self,
        run: &lab_instruments::ThermalRun,
        events: &mut dyn EventSink,
    ) -> Result<lab_instruments::OdtcStation> {
        events.emit(RunEvent::Connecting {
            asset: self.asset.clone(),
            detail: format!(
                "{}; {} samples at {} µL each",
                self.address, run.sample_count, run.fill_volume_ul
            ),
        });
        let session = lab_instruments::OdtcStation::connect_for_run(self.address, run)
            .with_context(|| {
                format!(
                    "the Inheco ODTC Asset '{}' did not answer at {}",
                    self.asset, self.address
                )
            })?;
        events.emit(RunEvent::Connected {
            asset: self.asset.clone(),
        });
        Ok(session)
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
            asset: self.asset.clone(),
            title: document.title.clone(),
            extent: ProgramExtent::Plateaus {
                plateaus: document.run.profile.total_steps(),
                final_hold_celsius: document.run.final_hold_celsius,
            },
        });
        let asset = self.asset.clone();
        // `inheco-sila` freezes MethodSettings when it connects. One fresh
        // session per reviewed run makes the document's fill volume the exact
        // control-class input and prevents settings from leaking across runs.
        let mut session = self.connect_for_run(&document.run, events)?;
        let handle = session
            .start_run(&document.run)
            .with_context(|| format!("could not start '{}' on {asset}", document.id))?;
        events.emit(RunEvent::ThermalRunning {
            asset: asset.clone(),
        });
        session
            .await_completion(handle)
            .with_context(|| format!("'{}' did not complete on {asset}", document.id))?;
        if let Some(celsius) = document.run.final_hold_celsius {
            session
                .hold_block(celsius, None)
                .with_context(|| format!("could not hold {celsius} C on {asset}"))?;
            events.emit(RunEvent::ThermalHold {
                asset: asset.clone(),
                celsius,
            });
        }
        for warning in session.take_warnings() {
            events.emit(RunEvent::ThermalWarning {
                asset: asset.clone(),
                warning,
            });
        }
        Ok(())
    }
}

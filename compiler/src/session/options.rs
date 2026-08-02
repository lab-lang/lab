use std::path::PathBuf;

use pliron::pass::PMConfig;

/// Configuration for verification and pass instrumentation in one compiler session.
#[derive(Clone, Debug)]
pub struct SessionOptions {
    /// Verify the complete IR immediately before and after every pass.
    pub verify_each: bool,
    /// Log the complete IR immediately before every pass.
    pub print_before_all: bool,
    /// Log the complete IR immediately after every pass.
    pub print_after_all: bool,
    /// Measure and log the running time of every pass.
    pub time_passes: bool,
    /// Write requested before/after IR snapshots to this directory.
    pub ir_printing_dir: Option<PathBuf>,
}

impl SessionOptions {
    pub(crate) fn pass_manager_config(&self) -> PMConfig {
        PMConfig {
            print_before_all: self.print_before_all,
            print_after_all: self.print_after_all,
            ir_printing_dir: self.ir_printing_dir.clone(),
            verify_before_all: self.verify_each,
            verify_after_all: self.verify_each,
            time_all_passes: self.time_passes,
            ..PMConfig::default()
        }
    }
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            verify_each: true,
            print_before_all: false,
            print_after_all: false,
            time_passes: false,
            ir_printing_dir: None,
        }
    }
}

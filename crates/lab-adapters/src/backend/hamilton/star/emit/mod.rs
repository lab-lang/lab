//! Rendering for independently reviewable `lab.star-run.v0` documents.

mod runs;

pub(in crate::backend::hamilton::star) use runs::render_run;

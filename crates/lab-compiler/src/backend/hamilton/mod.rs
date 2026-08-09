//! Hamilton backends. This module is the vendor family; [`star`] is the
//! machine under it, the containment boundary for STAR deck vocabulary, the
//! vendored carrier catalog, and the emitted run format. Planning common to
//! any liquid handler lives beside the backend contracts, not here.

pub mod star;

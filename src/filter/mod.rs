//! Web protection core: DNS packet handling, block-list parsing and matching,
//! and the config/status files shared by the filter service and the app.
//! Everything here is portable (no Windows calls) so it is unit-tested on any host.

pub mod adapters;
pub mod config;
pub mod control;
pub mod dns;
pub mod fetch;
pub mod lists;
pub mod matcher;
#[cfg(windows)]
pub mod routing;
#[cfg(windows)]
pub mod scm;
pub mod server;
pub mod service;

/// The Windows service that answers lookups for web protection.
pub const SERVICE_NAME: &str = "SecblitzFilter";

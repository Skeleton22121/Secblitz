//! Web protection core: DNS packet handling, block-list parsing and matching,
//! and the config/status files shared by the filter service and the app.
//! Everything here is portable (no Windows calls) so it is unit-tested on any host.

pub mod adapters;
pub mod config;
pub mod dns;
pub mod fetch;
pub mod lists;
pub mod matcher;
pub mod server;
pub mod service;

/// Name of the Windows service that answers on loopback.
pub const SERVICE_NAME: &str = "SecblitzFilter";

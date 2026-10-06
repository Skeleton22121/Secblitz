//! Web protection core: DNS packets, block-list parsing and matching, and the
//! config/status files shared with the app. Portable, so tests run on any host.

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

pub const SERVICE_NAME: &str = "SecblitzFilter";

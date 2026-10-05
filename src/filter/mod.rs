//! Web protection core: DNS packet handling, block-list parsing and matching,
//! and the config/status files shared by the filter service and the app.
//! Everything here is portable (no Windows calls) so it is unit-tested on any host.

pub mod config;
pub mod dns;
pub mod lists;
pub mod matcher;

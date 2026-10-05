//! The one Windows lookup rule that sends every name to the filter. It is
//! read and changed through `nrpt.ps1` (Windows PowerShell 5.1, hidden); the
//! script only ever touches the rule that carries Secblitz's own marks.

use anyhow::{ensure, Result};
use std::net::IpAddr;
use std::time::Duration;

use super::control::{parse_shown, NRPT_SCRIPT};
use crate::debloat::windows::run_with_modules;

const MODULES: &[&str] = &[
    "Microsoft.PowerShell.Management",
    "Microsoft.PowerShell.Utility",
    "DnsClient",
];
const TIMEOUT: Duration = Duration::from_secs(60);

fn run(mode: &str, servers: &str) -> Result<Option<Vec<IpAddr>>> {
    let line = run_with_modules(
        NRPT_SCRIPT,
        MODULES,
        &[
            ("SECBLITZ_NRPT_MODE", mode),
            ("SECBLITZ_NRPT_SERVERS", servers),
        ],
        TIMEOUT,
    )?;
    parse_shown(&line)
}

/// The servers of our rule, or `None` when there is no rule of ours.
pub fn current_rule() -> Result<Option<Vec<IpAddr>>> {
    run("Show", "")
}

pub fn set_rule(servers: &[IpAddr]) -> Result<()> {
    ensure!(
        !servers.is_empty() && servers.len() <= 12,
        "Wrong number of servers"
    );
    let list = servers
        .iter()
        .map(IpAddr::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let now = run("Set", &list)?;
    ensure!(now.is_some(), "The web protection rule was not added");
    Ok(())
}

pub fn remove_rule() -> Result<()> {
    let now = run("Remove", "")?;
    ensure!(now.is_none(), "The web protection rule is still there");
    Ok(())
}

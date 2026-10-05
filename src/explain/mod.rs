//! Plain-language explanation for every check: what it is, what can happen
//! if it is off, and what changes when it is turned on (or what to do, for
//! checks Secblitz only reports). Every string is a translation source key.
//!
//! Each area keeps its own table so the catalogs stay small and readable.

mod core;
mod detect;
mod network;
mod system;
mod user;

/// The three short lines shown when a person opens a check's details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Explainer {
    /// "What it is": one plain sentence, no jargon.
    pub what: &'static str,
    /// "If it's off": a concrete what-if a non-technical person recognises.
    pub risk: &'static str,
    /// "If you turn it on" (or "What you can do" for report-only checks):
    /// what the person will notice, including any downside.
    pub change: &'static str,
}

/// Explanation for a control id, diagnostics rule id or finding title.
pub fn for_check(id: &str) -> Option<Explainer> {
    core::get(id)
        .or_else(|| network::get(id))
        .or_else(|| system::get(id))
        .or_else(|| user::get(id))
        .or_else(|| detect::get(id))
}

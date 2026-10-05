//! Fix-flow rules ported from the former terminal guide.
//!
//! OWNER: app-core agent. Contract (do not change signatures without updating
//! all callers):
//! - `candidates(report, available)` — ids a user may select as fixes.
//! - `recommended(report, available)` — the default-ticked subset.
//! - `payoff(attempted, applied, verified)` — (protected now, after restart)
//!   translation keys, only for fixes confirmed by the fresh post-check.
//! - `summarize(...)` — the plain-language result card model.
use crate::advice::{self, Group, NextStep};
use secblitz::engine::Report;

/// Ids that can be offered as fixes. Empty while anything is still pending.
pub fn candidates(report: &Report, available: &[String]) -> Vec<String> {
    if report.findings.iter().any(|f| f.status == "pending")
        || report.results.iter().any(|r| r.status == "pending")
    {
        return Vec::new();
    }
    let mut ids = Vec::new();
    for r in &report.results {
        if r.status == "attention"
            && available.contains(&r.id)
            && advice::for_outcome(r).step == NextStep::Repair
            && !ids.contains(&r.id)
        {
            ids.push(r.id.clone());
        }
    }
    ids
}

/// The default-ticked set. Currently identical to `candidates`.
pub fn recommended(report: &Report, available: &[String]) -> Vec<String> {
    candidates(report, available)
}

/// Impact source keys earned by this batch: (protected now, protected after restart).
/// Evidence only: applied in this batch AND confirmed Protected by the post-check.
pub fn payoff(
    attempted: &[String],
    applied: &Report,
    verified: &Report,
) -> (Vec<String>, Vec<String>) {
    let mut now: Vec<String> = Vec::new();
    let mut after_restart: Vec<String> = Vec::new();
    for id in attempted {
        let impact = advice::control_impact(id);
        let Some(change) = applied
            .results
            .iter()
            .find(|r| r.id == *id && r.status == "applied")
        else {
            continue;
        };
        let confirmed = verified
            .results
            .iter()
            .any(|r| r.id == *id && advice::for_outcome(r).group == Group::Protected);
        if impact.is_empty() || !confirmed {
            continue;
        }
        let list = if advice::for_control(id, &change.status, &change.detail).step
            == NextStep::Restart
        {
            &mut after_restart
        } else {
            &mut now
        };
        if !list.iter().any(|k| k == impact) {
            list.push(impact.to_owned());
        }
    }
    (now, after_restart)
}

/// Plain-language outcome of an apply or undo, ready for the result card.
/// All strings are English translation source keys or control ids; the view
/// translates them with `Lang::t` / `Lang::control`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub kind: SummaryKind,
    /// Impact keys: "You're now protected from: …"
    pub protected_now: Vec<String>,
    /// Impact keys: "After you restart, you'll be protected from: …"
    pub after_restart: Vec<String>,
    /// Control ids that were changed successfully (apply) or restored (undo).
    pub done: Vec<String>,
    /// Control ids that could not be completed, with a plain reason key.
    pub not_done: Vec<(String, String)>,
    /// True when the post-check failed; current protection is unverified.
    pub unverified: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SummaryKind {
    #[default]
    Success,
    Partial,
    Failed,
}

/// Build the result card. `attempted` is `Some(ids)` for apply, `None` for undo.
/// `result` is the operation outcome; `verify` the fresh post-check.
pub fn summarize(
    attempted: Option<&[String]>,
    result: Result<&Report, &str>,
    verify: Result<&Report, &str>,
) -> Summary {
    // TODO(app-core): full implementation + tests. Placeholder keeps the
    // skeleton compiling.
    let _ = (attempted, result, verify);
    Summary::default()
}

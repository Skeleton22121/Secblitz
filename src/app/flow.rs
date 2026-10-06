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

/// The default-ticked set: every fixable id except choices the person has to
/// make themselves (those are shown unticked with their consequence).
pub fn recommended(report: &Report, available: &[String]) -> Vec<String> {
    candidates(report, available)
        .into_iter()
        .filter(|id| !advice::is_choice(id))
        .collect()
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
        let list =
            if advice::for_control(id, &change.status, &change.detail).step == NextStep::Restart {
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
    /// Impact keys: "You're now protected from: ..."
    pub protected_now: Vec<String>,
    /// Impact keys: "After you restart, you'll be protected from: ..."
    pub after_restart: Vec<String>,
    /// Control ids that were changed successfully (apply) or restored (undo).
    pub done: Vec<String>,
    /// Control ids that could not be completed, with a plain reason key.
    pub not_done: Vec<(String, String)>,
    /// True when the post-check failed; current protection is unverified.
    pub unverified: bool,
    /// At least one completed change only takes effect after a restart.
    pub restart: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SummaryKind {
    #[default]
    Success,
    Partial,
    Failed,
}

pub const REASON_BLOCKED: &str =
    "Windows didn't allow this change. Restart your PC, then check again and try once more.";
pub const REASON_RESTART: &str = "Restart your PC first, then try again.";
pub const REASON_MANAGED: &str =
    "Your organization manages this setting, so we left it alone. Ask whoever looks after this PC.";
pub const REASON_UNDO_FIRST: &str = "Press Undo to put back your last fixes, then try again.";
pub const REASON_CHANGED: &str =
    "Something else changed this while we were working. Check again, then try again.";
pub const REASON_STILL_OPEN: &str =
    "This still needs attention after the fix. Restart your PC and check again.";
pub const REASON_KEPT: &str = "We kept your current setting to stay safe. Nothing needs doing.";

/// Shown when something fails in a way we do not recognise. Never echoes raw text.
pub const FAILURE_GENERAL: &str = "Something went wrong and nothing was changed. Close Secblitz and open it again. If it keeps happening, restart your PC or check for a Secblitz update.";
pub const COULDNT_READ: &str = "We couldn't read this from Windows. Check again in a moment. If it keeps happening, restart your PC.";
pub const NOT_DONE: &str = "This change did not go through. Restart your PC, then try again.";

/// Plain-words explanation for a whole-operation failure (the engine could not
/// start, a check or a fix stopped). The raw text only ever picks the message;
/// it is never shown.
pub fn plain_failure(raw: &str) -> &'static str {
    let r = raw.to_ascii_lowercase();
    if r.contains("holds the journal lock") || r.contains("another secblitz") {
        "Another Secblitz window is already making changes. Close it, wait a moment, then try again."
    } else if r.contains("journal") || r.contains("transaction") {
        "Secblitz can't read its record of your earlier changes, so it stopped to stay safe. Restart your PC and try again. If it keeps happening, check for a Secblitz update."
    } else if r.contains("administrator")
        || r.contains("access is denied")
        || r.contains("elevat")
        || r.contains("split-token")
        || r.contains("permission")
    {
        "Secblitz needs an account that can make changes to this PC. Sign in with one, then open Secblitz again."
    } else if r.contains("could not be read") || r.contains("could not be collected") {
        COULDNT_READ
    } else if r.contains("engine stopped") {
        "Secblitz stopped unexpectedly. Close it and open it again."
    } else if r.contains("select at least one") {
        "Pick at least one fix first."
    } else {
        FAILURE_GENERAL
    }
}

/// Plain (status, next step) pair for a "More details" line. Raw backend text
/// is never part of the answer.
pub fn plain_detail(status: &str, a: &advice::Advice) -> (&'static str, &'static str) {
    match status {
        "unknown" | "error" => ("Couldn't check", COULDNT_READ),
        "skipped" if a.status == "Needs your choice" => ("Left as it is", REASON_KEPT),
        "applied" | "unchanged" | "restored" | "skipped" | "conflict" | "pending"
        | "attention" | "compliant" | "ok" | "info" => (a.status, a.next),
        _ => ("Not done", NOT_DONE),
    }
}

/// Plain reason key for an outcome that did not complete.
fn reason(status: &str, detail: &str, id: &str) -> &'static str {
    let advice = advice::for_control(id, status, detail);
    if advice.status == "Managed elsewhere" {
        REASON_MANAGED
    } else if status == "conflict" {
        REASON_CHANGED
    } else if status == "pending" || detail.starts_with("Revert the active transaction") {
        REASON_UNDO_FIRST
    } else if detail.to_ascii_lowercase().contains("restart") {
        REASON_RESTART
    } else if status == "skipped" {
        REASON_KEPT
    } else {
        REASON_BLOCKED
    }
}

fn restart_needed(r: &secblitz::engine::Outcome) -> bool {
    advice::for_control(&r.id, &r.status, &r.detail).step == NextStep::Restart
}

/// Build the result card. `attempted` is `Some(ids)` for apply, `None` for undo.
/// `result` is the operation outcome; `verify` the fresh post-check.
/// Protection is only ever claimed for fixes the fresh post-check confirms.
pub fn summarize(
    attempted: Option<&[String]>,
    result: Result<&Report, &str>,
    verify: Result<&Report, &str>,
) -> Summary {
    let mut s = Summary {
        unverified: verify.is_err(),
        ..Summary::default()
    };
    let verified = verify.ok();
    match (attempted, result) {
        (Some(ids), Err(_)) => {
            s.not_done = ids
                .iter()
                .map(|id| (id.clone(), REASON_BLOCKED.to_owned()))
                .collect();
        }
        (Some(ids), Ok(report)) => {
            for id in ids {
                match report.results.iter().find(|r| r.id == *id) {
                    Some(r)
                        if r.status == "applied"
                            || (r.status == "unchanged"
                                && advice::for_control(&r.id, &r.status, &r.detail).group
                                    == Group::Protected) =>
                    {
                        let confirmed = verified.is_none_or(|v| {
                            v.results.iter().any(|o| {
                                o.id == *id && advice::for_outcome(o).group == Group::Protected
                            })
                        });
                        if confirmed {
                            s.restart |= r.status == "applied" && restart_needed(r);
                            s.done.push(id.clone());
                        } else {
                            s.not_done.push((id.clone(), REASON_STILL_OPEN.to_owned()));
                        }
                    }
                    Some(r) => s
                        .not_done
                        .push((id.clone(), reason(&r.status, &r.detail, id).to_owned())),
                    None => s.not_done.push((id.clone(), REASON_BLOCKED.to_owned())),
                }
            }
            if let Some(v) = verified {
                let (now, later) = payoff(ids, report, v);
                s.protected_now = now;
                s.after_restart = later;
            }
        }
        (None, Err(_)) => {
            s.kind = SummaryKind::Failed;
            return s;
        }
        (None, Ok(report)) => {
            for r in &report.results {
                match r.status.as_str() {
                    "restored" => {
                        s.restart |= restart_needed(r);
                        s.done.push(r.id.clone());
                    }
                    "unchanged" | "ok" | "compliant" => {}
                    _ => s
                        .not_done
                        .push((r.id.clone(), reason(&r.status, &r.detail, &r.id).to_owned())),
                }
            }
        }
    }
    s.kind = if s.done.is_empty() {
        SummaryKind::Failed
    } else if s.not_done.is_empty() {
        SummaryKind::Success
    } else {
        SummaryKind::Partial
    };
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use secblitz::engine::Outcome;

    fn out(id: &str, status: &str, detail: &str) -> Outcome {
        Outcome {
            id: id.into(),
            status: status.into(),
            detail: detail.into(),
            ..Outcome::default()
        }
    }
    fn rep(results: Vec<Outcome>) -> Report {
        Report {
            results,
            ..Report::default()
        }
    }
    #[test]
    fn choices_are_candidates_but_never_pre_selected() {
        let r = rep(vec![
            out("printer.point_and_print", "attention", ""),
            out("lsa.run_as_ppl", "attention", ""),
            out("privacy.advertising_id", "attention", ""),
            out("autorun.disabled", "compliant", ""),
        ]);
        let available = ids(&[
            "printer.point_and_print",
            "lsa.run_as_ppl",
            "privacy.advertising_id",
            "autorun.disabled",
        ]);
        assert_eq!(
            candidates(&r, &available),
            ids(&[
                "printer.point_and_print",
                "lsa.run_as_ppl",
                "privacy.advertising_id"
            ])
        );
        assert_eq!(
            recommended(&r, &available),
            ids(&["printer.point_and_print"])
        );
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn payoff_only_counts_verified_protected_ids_from_attempted_list() {
        let applied = rep(vec![out("uac.enabled", "applied", "")]);
        let verified = rep(vec![out("uac.enabled", "compliant", "")]);
        let (now, restart) = payoff(&ids(&["uac.enabled"]), &applied, &verified);
        assert!(now[0].contains("system-wide"), "{now:?}");
        assert!(restart.is_empty());
        let (none, _) = payoff(&[], &applied, &verified);
        assert!(none.is_empty());
        let (none, _) = payoff(&ids(&["uac.enabled"]), &applied, &Report::default());
        assert!(none.is_empty());
    }

    #[test]
    fn payoff_restart_required_id_lands_in_second_list() {
        let applied = rep(vec![out(
            "uac.enabled",
            "applied",
            "Preference applied; restart required",
        )]);
        let verified = rep(vec![out("uac.enabled", "compliant", "")]);
        let (now, restart) = payoff(&ids(&["uac.enabled"]), &applied, &verified);
        assert!(now.is_empty());
        assert_eq!(restart.len(), 1);
    }

    #[test]
    fn payoff_still_broken_or_unknown_or_duplicate() {
        let applied = rep(vec![out("uac.enabled", "applied", "")]);
        let verified = rep(vec![out("uac.enabled", "attention", "")]);
        let (now, restart) = payoff(&ids(&["uac.enabled"]), &applied, &verified);
        assert!(now.is_empty() && restart.is_empty());

        let applied = rep(vec![out("unknown.control", "applied", "")]);
        let verified = rep(vec![out("unknown.control", "compliant", "")]);
        let (now, restart) = payoff(&ids(&["unknown.control"]), &applied, &verified);
        assert!(now.is_empty() && restart.is_empty());

        let both = ["permissions.service.bits", "permissions.service.wuauserv"];
        let applied = rep(both.iter().map(|i| out(i, "applied", "")).collect());
        let verified = rep(both.iter().map(|i| out(i, "compliant", "")).collect());
        let (now, _) = payoff(&ids(&both), &applied, &verified);
        assert_eq!(now.len(), 1);
    }

    #[test]
    fn payoff_contradictory_firewall_evidence_excluded() {
        use secblitz::model::{Authority, EffectiveFirewall, InboundAction};
        let id = "firewall.public.inbound";
        let applied = rep(vec![Outcome {
            effective: Some(EffectiveFirewall::Inbound(InboundAction::Block)),
            authority: Some(Authority::Local),
            ..out(id, "applied", "Preference applied; restart required")
        }]);
        let verified = rep(vec![Outcome {
            effective: Some(EffectiveFirewall::Inbound(InboundAction::Allow)),
            authority: Some(Authority::Local),
            ..out(id, "applied", "")
        }]);
        let (now, restart) = payoff(&ids(&[id]), &applied, &verified);
        assert!(now.is_empty() && restart.is_empty());
    }

    #[test]
    fn candidates_are_attention_repairs_only() {
        let report = rep(vec![
            out("uac.enabled", "attention", ""),
            out("uac.consent", "compliant", ""),
            out("not.available", "attention", ""),
            out("uac.enabled", "attention", ""),
        ]);
        let available = ids(&["uac.enabled", "uac.consent"]);
        assert_eq!(candidates(&report, &available), ids(&["uac.enabled"]));
        assert_eq!(recommended(&report, &available), ids(&["uac.enabled"]));
    }

    #[test]
    fn pending_recovery_blocks_every_candidate() {
        let available = ids(&["uac.enabled"]);
        let mut report = rep(vec![out("uac.enabled", "attention", "")]);
        report.findings.push(secblitz::model::Finding {
            title: "Journal recovery".into(),
            status: "pending".into(),
            detail: String::new(),
        });
        assert!(candidates(&report, &available).is_empty());
        let report = rep(vec![
            out("uac.enabled", "attention", ""),
            out("uac.consent", "pending", ""),
        ]);
        assert!(candidates(&report, &available).is_empty());
    }

    #[test]
    fn apply_success_with_confirmation() {
        let a = ids(&["uac.enabled"]);
        let applied = rep(vec![out("uac.enabled", "applied", "")]);
        let verified = rep(vec![out("uac.enabled", "compliant", "")]);
        let s = summarize(Some(&a), Ok(&applied), Ok(&verified));
        assert_eq!(s.kind, SummaryKind::Success);
        assert_eq!(s.done, a);
        assert_eq!(s.protected_now.len(), 1);
        assert!(!s.unverified && !s.restart && s.not_done.is_empty());
    }

    #[test]
    fn apply_restart_goes_to_after_restart() {
        let a = ids(&["uac.enabled"]);
        let applied = rep(vec![out(
            "uac.enabled",
            "applied",
            "Preference applied; restart required",
        )]);
        let verified = rep(vec![out("uac.enabled", "compliant", "")]);
        let s = summarize(Some(&a), Ok(&applied), Ok(&verified));
        assert_eq!(s.kind, SummaryKind::Success);
        assert!(s.restart && s.protected_now.is_empty());
        assert_eq!(s.after_restart.len(), 1);
    }

    #[test]
    fn apply_partial_maps_reasons() {
        let a = ids(&["uac.enabled", "uac.consent", "defender.ioav"]);
        let applied = rep(vec![
            out("uac.enabled", "applied", ""),
            out(
                "uac.consent",
                "skipped",
                "Domain-managed machine: assessment only",
            ),
            out("defender.ioav", "error", "access denied"),
        ]);
        let verified = rep(vec![
            out("uac.enabled", "compliant", ""),
            out("uac.consent", "attention", ""),
            out("defender.ioav", "attention", ""),
        ]);
        let s = summarize(Some(&a), Ok(&applied), Ok(&verified));
        assert_eq!(s.kind, SummaryKind::Partial);
        assert_eq!(s.done, ids(&["uac.enabled"]));
        assert_eq!(
            s.not_done,
            vec![
                ("uac.consent".to_owned(), REASON_MANAGED.to_owned()),
                ("defender.ioav".to_owned(), REASON_BLOCKED.to_owned()),
            ]
        );
    }

    #[test]
    fn failed_apply_is_failed_and_never_claims_protection() {
        let a = ids(&["uac.enabled"]);
        let verified = rep(vec![out("uac.enabled", "compliant", "")]);
        let s = summarize(Some(&a), Err("disk full"), Ok(&verified));
        assert_eq!(s.kind, SummaryKind::Failed);
        assert!(s.protected_now.is_empty() && s.after_restart.is_empty() && s.done.is_empty());
        assert_eq!(s.not_done.len(), 1);
        assert!(!s.unverified);
        // The cause is technical and must not leak into the plain model.
        assert!(!format!("{s:?}").contains("disk full"));
    }

    #[test]
    fn failed_post_check_is_unverified_and_has_no_payoff() {
        let a = ids(&["uac.enabled"]);
        let applied = rep(vec![out("uac.enabled", "applied", "")]);
        let s = summarize(Some(&a), Ok(&applied), Err("scan unavailable"));
        assert!(s.unverified);
        assert!(s.protected_now.is_empty() && s.after_restart.is_empty());
        assert_eq!(s.done, a);
        let s = summarize(Some(&a), Err("x"), Err("y"));
        assert!(s.unverified);
        assert_eq!(s.kind, SummaryKind::Failed);
    }

    #[test]
    fn applied_but_still_open_after_check_is_not_done() {
        let a = ids(&["uac.enabled"]);
        let applied = rep(vec![out("uac.enabled", "applied", "")]);
        let verified = rep(vec![out("uac.enabled", "attention", "")]);
        let s = summarize(Some(&a), Ok(&applied), Ok(&verified));
        assert_eq!(s.kind, SummaryKind::Failed);
        assert_eq!(s.not_done[0].1, REASON_STILL_OPEN);
    }

    #[test]
    fn missing_result_and_restart_detail_reasons() {
        let a = ids(&["uac.enabled", "uac.consent"]);
        let applied = rep(vec![out("uac.consent", "skipped", "restart the PC first")]);
        let s = summarize(Some(&a), Ok(&applied), Ok(&Report::default()));
        assert_eq!(s.not_done[0].1, REASON_BLOCKED);
        assert_eq!(s.not_done[1].1, REASON_RESTART);
    }

    #[test]
    fn undo_success_partial_failed() {
        let verified = rep(vec![]);
        let ok = rep(vec![out("uac.enabled", "restored", "")]);
        let s = summarize(None, Ok(&ok), Ok(&verified));
        assert_eq!(s.kind, SummaryKind::Success);
        assert_eq!(s.done, ids(&["uac.enabled"]));
        assert!(s.protected_now.is_empty());

        let mixed = rep(vec![
            out(
                "uac.enabled",
                "restored",
                "Original preference restored; restart required",
            ),
            out("uac.consent", "conflict", ""),
        ]);
        let s = summarize(None, Ok(&mixed), Err("no"));
        assert_eq!(s.kind, SummaryKind::Partial);
        assert!(s.restart && s.unverified);
        assert_eq!(s.not_done[0].1, REASON_CHANGED);

        assert_eq!(
            summarize(None, Err("boom"), Ok(&verified)).kind,
            SummaryKind::Failed
        );
        assert_eq!(
            summarize(None, Ok(&Report::default()), Ok(&verified)).kind,
            SummaryKind::Failed
        );
    }

    #[test]
    fn known_raw_failures_get_a_friendly_fix() {
        assert!(plain_failure("Another Secblitz operation holds the journal lock")
            .contains("Close it"));
        assert!(plain_failure("Interactive split-token administrator required; service/over-the-shoulder elevation unsupported")
            .contains("Sign in with"));
        assert!(plain_failure("Journal exceeds size limit").contains("Restart your PC"));
    }

    #[test]
    fn unknown_failures_never_echo_raw_text() {
        let raw = "HRESULT 0x80070005 at C:\\secret\\path";
        let msg = plain_failure(raw);
        assert_eq!(msg, FAILURE_GENERAL);
        assert!(!msg.contains("HRESULT") && !msg.contains("secret"));
    }

    #[test]
    fn detail_lines_never_use_raw_detail() {
        let a = advice::for_control("uac.enabled", "error", "boom 0xdead");
        let (st, next) = plain_detail("error", &a);
        assert_eq!(st, "Couldn't check");
        assert!(!next.contains("0xdead"));
        let (st, next) = plain_detail("weird", &a);
        assert_eq!((st, next), ("Not done", NOT_DONE));
    }
}

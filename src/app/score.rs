//! Protection score: protected checks over all checks (findings excluded).
//! OWNER: app-core agent.
//!
//! Same semantics as the former terminal "N of M checks protected" header:
//! every control result counts once; Information-group items are excluded
//! from the total; results that could not be verified count as unknown.
use crate::advice::{self, Group};
use secblitz::engine::{Outcome, Report};
use secblitz::model::Authority;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    pub protected: usize,
    pub total: usize,
    /// Results that need the user's attention (fixable or a choice to review).
    pub attention: usize,
    /// Results that could not be checked.
    pub unknown: usize,
    /// Results this PC's owner (Group Policy, MDM) controls: not protected by
    /// us and not something the person can act on.
    pub managed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Everything protected.
    Protected,
    /// Some things need attention.
    Attention,
    /// The check failed or is incomplete.
    Unknown,
}

/// How one result counts towards the score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Protected,
    /// Can be fixed here.
    Fixable,
    /// Needs the person's own choice or step.
    Review,
    /// Controlled by this PC's owner (Group Policy, MDM): nothing to do here.
    Managed,
    /// Could not be checked right now.
    Unknown,
    /// Not part of the score.
    Excluded,
}

/// Classify a result using stable statuses and typed evidence only.
pub fn classify(r: &Outcome) -> Class {
    let unavailable = r.authority == Some(Authority::Unknown)
        || (r.id.starts_with("firewall.")
            && !secblitz::hardening::is_hardening(&r.id)
            && (r.effective.is_none() || r.authority.is_none()));
    let known = matches!(
        r.status.as_str(),
        "compliant"
            | "ok"
            | "applied"
            | "restored"
            | "unchanged"
            | "attention"
            | "skipped"
            | "conflict"
            | "pending"
            | "info"
            | "review"
    );
    if r.status == "error"
        || unavailable
        || matches!(r.status.as_str(), "unknown" | "unsupported")
        || !known
    {
        return Class::Unknown;
    }
    let a = advice::for_outcome(r);
    if managed(&a) {
        return Class::Managed;
    }
    if a.step == advice::NextStep::CheckAgain {
        // "Check again" means we could not confirm the state.
        return Class::Unknown;
    }
    if a.status == "Left as it is" {
        // Deliberately not edited (an unusual permission layout): nothing for
        // the person to do and never claimed as protected. A note only.
        return Class::Excluded;
    }
    match a.group {
        Group::Protected => Class::Protected,
        Group::Recommended => Class::Fixable,
        Group::Choice => Class::Review,
        Group::Information => Class::Excluded,
    }
}

/// Only positive management evidence. `ReviewWithAdministrator` alone is also
/// the fallback step of ordinary controls, so it is not proof of an owner.
fn managed(a: &advice::Advice) -> bool {
    a.status == "Managed elsewhere"
}

/// Classify a diagnostic finding. `info` findings are "Good to know" notes:
/// `Class::Excluded` means they are never counted as something to check.
#[allow(dead_code)] // consumed by the GUI integration
pub fn classify_finding(f: &secblitz::model::Finding) -> Class {
    match f.status.as_str() {
        "error" | "unknown" | "unsupported" => return Class::Unknown,
        "info" | "compliant" | "ok" | "attention" | "review" | "pending" => {}
        _ => return Class::Unknown,
    }
    let a = advice::for_finding(&f.title, &f.status, &f.detail);
    if managed(&a) {
        return Class::Managed;
    }
    if a.step == advice::NextStep::CheckAgain {
        return Class::Unknown;
    }
    match a.group {
        Group::Protected => Class::Protected,
        Group::Recommended => Class::Fixable,
        Group::Choice => Class::Review,
        Group::Information => Class::Excluded,
    }
}

/// True for notes that are only "Good to know" (never counted, never a problem).
#[allow(dead_code)] // consumed by the GUI integration
pub fn is_good_to_know(f: &secblitz::model::Finding) -> bool {
    classify_finding(f) == Class::Excluded
}

/// One thing the person should look at.
#[derive(Debug, Clone, Copy)]
pub enum ToCheck<'a> {
    Control(&'a Outcome),
    Finding(&'a secblitz::model::Finding),
}

/// True when a core protection is set but the PC says it is not running: our
/// own note after a restart, or the older check of the running state.
pub fn core_not_running(report: &Report, id: &str) -> bool {
    report.findings.iter().any(|f| {
        (secblitz::vbs::finding_control(&f.title) == Some(id)
            && f.title != secblitz::vbs::DEVICE_BLOCKED)
            || (advice::finding_replaced_by(&f.title) == Some(id) && f.status == "attention")
    })
}

/// A control that says "set" while the PC says "not running" is not protected
/// and is not listed as protected: the finding is the row that tells the truth.
pub fn classify_in(report: &Report, r: &Outcome) -> Class {
    match classify(r) {
        Class::Protected
            if secblitz::vbs::is_vbs(&r.id) && core_not_running(report, &r.id) =>
        {
            Class::Excluded
        }
        other => other,
    }
}

/// A finding is hidden when a fix row for the same thing is in the report
/// (the control says it better and can fix it). The older tip about the
/// running state stays when the control says it is set but the PC says it is
/// not running, unless our own note already says so.
pub fn finding_shown(report: &Report, f: &secblitz::model::Finding) -> bool {
    let Some(id) = advice::finding_replaced_by(&f.title) else {
        return true;
    };
    let Some(r) = report.results.iter().find(|r| r.id == id) else {
        return true;
    };
    r.status == "compliant"
        && f.status == "attention"
        && !report
            .findings
            .iter()
            .any(|n| secblitz::vbs::finding_control(&n.title) == Some(id))
}

/// What the person should look at, in report order: control results that need
/// a fix, a choice or a step, then findings that are actual tips. Protected,
/// informational, owner-managed and unverifiable items are never included.
/// Home and Protection both count and list exactly this.
pub fn to_check(report: &Report) -> Vec<ToCheck<'_>> {
    let controls = report
        .results
        .iter()
        .filter(|r| matches!(classify_in(report, r), Class::Fixable | Class::Review))
        .map(ToCheck::Control);
    let findings = report
        .findings
        .iter()
        .filter(|f| finding_shown(report, f))
        .filter(|f| matches!(classify_finding(f), Class::Fixable | Class::Review))
        .map(ToCheck::Finding);
    controls.chain(findings).collect()
}

pub fn to_check_count(report: &Report) -> usize {
    to_check(report).len()
}

/// The verdict shown everywhere (Home, sidebar dot, tray): the control score,
/// except that a tip still to check is never shown as "protected".
pub fn overall(report: &Report) -> Verdict {
    match Score::of(report).verdict() {
        Verdict::Protected if to_check_count(report) > 0 => Verdict::Attention,
        v => v,
    }
}

/// Stable ids of the things to check, for the tray status file: control ids,
/// and `finding.<slug>` for tips (titles are fixed English catalog keys).
pub fn to_check_ids(report: &Report) -> Vec<String> {
    to_check(report)
        .into_iter()
        .map(|item| match item {
            ToCheck::Control(r) => r.id.clone(),
            ToCheck::Finding(f) => {
                let slug: String = f
                    .title
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect();
                format!("finding.{slug}")
            }
        })
        .map(|id| id.chars().take(64).collect())
        .collect()
}

impl Score {
    pub fn of(report: &Report) -> Self {
        let mut s = Score::default();
        for r in &report.results {
            match classify_in(report, r) {
                Class::Protected => s.protected += 1,
                Class::Fixable | Class::Review => s.attention += 1,
                Class::Unknown => s.unknown += 1,
                Class::Managed => s.managed += 1,
                Class::Excluded => continue,
            }
            s.total += 1;
        }
        s
    }
    pub fn verdict(&self) -> Verdict {
        if self.total == 0 {
            Verdict::Unknown
        } else if self.protected == self.total {
            Verdict::Protected
        } else if self.attention > 0 {
            Verdict::Attention
        } else if self.unknown > 0 {
            Verdict::Unknown
        } else {
            // Only owner-managed settings are left: nothing for the person to do.
            Verdict::Protected
        }
    }
    /// 0.0..=1.0 for the ring.
    pub fn ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.protected as f32 / self.total as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secblitz::model::{EffectiveFirewall, InboundAction};

    fn out(id: &str, status: &str) -> Outcome {
        Outcome {
            id: id.into(),
            status: status.into(),
            ..Outcome::default()
        }
    }
    fn rep(results: Vec<Outcome>) -> Report {
        Report {
            results,
            ..Report::default()
        }
    }

    /// Protection lists every fix candidate: counted ones under "Needs your
    /// attention", Excluded ones (privacy extras) in their own group. No
    /// candidate may fall outside those two, or the page and count disagree.
    #[test]
    fn every_fix_candidate_is_counted_or_a_privacy_extra() {
        let report = rep(vec![
            out("uac.enabled", "attention"),
            out("autorun.disabled", "attention"),
            out("privacy.advertising_id", "attention"),
            out("privacy.clipboard_sync", "attention"),
        ]);
        let available: Vec<String> = report.results.iter().map(|r| r.id.clone()).collect();
        let counted = to_check_ids(&report);
        let mut extras = Vec::new();
        for id in crate::app::flow::candidates(&report, &available) {
            let r = report.results.iter().find(|r| r.id == id).unwrap();
            match classify(r) {
                Class::Fixable | Class::Review => assert!(counted.contains(&id), "{id}"),
                Class::Excluded => extras.push(id),
                other => panic!("{id} is a fix candidate classified {other:?}"),
            }
        }
        assert_eq!(counted, ["uac.enabled", "autorun.disabled"]);
        assert_eq!(extras, ["privacy.advertising_id", "privacy.clipboard_sync"]);
    }

    #[test]
    fn empty_report_is_unknown() {
        let s = Score::of(&Report::default());
        assert_eq!((s.protected, s.total), (0, 0));
        assert_eq!(s.verdict(), Verdict::Unknown);
        assert_eq!(s.ratio(), 0.0);
    }

    #[test]
    fn all_protected() {
        let s = Score::of(&rep(vec![
            out("uac.enabled", "compliant"),
            out("uac.consent", "ok"),
        ]));
        assert_eq!((s.protected, s.total), (2, 2));
        assert_eq!(s.verdict(), Verdict::Protected);
        assert_eq!(s.ratio(), 1.0);
    }

    #[test]
    fn attention_and_unknown_count_in_total() {
        let s = Score::of(&rep(vec![
            out("uac.enabled", "compliant"),
            out("uac.consent", "attention"),
            out("defender.ioav", "unknown"),
            out("defender.archive", "error"),
            out("lsa.restrict_anonymous_sam", "mystery"),
        ]));
        assert_eq!((s.protected, s.total, s.attention, s.unknown), (1, 5, 1, 3));
        assert_eq!(s.verdict(), Verdict::Attention);
    }

    #[test]
    fn only_unknown_is_unknown_verdict() {
        let s = Score::of(&rep(vec![
            out("uac.enabled", "compliant"),
            out("uac.consent", "unknown"),
        ]));
        assert_eq!(s.verdict(), Verdict::Unknown);
    }

    #[test]
    fn firewall_without_evidence_is_unknown_not_protected() {
        let s = Score::of(&rep(vec![out("firewall.public.enabled", "compliant")]));
        assert_eq!((s.protected, s.unknown, s.total), (0, 1, 1));
        let verified = Outcome {
            effective: Some(EffectiveFirewall::Inbound(InboundAction::Block)),
            authority: Some(Authority::Local),
            ..out("firewall.public.inbound", "compliant")
        };
        assert_eq!(classify(&verified), Class::Protected);
        let managed = Outcome {
            authority: Some(Authority::Unknown),
            ..verified
        };
        assert_eq!(classify(&managed), Class::Unknown);
    }

    #[test]
    fn informational_findings_are_good_to_know_not_to_check() {
        let find = |title: &str, status: &str| secblitz::model::Finding {
            title: title.into(),
            status: status.into(),
            detail: String::new(),
        };
        let mut r = rep(vec![out("uac.consent", "attention")]);
        for t in ["Windows updates", "Secure Boot", "Local accounts", "SMB1"] {
            r.findings.push(find(t, "info"));
        }
        r.findings.push(find("Remote Desktop", "attention"));
        assert!(is_good_to_know(&r.findings[0]));
        assert!(!is_good_to_know(&r.findings[4]));
        assert_eq!(to_check_count(&r), 2);
        assert_eq!(classify_finding(&find("SMB1", "error")), Class::Unknown);
    }

    #[test]
    fn safe_default_skips_are_protected_in_the_score() {
        let o = Outcome {
            detail: "Preserving absent or already-safe machine preference".into(),
            ..out("wdigest.use_logon_credential", "skipped")
        };
        assert_eq!(classify(&o), Class::Protected);
        let u = Outcome {
            detail: "Preserving absent or nonzero UAC preference".into(),
            ..out("uac.enabled", "skipped")
        };
        assert_eq!(classify(&u), Class::Protected);
    }

    #[test]
    fn a_fix_row_replaces_the_old_tip_for_the_same_thing() {
        let find = |title: &str| secblitz::model::Finding {
            title: title.into(),
            status: "attention".into(),
            detail: String::new(),
        };
        let mut r = rep(vec![out("vbs.memory_integrity", "attention")]);
        r.findings.push(find("Memory integrity"));
        r.findings.push(find("Memory integrity not running"));
        assert!(!finding_shown(&r, &r.findings[0]));
        assert!(finding_shown(&r, &r.findings[1]));
        // One row for the fix, one for the not-running note: never a duplicate.
        assert_eq!(to_check_count(&r), 2);
        // Without a fix row (an older Windows), the tip stays.
        let mut r = rep(vec![]);
        r.findings.push(find("Memory integrity"));
        assert!(finding_shown(&r, &r.findings[0]));
        assert_eq!(to_check_count(&r), 1);
    }

    #[test]
    fn a_core_protection_that_is_set_but_not_running_is_never_counted_as_protected() {
        let find = |title: &str, status: &str| secblitz::model::Finding {
            title: title.into(),
            status: status.into(),
            detail: String::new(),
        };
        // Set, and the older check says it runs: protected, no tip.
        let mut r = rep(vec![out("vbs.memory_integrity", "compliant")]);
        r.findings.push(find("Memory integrity", "ok"));
        assert_eq!(classify_in(&r, &r.results[0]), Class::Protected);
        assert_eq!(Score::of(&r).protected, 1);
        assert!(!finding_shown(&r, &r.findings[0]) || r.findings[0].status == "ok");
        // Set, but the PC says it is not running: the old tip stays, the
        // control is neither protected nor listed as protected.
        let mut r = rep(vec![out("vbs.memory_integrity", "compliant")]);
        r.findings.push(find("Memory integrity", "attention"));
        assert!(finding_shown(&r, &r.findings[0]));
        assert_eq!(classify_in(&r, &r.results[0]), Class::Excluded);
        assert_eq!(Score::of(&r).protected, 0);
        assert_eq!(to_check_count(&r), 1);
        // Our own note replaces the old tip: one row, never two.
        r.findings
            .push(find("Memory integrity not running", "attention"));
        assert!(!finding_shown(&r, &r.findings[0]));
        assert!(finding_shown(&r, &r.findings[1]));
        assert_eq!(to_check_count(&r), 1);
        // Stack protection has only our own note.
        let mut r = rep(vec![out("vbs.kernel_stack_protection", "compliant")]);
        assert_eq!(classify_in(&r, &r.results[0]), Class::Protected);
        r.findings
            .push(find("Kernel stack protection not running", "attention"));
        assert_eq!(classify_in(&r, &r.results[0]), Class::Excluded);
        // A blocked device while it runs does not make it "not running".
        let mut r = rep(vec![out("vbs.memory_integrity", "compliant")]);
        r.findings.push(find("A device may not be working", "attention"));
        assert_eq!(classify_in(&r, &r.results[0]), Class::Protected);
        assert_eq!(to_check_count(&r), 1);
        // Running already (not offered because it is on) counts as protected.
        let mut on = out("vbs.memory_integrity", "skipped");
        on.detail = secblitz::vbs::ALREADY_ON.into();
        assert_eq!(classify(&on), Class::Protected);
    }

    #[test]
    fn findings_never_count() {
        let mut r = rep(vec![out("uac.enabled", "compliant")]);
        r.findings.push(secblitz::model::Finding {
            title: "SMB1".into(),
            status: "attention".into(),
            detail: String::new(),
        });
        assert_eq!(Score::of(&r).total, 1);
    }

    #[test]
    fn managed_is_apart_and_applied_restart_is_review() {
        let managed = Outcome {
            detail: "Domain-managed machine: assessment only".into(),
            ..out("uac.enabled", "skipped")
        };
        assert_eq!(classify(&managed), Class::Managed);
        let again = || Outcome {
            detail: "Domain-managed machine: assessment only".into(),
            ..out("uac.enabled", "skipped")
        };
        let s = Score::of(&rep(vec![out("uac.consent", "compliant"), again()]));
        assert_eq!((s.attention, s.managed, s.total), (0, 1, 2));
        assert_eq!(s.verdict(), Verdict::Protected);
        assert_eq!(to_check_count(&rep(vec![again()])), 0);
        let restart = Outcome {
            detail: "Preference applied; restart required".into(),
            ..out("uac.enabled", "applied")
        };
        assert_eq!(classify(&restart), Class::Review);
        let kept = Outcome {
            detail: "Service permissions preserved: deny ACE".into(),
            ..out("permissions.service.bits", "skipped")
        };
        assert_eq!(classify(&kept), Class::Excluded);
    }
}

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
    /// Needs the person's own choice (or their organization's).
    Review,
    /// Could not be checked right now.
    Unknown,
    /// Not part of the score.
    Excluded,
}

/// Classify a result using stable statuses and typed evidence only.
pub fn classify(r: &Outcome) -> Class {
    let unavailable = r.authority == Some(Authority::Unknown)
        || (r.id.starts_with("firewall.") && (r.effective.is_none() || r.authority.is_none()));
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
    if r.status == "error" || unavailable || matches!(r.status.as_str(), "unknown" | "unsupported") || !known
    {
        return Class::Unknown;
    }
    match advice::for_outcome(r).group {
        Group::Protected => Class::Protected,
        Group::Recommended => Class::Fixable,
        Group::Choice => Class::Review,
        Group::Information => Class::Excluded,
    }
}

impl Score {
    pub fn of(report: &Report) -> Self {
        let mut s = Score::default();
        for r in &report.results {
            match classify(r) {
                Class::Protected => s.protected += 1,
                Class::Fixable | Class::Review => s.attention += 1,
                Class::Unknown => s.unknown += 1,
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
        } else {
            Verdict::Unknown
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

    #[test]
    fn empty_report_is_unknown() {
        let s = Score::of(&Report::default());
        assert_eq!((s.protected, s.total), (0, 0));
        assert_eq!(s.verdict(), Verdict::Unknown);
        assert_eq!(s.ratio(), 0.0);
    }

    #[test]
    fn all_protected() {
        let s = Score::of(&rep(vec![out("uac.enabled", "compliant"), out("uac.consent", "ok")]));
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
    fn managed_is_review_and_applied_restart_is_review() {
        let managed = Outcome {
            detail: "Domain-managed machine: assessment only".into(),
            ..out("uac.enabled", "skipped")
        };
        assert_eq!(classify(&managed), Class::Review);
        let restart = Outcome {
            detail: "Preference applied; restart required".into(),
            ..out("uac.enabled", "applied")
        };
        assert_eq!(classify(&restart), Class::Review);
    }
}

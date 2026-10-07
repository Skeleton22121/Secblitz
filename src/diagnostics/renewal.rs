//! Whether the Secure Boot certificate renewal can be offered, and why not.
use super::types::SecureBootCerts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renewal {
    NotApplicable,
    Done,
    VirtualPc,
    Started,
    Offer { bitlocker: bool },
    Blocked(Blocker),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    MakerUpdate,
    TaskOff,
    OtherSystem,
    NotChecked,
}

impl Renewal {
    pub fn of(v: &SecureBootCerts) -> Self {
        if v.secure_boot_enabled.known() == Some(&false) {
            return Self::NotApplicable;
        }
        let status = v.servicing_status.known().map(String::as_str);
        let seen = |r: &super::types::Reading<bool>| r.known() == Some(&true);
        let done =
            seen(&v.update_completed_event) || status == Some("Updated") || seen(&v.ca2023_in_db);
        if done {
            return Self::Done;
        }
        let pending = seen(&v.update_error_event)
            || matches!(status, Some("NotStarted" | "InProgress"))
            || (v.update_completed_event.known() == Some(&false)
                && v.ca2023_in_db.known() == Some(&false));
        if !pending {
            return Self::Unknown;
        }
        if seen(&v.is_vm) {
            return Self::VirtualPc;
        }
        let available = v.available_updates.known().copied();
        if available.is_some_and(|n| n != 0) || status == Some("InProgress") {
            return Self::Started;
        }
        let task = v.task_state.known().map(String::as_str);
        if seen(&v.maker_blocked_event) {
            return Self::Blocked(Blocker::MakerUpdate);
        }
        if matches!(task, Some(t) if t != "Ready") {
            return Self::Blocked(Blocker::TaskOff);
        }
        if seen(&v.other_os) {
            return Self::Blocked(Blocker::OtherSystem);
        }
        let ready = status.is_some()
            && available == Some(0)
            && task == Some("Ready")
            && v.is_vm.known() == Some(&false)
            && v.other_os.known() == Some(&false)
            && v.maker_blocked_event.known() == Some(&false);
        if !ready {
            return Self::Blocked(Blocker::NotChecked);
        }
        Self::Offer {
            bitlocker: v.bitlocker_on.known() != Some(&false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::types::{Reading, UnknownReason};

    fn known<T>(value: T) -> Reading<T> {
        Reading::Known(value)
    }

    fn start() -> SecureBootCerts {
        SecureBootCerts {
            update_completed_event: known(false),
            update_staged_event: known(true),
            update_error_event: known(false),
            servicing_status: known("NotStarted".into()),
            ca2023_in_db: known(false),
            secure_boot_enabled: known(true),
            maker_blocked_event: known(false),
            available_updates: known(0),
            servicing_error: known(0),
            capable: known(0),
            task_state: known("Ready".into()),
            is_vm: known(false),
            bitlocker_on: known(false),
            other_os: known(false),
        }
    }

    type Change = fn(&mut SecureBootCerts);

    const OFFER: Renewal = Renewal::Offer { bitlocker: false };

    #[test]
    fn every_condition_flips_the_offer() {
        assert_eq!(Renewal::of(&start()), OFFER);
        let flips: [(&str, Change, Renewal); 12] = [
            (
                "secure boot off",
                |v| v.secure_boot_enabled = known(false),
                Renewal::NotApplicable,
            ),
            (
                "already updated",
                |v| v.servicing_status = known("Updated".into()),
                Renewal::Done,
            ),
            (
                "done event",
                |v| v.update_completed_event = known(true),
                Renewal::Done,
            ),
            (
                "certificate in db",
                |v| v.ca2023_in_db = known(true),
                Renewal::Done,
            ),
            ("virtual pc", |v| v.is_vm = known(true), Renewal::VirtualPc),
            (
                "value already set",
                |v| v.available_updates = known(0x5944),
                Renewal::Started,
            ),
            (
                "value stepping down",
                |v| v.available_updates = known(0x4100),
                Renewal::Started,
            ),
            (
                "status in progress",
                |v| v.servicing_status = known("InProgress".into()),
                Renewal::Started,
            ),
            (
                "maker event",
                |v| v.maker_blocked_event = known(true),
                Renewal::Blocked(Blocker::MakerUpdate),
            ),
            (
                "task disabled",
                |v| v.task_state = known("Disabled".into()),
                Renewal::Blocked(Blocker::TaskOff),
            ),
            (
                "task missing",
                |v| v.task_state = known("Missing".into()),
                Renewal::Blocked(Blocker::TaskOff),
            ),
            (
                "other system",
                |v| v.other_os = known(true),
                Renewal::Blocked(Blocker::OtherSystem),
            ),
        ];
        for (name, change, expected) in flips {
            let mut v = start();
            change(&mut v);
            assert_eq!(Renewal::of(&v), expected, "{name}");
        }
    }

    #[test]
    fn a_fact_that_could_not_be_read_never_offers_the_renewal() {
        let reads: [(&str, Change); 5] = [
            ("value", |v| {
                v.available_updates = Reading::Unknown(UnknownReason::Unavailable)
            }),
            ("task", |v| {
                v.task_state = Reading::Unknown(UnknownReason::Unavailable)
            }),
            ("virtual pc", |v| {
                v.is_vm = Reading::Unknown(UnknownReason::Unavailable)
            }),
            ("other system", |v| {
                v.other_os = Reading::Unknown(UnknownReason::Unavailable)
            }),
            ("maker event", |v| {
                v.maker_blocked_event = Reading::Unknown(UnknownReason::Unavailable)
            }),
        ];
        for (name, change) in reads {
            let mut v = start();
            change(&mut v);
            assert_eq!(
                Renewal::of(&v),
                Renewal::Blocked(Blocker::NotChecked),
                "{name}"
            );
        }
        let mut v = start();
        v.servicing_status = Reading::Unknown(UnknownReason::Unavailable);
        v.update_completed_event = Reading::Unknown(UnknownReason::Unavailable);
        assert_eq!(Renewal::of(&v), Renewal::Unknown);
    }

    #[test]
    fn the_recovery_key_reminder_shows_unless_bitlocker_is_known_off() {
        let mut v = start();
        v.bitlocker_on = known(true);
        assert_eq!(Renewal::of(&v), Renewal::Offer { bitlocker: true });
        v.bitlocker_on = Reading::Unknown(UnknownReason::Unavailable);
        assert_eq!(Renewal::of(&v), Renewal::Offer { bitlocker: true });
    }
}

//! The app-side jobs behind the Tools page: repair, Windows updates, PC health tips and the password generator.
mod errors;
mod password;
mod repair;
mod tips;
mod updates;

pub use errors::*;
pub use password::*;
pub use repair::*;
pub use tips::*;
pub use updates::*;

#[cfg(test)]
mod tests {
    use super::*;
    use super::errors::assert_no_dev_terms;
    use secblitz::diagnostics as diag;
    use secblitz::operations::OperationKind as Op;

    #[test]
    fn primary_text_has_no_developer_terms() {
        for kind in [
            Op::DismCheckHealth,
            Op::DismScanHealth,
            Op::DismRestoreHealth,
            Op::SfcVerify,
            Op::SfcRepair,
            Op::DefenderQuickScan,
        ] {
            assert_no_dev_terms(step_label(kind));
        }
        for r in [
            RepairResult::NoProblems,
            RepairResult::ProblemsFound,
            RepairResult::Repaired,
            RepairResult::NeedsRestart,
            RepairResult::Stopped,
            RepairResult::CouldNotFinish,
        ] {
            assert_no_dev_terms(r.title());
            assert_no_dev_terms(r.detail());
        }
        for r in [
            InstallResult::Installed,
            InstallResult::NeedsRestart,
            InstallResult::NotConfirmed,
            InstallResult::Stopped,
            InstallResult::CouldNotFinish,
        ] {
            assert_no_dev_terms(r.title());
            assert_no_dev_terms(r.detail());
        }
        for s in [
            InstallStage::Preparing,
            InstallStage::Installing,
            InstallStage::Checking,
        ] {
            assert_no_dev_terms(s.label());
        }
        for p in TipProfile::ALL {
            assert_no_dev_terms(p.title());
            assert_no_dev_terms(p.blurb());
        }
        for id in diag::ProbeId::ALL {
            assert_no_dev_terms(tip_title(*id));
            assert_no_dev_terms(tip_advice(*id));
        }
    }

    #[test]
    fn password_and_repair_calls_fail_cleanly_off_windows() {
        #[cfg(not(windows))]
        {
            use std::sync::atomic::AtomicBool;
            use std::sync::Arc;
            let seen = std::sync::Mutex::new(Vec::new());
            run_repair(RepairKind::Check, Arc::new(AtomicBool::new(false)), &|e| {
                seen.lock().unwrap().push(e);
            });
            let events = seen.into_inner().unwrap();
            assert!(matches!(
                events.last(),
                Some(RepairEvent::Done {
                    result: RepairResult::CouldNotFinish,
                    note: Some(_),
                    ..
                })
            ));
            assert!(discover_updates().is_err());
        }
    }
}

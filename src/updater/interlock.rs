//! Entry gates, separate from the leaf durable-state inspectors. A recovery
//! route may inspect its own pending state; it never bypasses another subsystem.
use anyhow::Result;
use std::fs::File;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Activity {
    Hardening,
    Operations,
    Patching,
    Updater,
}

pub(crate) fn check(
    caller: Activity,
    held: &File,
    mut inspect: impl FnMut(Activity, &File) -> Result<()>,
) -> Result<()> {
    for subsystem in [Activity::Updater, Activity::Operations, Activity::Patching] {
        if subsystem != caller {
            inspect(subsystem, held)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn ensure_others_idle(caller: Activity, held: &File) -> Result<()> {
    check(caller, held, |subsystem, held| match subsystem {
        Activity::Updater => super::ensure_install_idle(held),
        Activity::Operations => crate::operations::ensure_update_idle(held),
        Activity::Patching => crate::patching::ensure_idle(held),
        Activity::Hardening => unreachable!("hardening has no asynchronous supervisor"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_intents_veto_every_entry_but_own_recovery_does_not_self_block() {
        let file = tempfile::tempfile().unwrap();
        fs2::FileExt::try_lock_exclusive(&file).unwrap();
        for caller in [
            Activity::Hardening,
            Activity::Operations,
            Activity::Patching,
            Activity::Updater,
        ] {
            for uncertain in [Activity::Operations, Activity::Patching, Activity::Updater] {
                let mut inspected = Vec::new();
                let result = check(caller, &file, |subsystem, held| {
                    assert!(std::ptr::eq(held, &file), "must pass the already-held lock");
                    assert_ne!(caller, subsystem, "own recovery must remain reachable");
                    inspected.push(subsystem);
                    anyhow::ensure!(subsystem != uncertain, "durable uncertainty veto");
                    Ok(())
                });
                assert_eq!(
                    result.is_ok(),
                    caller == uncertain,
                    "{caller:?}/{uncertain:?}"
                );
                if result.is_err() {
                    assert_eq!(inspected.last(), Some(&uncertain));
                } else {
                    assert_eq!(inspected.len(), 2);
                }
            }
        }
    }
}

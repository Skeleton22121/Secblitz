//! What Secblitz does when it opens: show the welcome, check at once, or wait.
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launch {
    pub welcome: bool,
    pub check: bool,
}

/// The welcome is for a fresh install only. A check starts on its own only for
/// someone who has checked before; everyone else is asked first.
pub fn launch(used_before: bool, welcome_seen: bool, has_history: bool, cached: bool) -> Launch {
    let welcome = !welcome_seen && !used_before;
    Launch {
        welcome,
        check: !welcome && has_history && !cached,
    }
}

/// A check was run on this PC before: a history entry or a saved result.
pub fn has_history(dir: &Path) -> bool {
    dir.join(super::last_check::FILE).exists() || !super::history::load(dir).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_shows_the_welcome_and_does_not_check() {
        assert_eq!(
            launch(false, false, false, false),
            Launch {
                welcome: true,
                check: false
            }
        );
    }

    #[test]
    fn someone_who_checked_before_is_checked_again_without_a_welcome() {
        assert_eq!(
            launch(true, false, true, false),
            Launch {
                welcome: false,
                check: true
            }
        );
        assert_eq!(
            launch(true, true, true, false),
            Launch {
                welcome: false,
                check: true
            }
        );
    }

    #[test]
    fn a_recent_saved_result_is_shown_instead_of_checking() {
        assert!(!launch(true, true, true, true).check);
    }

    #[test]
    fn an_upgrade_without_history_waits_to_be_asked_and_has_no_welcome() {
        assert_eq!(
            launch(true, false, false, false),
            Launch {
                welcome: false,
                check: false
            }
        );
    }

    #[test]
    fn the_welcome_is_never_shown_twice() {
        assert!(!launch(false, true, false, false).welcome);
    }

    #[test]
    fn history_means_an_entry_or_a_saved_result() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_history(dir.path()));
        std::fs::write(dir.path().join(crate::app::history::FILE), "").unwrap();
        assert!(!has_history(dir.path()));
        std::fs::write(dir.path().join(crate::app::last_check::FILE), "{}").unwrap();
        assert!(has_history(dir.path()));
    }
}

//! Plain-words explanations for the raw reasons the app clean-up records.
//!
//! The raw text (Windows messages, error chains) stays in the journal and the
//! logs. Only what this module returns is ever shown on screen, and an
//! unknown reason never reaches the screen as it is.

/// Why one app could not be removed.
pub fn removal_failure(raw: &str) -> &'static str {
    let r = raw.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| r.contains(w));
    if has(&["still installed"]) {
        "Windows removed this app for some accounts but not all. Restart your PC and try again."
    } else if has(&["took too long"]) {
        "Windows took too long to answer. Restart your PC and try again."
    } else if has(&["access is denied", "0x80070005", "administrator", "permission"]) {
        "Windows did not allow the change. Sign in with an account that can make changes to this PC, then open Secblitz again."
    } else if has(&[
        "in use",
        "0x80073d02",
        "0x80073d01",
        "currently running",
        "being used",
    ]) {
        "The app was open or in use. Close it, then try again."
    } else if has(&["0x80073cf", "another", "pending", "reboot"]) {
        "Windows is busy with another change. Restart your PC and try again."
    } else if has(&["not enough space", "0x80070070", "disk full"]) {
        "There is not enough free space on this PC. Free up some space, then try again."
    } else {
        "Windows couldn't remove this app. Restart your PC and try again. If it still fails, check for a Secblitz update."
    }
}

/// Why a safety copy could not be saved, so the app was left in place.
pub fn no_copy(raw: &str) -> &'static str {
    let r = raw.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| r.contains(w));
    if has(&["not enough space", "disk full", "0x80070070", "no space"]) {
        "There is not enough free space to save a copy. Free up some space, then try again."
    } else if has(&["access is denied", "0x80070005", "permission"]) {
        "Windows did not let Secblitz save a copy. Sign in with an account that can make changes to this PC, then open Secblitz again."
    } else if has(&["encryption", "damaged saved data key"]) {
        "Secblitz couldn't protect the saved copy on this PC. Restart your PC and try again."
    } else if has(&["took too long"]) {
        "Windows took too long to answer. Restart your PC and try again."
    } else {
        "Secblitz couldn't save a copy first, so the app was left in place. Restart your PC and try again. If it still fails, check for a Secblitz update."
    }
}

/// Why the whole list of apps could not be read, or a whole run failed.
pub fn run_failure(raw: &str) -> &'static str {
    let r = raw.to_lowercase();
    if r.contains("took too long") {
        "Windows took too long to answer. Restart your PC and try again."
    } else if r.contains("only available on windows") || r.contains("only available") {
        "This works on Windows only."
    } else {
        "Windows didn't give Secblitz the list of apps. Restart your PC and try again. If it still fails, check for a Secblitz update."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_reasons_get_a_fix() {
        assert!(removal_failure("The app is still installed for at least one account.")
            .contains("Restart your PC"));
        assert!(removal_failure("Access is denied. (0x80070005)").contains("Sign in"));
        assert!(no_copy("not enough space").contains("Free up"));
        assert!(run_failure("Windows took too long to answer").contains("too long"));
    }

    #[test]
    fn unknown_reasons_never_echo_raw_text() {
        let raw = "Deployment failed with HRESULT: 0x8007ZZZZ, Add-AppxPackage";
        for out in [removal_failure(raw), no_copy(raw), run_failure(raw)] {
            assert!(!out.contains("HRESULT") && !out.contains("Appx"));
            assert!(out.contains("Restart your PC"));
        }
    }

    #[test]
    fn no_message_uses_developer_words() {
        let raws = [
            "Access is denied",
            "PowerShell failed (exit code: 1)",
            "Encryption is unavailable (0xc0000001)",
            "",
        ];
        for raw in raws {
            for out in [removal_failure(raw), no_copy(raw), run_failure(raw)] {
                for bad in ["PowerShell", "HRESULT", "token", "registry", "0x", "\u{2014}"] {
                    assert!(!out.contains(bad), "{out}");
                }
            }
        }
    }
}

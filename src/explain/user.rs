//! Per-user settings (the signed-in person's own account) and app updates.
use super::Explainer;

pub(super) fn get(id: &str) -> Option<Explainer> {
    Some(match id {
        "smartscreen.store_apps" => Explainer {
            what: "Windows can check the web addresses that Store apps open and warn you about unsafe ones.",
            risk: "A Store app could send you to a harmful website and Windows would not warn you.",
            change: "Nothing you'll notice. You may see a warning page now and then when an app opens a risky site.",
        },
        "files.show_extensions" => Explainer {
            what: "Windows can show the ending of a file name, like .pdf or .exe, which tells you what kind of file it is.",
            risk: "A program disguised as \"invoice.pdf\" can really be \"invoice.pdf.exe\", and you would not see the difference.",
            change: "File names in folders will show their ending, like .docx. Nothing else changes.",
        },
        "net.nearby_sharing" => Explainer {
            what: "Nearby sharing lets this PC send and receive files with other devices close to you.",
            risk: "While it is open to everyone nearby, people on a train or in a cafe can try to send you files.",
            change: "Only your own devices can share with this PC. Sending to a friend's PC may need you to switch it back.",
        },
        "privacy.tailored_experiences" => Explainer {
            what: "Windows can use the information it collects about how you use your PC to suggest tips and ads.",
            risk: "Your PC habits are used to personalize suggestions, which many people would rather keep private.",
            change: "You'll see more general tips and ads instead of ones based on how you use your PC. Nothing breaks.",
        },
        "office.internet_macros" => Explainer {
            what: "Office files downloaded from the internet can contain small programs, called macros, that run when you open them.",
            risk: "A document from an unexpected email could run a hidden program the moment you click Enable Content.",
            change: "Macros in files from email or the web stay off. Work files that need macros may need to be unblocked first.",
        },
        "software.outdated_winget" => Explainer {
            what: "Popular programs like your browser, Java and PDF reader get safety fixes from time to time.",
            risk: "An out-of-date browser or PDF reader can be taken over by a harmful web page or file.",
            change: "The program updates itself and may close for a moment. You can't go back to the old version afterwards.",
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: [&str; 6] = [
        "smartscreen.store_apps",
        "files.show_extensions",
        "net.nearby_sharing",
        "privacy.tailored_experiences",
        "office.internet_macros",
        "software.outdated_winget",
    ];

    #[test]
    fn every_user_id_is_explained_in_plain_short_lines() {
        for id in IDS {
            let e = get(id).unwrap_or_else(|| panic!("{id}"));
            for line in [e.what, e.risk, e.change] {
                assert!(!line.is_empty() && line.len() <= 165, "{id}: {line}");
                assert!(!line.contains('!'), "{id}: {line}");
                for jargon in ["HKCU", "registry", "SMB", "NTLM", "CDP", "WinGet", "winget"] {
                    assert!(!line.contains(jargon), "{id}: {line}");
                }
            }
        }
        assert!(get("nope").is_none());
    }

    #[test]
    fn user_ids_resolve_through_the_catalog() {
        for id in IDS {
            assert!(crate::explain::for_check(id).is_some(), "{id}");
        }
    }

    #[test]
    fn app_update_text_says_there_is_no_way_back() {
        let e = get("software.outdated_winget").unwrap();
        assert!(e.change.contains("can't go back"));
    }
}

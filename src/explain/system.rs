//! Explanations for the system area: sign-in leftovers, drivers, memory
//! protections, old tools, remote-access services, updates and privacy.
use super::Explainer;

const fn e(what: &'static str, risk: &'static str, change: &'static str) -> Explainer {
    Explainer { what, risk, change }
}

#[cfg(test)]
const IDS: &[&str] = &[
    "ntlm.extras",
    "driver.vulnerable_blocklist",
    "system.exploit_mitigations",
    "ps.v2_engine",
    "printer.spooler_remote",
    "services.legacy_remote",
    "session.lock_on_wake",
    "update.store_autoupdate_policy",
    "update.paused",
    "smartscreen.apps",
    "privacy.recall",
    "privacy.diagnostic_data_level",
    "privacy.delivery_optimization",
    "privacy.clipboard_sync",
    "ai.click_to_do",
    "ai.paint",
    "ai.notepad",
    "debloat.widgets_policy",
    "debloat.device_companion_apps",
    "defender.exclusions_risky",
    "recovery.winre_enabled",
];

pub(super) fn get(id: &str) -> Option<Explainer> {
    Some(match id {
        "ntlm.extras" => e(
            "Windows can keep a weak, old-style copy of your password and let a background process sign in without a name.",
            "Someone who got hold of that weak copy could work out your password quickly, or reach shared items without signing in.",
            "Nothing you'll notice day to day. Very old network drives or scanners may stop connecting. Needs a restart.",
        ),
        "driver.vulnerable_blocklist" => e(
            "Windows keeps a list of hardware programs, called drivers, that are known to be dangerous and refuses to load them.",
            "Malware can bring along a trusted but flawed driver and use it to switch off your security software.",
            "Nothing you'll notice. A very old hardware tool may stop working if its driver is on the list. Needs a restart.",
        ),
        "system.exploit_mitigations" => e(
            "Windows has built-in protections that make it harder for a flaw in a program to be used to take over your PC.",
            "With one of them switched off, a small program bug is easier to use for taking control of your PC.",
            "Nothing you'll notice. A very old, badly written program may close unexpectedly. Needs a restart.",
        ),
        "ps.v2_engine" => e(
            "An old version of a Windows scripting tool is still installed, kept only so very old programs keep working.",
            "Harmful scripts can start the old version to slip past the checks of the newer one.",
            "Nothing you'll notice. Very old scripts that need that version will stop working. Removing it can take a minute.",
        ),
        "printer.spooler_remote" => e(
            "The part of Windows that handles printing can accept requests from other computers on your network.",
            "Someone on the same network could use printing flaws to take over a PC that does not need to share printers.",
            "You can print as usual. Other computers can no longer print through this PC. Printing restarts for a moment.",
        ),
        "services.legacy_remote" => e(
            "Some old remote-access services are running or set to start by themselves, such as ones for remote control or file transfer.",
            "Anyone on your network who guesses a password could use them to get into your PC from far away.",
            "Nothing you'll notice unless you use them on purpose. Remote tools that rely on them stop until you turn them back on.",
        ),
        "session.lock_on_wake" => e(
            "When your PC wakes from sleep, Windows can ask for your password before it shows anything.",
            "If your PC is lost, or someone walks past while you are away, they could open it and see everything.",
            "You will type your password after sleep. It only takes a moment, but you can no longer skip it.",
        ),
        "update.store_autoupdate_policy" => e(
            "A setting on this PC is stopping Microsoft Store apps from updating themselves.",
            "Apps stay on old versions that have known flaws.",
            "Store apps update by themselves again, usually quietly in the background.",
        ),
        "update.paused" => e(
            "Windows updates are paused, so new security fixes are not being installed.",
            "While updates wait, flaws that are already fixed stay open on your PC.",
            "Windows starts downloading updates again and may ask you to restart afterwards.",
        ),
        "smartscreen.apps" => e(
            "Windows can check downloads and apps and warn you before you run something unknown or risky.",
            "Without the warning, an unrecognized installer or harmful download can start with a single click.",
            "You will sometimes see a warning for unknown programs. You can still choose to run them.",
        ),
        "privacy.recall" => e(
            "Recall takes pictures of your screen every few seconds so you can search for what you did earlier.",
            "Those pictures can show passwords, messages and bank details, and anyone who gets into your account could look through them.",
            "Windows stops saving screen pictures and removes the ones it kept. You can no longer search your past screen.",
        ),
        "privacy.diagnostic_data_level" => e(
            "Windows sends information about how it and your apps run to Microsoft. You can limit it to the basics.",
            "Extra details, such as which apps you use, leave your PC when they are not needed for it to run well.",
            "Windows sends only the basics it needs. Nothing stops working, but some tips may feel less tailored to you.",
        ),
        "privacy.delivery_optimization" => e(
            "Windows can share the updates it has downloaded with other computers, nearby or on the internet.",
            "Your PC may use your internet connection to send files to other people, and other computers can see that it is there.",
            "Updates still download from Microsoft as before. Your PC just stops sharing them with others.",
        ),
        "privacy.clipboard_sync" => e(
            "Windows can copy what you cut or copy on one device to your other devices that use the same account.",
            "Anything you copy, like a password or a card number, can turn up on another device you forgot about.",
            "Copying and pasting works as usual on this PC. It just no longer appears on your other devices.",
        ),
        "ai.click_to_do" => e(
            "Click to Do lets you pick text or pictures on your screen and send them to AI tools for quick actions.",
            "What is on your screen, including private messages or details, can be handed to AI tools when you or a stray click chooses it.",
            "Click to Do stops offering actions on your screen. Nothing else changes. Not every version of Windows has it.",
        ),
        "ai.paint" => e(
            "Paint has AI tools that can make or change pictures for you, such as Cocreator, Generative fill and Image Creator.",
            "Your drawings and what you ask for can be sent over the internet to online AI services.",
            "Those AI tools turn off. Drawing, editing and saving in Paint work as before. You can undo this.",
        ),
        "ai.notepad" => e(
            "Notepad has AI tools that can write, rewrite and sum up text for you.",
            "The words you type or select can be sent over the internet to online AI services.",
            "Those AI tools turn off. Typing, opening and saving notes work as before. You can undo this.",
        ),
        "debloat.widgets_policy" => e(
            "Widgets is a button on the taskbar that opens a board of news, weather, stories and ads.",
            "The board can fill your screen with news and ads you did not ask for, and it uses your internet connection.",
            "The Widgets button and board go away for everyone on this PC. Windows Home does not support this. You can undo this.",
        ),
        "debloat.device_companion_apps" => e(
            "When you plug in a new mouse, keyboard or monitor, Windows can look up its picture and suggest the maker's extra app.",
            "Those suggestions can put extra apps on your PC that you never asked for.",
            "Device pictures and details stop downloading, so devices may show a plain icon. They still work. You can undo this.",
        ),
        "defender.exclusions_risky" => e(
            "Your antivirus keeps a list of places and programs it was told to skip, and some entries cover far too much.",
            "Malware that lands in a skipped place is never scanned, so it can sit there unnoticed.",
            "Those entries are removed, so the places are scanned again. A game or work tool kept there may scan slowly. You can undo this.",
        ),
        "recovery.winre_enabled" => e(
            "Windows has built-in recovery tools that open when Windows can't start, so you can repair it or undo a bad update.",
            "If Windows ever stops starting, there would be no built-in way to repair it, and you might have to reinstall it.",
            "Nothing changes day to day. Windows turns the tools back on from files already on this PC. You can undo this.",
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_system_check_is_explained_in_plain_words() {
        // Words a grandparent would not know; the plain version is used instead.
        let jargon = [
            "NTLM",
            "SMB",
            "DEP",
            "SEHOP",
            "ASLR",
            "DISM",
            "LSA",
            "WinRM",
            "registry",
            "policy",
            "RPC",
            "UAC",
            "DNS",
            "TLS",
            "telemetry",
            "SmartScreen",
            "Defender",
            "PowerShell",
            "Spooler",
            "spooler",
            "!",
        ];
        for id in IDS {
            let x = get(id).unwrap_or_else(|| panic!("no explanation for {id}"));
            for (label, line) in [("what", x.what), ("risk", x.risk), ("change", x.change)] {
                crate::explain::tests::assert_short_sentence(id, label, line);
                assert!(!line.contains("  ") && line.trim() == line, "{id} {label}");
                for word in jargon {
                    assert!(!line.contains(word), "{id} {label} uses '{word}'");
                }
            }
            assert!(!x.risk.to_lowercase().contains("hack"), "{id}");
        }
        assert!(get("not.a.check").is_none());
    }

    #[test]
    fn every_system_engine_control_has_an_explanation() {
        for id in IDS {
            assert!(
                crate::hardening::is_hardening_check_id(id),
                "{id} is not an engine control"
            );
        }
        // Ids shared with the read-only tips are explained once, here.
        for id in ["smartscreen.apps", "ps.v2_engine", "update.paused"] {
            assert!(crate::explain::for_check(id).is_some(), "{id}");
        }
    }
}

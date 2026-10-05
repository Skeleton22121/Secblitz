use super::Explainer;

// Plain-language explanations for the read-only checks (diagnostics rule ids).

pub(super) fn get(id: &str) -> Option<Explainer> {
    Some(match id {
        "os.feature_release_support" => Explainer {
            what: "This checks whether your version of Windows still gets safety updates from Microsoft.",
            risk: "Once a version stops getting updates, new security holes stay open and attackers can use them on your PC.",
            change: "To fix it, install the newest Windows version in Windows Update. It takes a while and needs a restart. Back up first.",
        },
        "boot.secure_boot_certs" => Explainer {
            what: "This checks that your PC's startup security has received the newer certificates from Microsoft.",
            risk: "Old startup certificates expire, and a PC without the new ones may stop getting startup security fixes.",
            change: "Install all Windows updates and check your PC maker's website. This app never changes your PC's startup settings.",
        },
        "defender.tamper_protection" => Explainer {
            what: "Tamper Protection stops harmful programs from switching off your virus protection.",
            risk: "Without it, malware can quietly turn off your antivirus and then do whatever it likes.",
            change: "Turn it on in Windows Security. You won't notice anything, but some tools can no longer change virus settings.",
        },
        "defender.threats" => Explainer {
            what: "This looks for harmful files that Windows Security found and has not finished dealing with.",
            risk: "A harmful file left in place can steal passwords or damage your files.",
            change: "Open Windows Security and follow the steps for each item, or run a quick scan. Nothing is deleted for you.",
        },
        "defender.exclusions_risky" => Explainer {
            what: "Windows Security can be told to skip some places or programs. This checks that the skipped list is not risky.",
            risk: "Harmful programs can hide in a skipped place, because your virus protection never looks there.",
            change: "Look at the list in Windows Security and remove entries you don't know. Some games or tools may need a few.",
        },
        "defender.scan_age" => Explainer {
            what: "This checks when your PC was last scanned for viruses.",
            risk: "Without regular scans, a harmful file that slipped in can stay hidden for weeks.",
            change: "Run a quick scan. It takes a few minutes and you can keep using your PC.",
        },
        "smartscreen.apps" => Explainer {
            what: "This is the warning Windows shows before you run an unknown or risky download.",
            risk: "Without it, you could open a fake installer from a website or email with no warning at all.",
            change: "Turn it on in Windows Security. You'll see an extra warning sometimes when you open unfamiliar downloads.",
        },
        "smartscreen.browser_policy" => Explainer {
            what: "This checks that Edge and Chrome can still warn you about dangerous websites.",
            risk: "A browser that doesn't warn you may let you walk into a fake bank or shopping page.",
            change: "Ask whoever set up this PC, since a setting turned the warnings off. Once on, risky sites get a warning page.",
        },
        "smart_app_control.state" => Explainer {
            what: "Smart App Control blocks apps that Microsoft doesn't trust. This just shows whether it is on.",
            risk: "Without it, a brand-new harmful program is less likely to be stopped before it runs.",
            change: "Nothing to do here. Once it is off, Windows can only turn it back on after a fresh install, so this app leaves it alone.",
        },
        "update.paused" => Explainer {
            what: "This checks whether Windows updates are paused.",
            risk: "While updates are paused, known security holes stay open on your PC.",
            change: "Resume updates in Windows Update. Your PC will download updates and may ask for a restart.",
        },
        "update.drivers_excluded" => Explainer {
            what: "This checks whether Windows is told to skip driver updates for your hardware.",
            risk: "Old drivers can leave bugs or security holes in things like Wi-Fi and graphics.",
            change: "Nothing to change if it was on purpose. Some people skip drivers to avoid problems after updates.",
        },
        "update.reboot_overdue" => Explainer {
            what: "This checks whether your PC has been waiting a long time for a restart to finish an update.",
            risk: "Until you restart, the update isn't fully in place, so the hole it fixes may still be open.",
            change: "Restart when it suits you. Save your work first. This app never restarts your PC for you.",
        },
        "ps.v2_engine" => Explainer {
            what: "This is a very old part of Windows' command tool that almost nobody needs anymore.",
            risk: "Attackers like it because it skips newer safety checks, so harmful scripts can run unnoticed.",
            change: "You can remove it in Windows Features. Very old scripts may stop working; normal use isn't affected.",
        },
        "net.hosts_file" => Explainer {
            what: "This is a small file on your PC that can send website names to other places.",
            risk: "A bad entry can send you to a fake bank site, or block your antivirus from updating.",
            change: "Have someone you trust look at it. Resetting it to the Windows default fixes bad entries. This app doesn't edit it.",
        },
        "persistence.wmi_subscriptions" => Explainer {
            what: "This counts hidden triggers that can start programs on your PC without showing up in the usual start-up lists.",
            risk: "Some malware uses these to come back after you remove it, with nothing visible.",
            change: "Ask someone who knows PCs to review them. Some hardware and management tools use them on purpose, so nothing is removed for you.",
        },
        "services.unquoted_paths" => Explainer {
            what: "This looks for background programs that are set up in a way an attacker could trick.",
            risk: "Someone without admin rights could plant a program that Windows then starts with full power.",
            change: "The software maker or a helper can correct it. This app never edits these settings.",
        },
        "accounts.stale_enabled" => Explainer {
            what: "This counts accounts on your PC that are switched on but haven't been used in months.",
            risk: "Nobody notices a forgotten account, so someone could sign in to it and use your PC unseen.",
            change: "Turn off or remove accounts nobody uses, in Settings. Check first that no family member still needs one.",
        },
        "smb.shares_exposed" => Explainer {
            what: "This looks for folders on your PC that other people on your network can open.",
            risk: "On a shared or café network, strangers could read or change files in a folder shared with everyone.",
            change: "Stop sharing folders you don't need. Devices at home or work that use a shared folder will stop seeing it.",
        },
        "smb.server_encryption" => Explainer {
            what: "This shows whether file sharing on your PC scrambles its traffic so others can't read it.",
            risk: "Without it, someone on the same network could read files while they travel to another device.",
            change: "Shown for information only. Turning it on can stop very old devices, such as old scanners, from connecting.",
        },
        "firewall.user_dir_inbound_allow" => Explainer {
            what: "This looks for apps in your Downloads, Desktop or Temp folders that are allowed to receive connections from the internet.",
            risk: "A harmful app in one of those folders could let attackers connect straight to your PC.",
            change: "Remove rules you don't recognise in Windows Firewall. Online games may need some, and may ask again.",
        },
        "accounts.daily_admin" => Explainer {
            what: "This checks whether the account you use every day is an administrator account.",
            risk: "If a harmful program runs under an administrator account, it can change anything on your PC.",
            change: "Make a normal account for daily use and keep this one for installing things. You'll type a password to install apps.",
        },
        "accounts.hello_configured" => Explainer {
            what: "This checks whether you have a PIN or Windows Hello (face or fingerprint) set up to sign in.",
            risk: "Without one you sign in with your long password, which people can watch you type or steal from a website.",
            change: "Add a PIN in Sign-in options. It only works on this PC, and signing in gets faster. Your password still works.",
        },
        "accounts.find_my_device" => Explainer {
            what: "Find my device lets you see where a lost laptop is and lock it.",
            risk: "If your laptop is lost or stolen, you can't find it or lock it, so strangers can try to get at your files.",
            change: "Turn it on in Settings. It shares your laptop's location with your Microsoft account. Only checked on laptops.",
        },
        "vbs.kernel_stack_protection" => Explainer {
            what: "This is an extra shield that protects the core of Windows from a kind of attack that hijacks programs.",
            risk: "Without it, an attacker who finds a bug in a driver has an easier time taking control of Windows.",
            change: "Turn it on in Windows Security under Core isolation, if offered. Some older drivers or games may not work with it.",
        },
        "net.dns_encryption" => Explainer {
            what: "When you open a website, your PC first asks a server where it is. This checks whether that question is private.",
            risk: "Someone on the same café Wi-Fi could see, or change, which sites you are looking up.",
            change: "Choose encrypted lookups in your network settings. Nothing else changes. This app never changes those servers for you.",
        },
        "net.wifi_security" => Explainer {
            what: "This checks how well the Wi-Fi network you're connected to is protected.",
            risk: "On a network with no or weak protection, others nearby can watch what you do or join in.",
            change: "Switch your router to the newest security option, with a strong password. Very old devices may need to reconnect.",
        },
        "persistence.run_and_tasks" => Explainer {
            what: "This counts programs that start by themselves with Windows from risky places and aren't signed by a known maker.",
            risk: "Malware often hides in a folder like Temp or Downloads and starts again every time you turn your PC on.",
            change: "Ask a helper to look at what starts with your PC. Nothing is removed for you, since some tools start this way on purpose.",
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::get;

    const IDS: &[&str] = &[
        "os.feature_release_support",
        "boot.secure_boot_certs",
        "defender.tamper_protection",
        "defender.threats",
        "defender.exclusions_risky",
        "defender.scan_age",
        "smartscreen.apps",
        "smartscreen.browser_policy",
        "smart_app_control.state",
        "update.paused",
        "update.drivers_excluded",
        "update.reboot_overdue",
        "ps.v2_engine",
        "net.hosts_file",
        "persistence.wmi_subscriptions",
        "services.unquoted_paths",
        "accounts.stale_enabled",
        "smb.shares_exposed",
        "smb.server_encryption",
        "firewall.user_dir_inbound_allow",
        "accounts.daily_admin",
        "accounts.hello_configured",
        "accounts.find_my_device",
        "vbs.kernel_stack_protection",
        "net.dns_encryption",
        "net.wifi_security",
        "persistence.run_and_tasks",
    ];

    #[test]
    fn every_read_only_check_is_explained_in_plain_words() {
        for id in IDS {
            let e = get(id).unwrap_or_else(|| panic!("{id}"));
            for line in [e.what, e.risk, e.change] {
                assert!(line.len() <= 160, "{id}: {line}");
                assert!(!line.contains('!'), "{id}");
                for jargon in [
                    "NTLM", "SMB", "DNS", "WMI", "RID", "HVCI", "TKIP", "WEP", "WPA",
                ] {
                    assert!(!line.contains(jargon), "{id}: {jargon}");
                }
            }
        }
        assert!(get("not.a.check").is_none());
    }
}

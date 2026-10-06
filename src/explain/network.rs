use super::Explainer;

pub(super) fn get(id: &str) -> Option<Explainer> {
    Some(match id {
        "defender.asr.office" => Explainer {
            what: "Stops Word, Excel and other Office programs from starting other programs or hiding code inside other apps.",
            risk: "A booby-trapped invoice in your email could use Office to quietly start a program that steals your files.",
            change: "Nothing you'll notice for normal documents. Files that rely on macros or add-ins may stop working until you allow them.",
        },
        "defender.asr.ransomware_usb" => Explainer {
            what: "Makes Windows ask before unknown programs run, including programs that start from a USB stick.",
            risk: "A USB stick someone hands you could start a program that locks your photos and documents and demands money.",
            change: "Windows asks first and you can allow it. A brand-new small program from a friend may get a question the first time.",
        },
        "defender.network_protection" => Explainer {
            what: "Stops programs on your PC from connecting to websites and servers that are known to be harmful.",
            risk: "A program or an ad could quietly connect to a known scam site and download something harmful.",
            change: "Nothing you'll notice most days. A game, VPN or work tool may be blocked by mistake and need to be allowed.",
        },
        "defender.cloud_block_level" => Explainer {
            what: "Makes Defender stricter about files it has never seen before by asking Microsoft's cloud to judge them.",
            risk: "A brand-new harmful file could be let through because nothing had flagged it yet.",
            change: "A download may pause for up to 20 seconds while it's checked. A few harmless new files may be blocked by mistake.",
        },
        "net.stack_hardening" => Explainer {
            what: "Tells your PC to ignore network messages that try to reroute its traffic or release its name.",
            risk: "Someone on the same cafe Wi-Fi could send fake messages that steer your PC's traffic through their device.",
            change: "Nothing you'll notice. It takes effect after a restart.",
        },
        "net.netbios" => Explainer {
            what: "Turns off an old way for computers on a network to find each other by name.",
            risk: "Someone on the same network could answer for another computer, and your PC might send its sign-in details to them.",
            change: "Nothing you'll notice. Very old network drives, printers or scanners may stop being found by name.",
        },
        "net.mdns" => Explainer {
            what: "Stops your PC from asking and answering name questions with other devices on your local network.",
            risk: "Someone on the same network could answer as a printer or TV, and your PC might start talking to them.",
            change: "Casting to a TV, AirPrint and some smart-home devices may stop showing up. It takes effect after a restart.",
        },
        "net.wpad" => Explainer {
            what: "Stops Windows from searching the network for a setup file that tells it how to reach the internet.",
            risk: "Someone on shared Wi-Fi could offer a different setup file and send your web traffic through their device.",
            change: "Nothing you'll notice at home. A work or school network that sets this up automatically may stop working. It takes effect after a restart.",
        },
        "firewall.outbound_smb_internet" => Explainer {
            what: "Adds a firewall rule so this PC never sends file-sharing traffic out to the internet.",
            risk: "A harmful link could make your PC try to log in to a stranger's file server, handing over your sign-in details.",
            change: "Nothing you'll notice. Cloud file shares reached over the internet may stop connecting. Sharing at home still works.",
        },
        "tls.legacy_protocols" => Explainer {
            what: "Turns off old versions of the secure connection Windows uses for websites and apps.",
            risk: "An attacker could force an old, breakable connection and read what you send, like passwords or card numbers.",
            change: "Browsers keep working. Very old apps, accounting software or network drives may fail to connect. It takes effect after a restart.",
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &[&str] = &[
        "defender.asr.office",
        "defender.asr.ransomware_usb",
        "defender.network_protection",
        "defender.cloud_block_level",
        "net.stack_hardening",
        "net.netbios",
        "net.mdns",
        "net.wpad",
        "firewall.outbound_smb_internet",
        "tls.legacy_protocols",
    ];

    #[test]
    fn every_network_control_has_a_calm_plain_explainer() {
        for id in IDS {
            assert!(secblitz::hardening::is_hardening_check_id(id), "{id}");
            let e = get(id).unwrap_or_else(|| panic!("missing explainer for {id}"));
            for line in [e.what, e.risk, e.change] {
                assert!(line.len() <= 160, "{id}: too long: {line}");
                assert!(!line.contains('!'), "{id}: {line}");
                assert!(line.ends_with('.'), "{id}: {line}");
            }
            for jargon in [
                "SMB", "NTLM", "TLS", "SSL", "WPAD", "mDNS", "NetBIOS", "ASR", "LLMNR",
            ] {
                for line in [e.what, e.risk, e.change] {
                    assert!(!line.contains(jargon), "{id} uses jargon {jargon}: {line}");
                }
            }
        }
        assert!(get("defender.pua").is_none());
    }
}

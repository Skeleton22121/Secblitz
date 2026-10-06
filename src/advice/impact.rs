//! What each check or finding protects you from.
pub fn control_impact(id: &str) -> &'static str {
    match id {
        "readiness" => "A full drive stopping fixes and updates from completing",
        "defender.realtime" => "Malware running as soon as it lands on your PC",
        "defender.behavior" => "Apps that behave like malware even when not yet known",
        "defender.ioav" => "Harmful files downloaded from the web or email attachments",
        "defender.archive" => "Malware hidden inside zip and other compressed files",
        "firewall.domain.enabled" => "Other devices on your work network reaching your PC",
        "firewall.private.enabled" => "Other devices on your home network reaching your PC",
        "firewall.public.enabled" => {
            "Other devices at public places like cafes or airports reaching your PC"
        }
        "firewall.domain.inbound" => "Uninvited incoming connections on your work network",
        "firewall.private.inbound" => "Uninvited incoming connections on your home network",
        "firewall.public.inbound" => {
            "Uninvited incoming connections on public networks like cafes or airports"
        }
        "uac.enabled" => "Apps silently making system-wide changes without asking you",
        "uac.consent" => "Apps making big changes without asking for approval",
        "installer.always_install_elevated" => {
            "Any app installer quietly getting full control of your PC"
        }
        "lsa.restrict_anonymous_sam" => {
            "Strangers on the network listing your account names to guess passwords"
        }
        "lsa.limit_blank_password_use" => {
            "Someone signing in over the network to an account with no password"
        }
        "wdigest.use_logon_credential" => "Attackers stealing your Windows password",
        "permissions.service.bits" | "permissions.service.wuauserv" => {
            "Tampered or fake Windows updates reaching your PC"
        }
        "defender.cloud_protection" => "New threats that only cloud checks can spot",
        "defender.pua" => "Junk and adware bundled with free downloads",
        "defender.script_nis" => "Harmful scripts and network attacks reaching your PC",
        "defender.asr.standard" => "Attackers stealing passwords or hiding inside Windows",
        "defender.asr.web_script_email" => {
            "Booby-trapped scripts and email attachments starting programs"
        }
        "lsa.run_as_ppl" => "Password-stealing programs reading your sign-in details",
        "net.public_sharing_exposure" => "Strangers on public Wi-Fi seeing your shared files",
        "printer.point_and_print" => "Fake printer drivers giving attackers full control",
        "net.llmnr" => "Strangers on your network answering lookups with fake replies",
        "accounts.lockout_policy" => "Someone guessing your password again and again",
        "autorun.disabled" => "Harmful programs starting from a USB stick or disc",
        "wifi.risky_profiles" => "Rogue Wi-Fi hotspots connecting your PC without asking",
        "lsa.restrict_anonymous" => "Strangers on the network listing your accounts and shares",
        "remote_assistance.disabled" => "Someone taking over your PC through a help invitation",
        "wsh.disabled" => "Harmful script files starting with a double-click",
        "update.auto_policy_disabled" => "Security fixes never being installed",
        "ntlm.lm_compat_level" => "Old, easily cracked sign-in methods used on your network",
        "accounts.builtin_administrator" => "An unused powerful account being guessed or abused",
        "privacy.activity_history" => "A record of what you did on this PC being kept and shared",
        "privacy.advertising_id" => "Apps tracking you across other apps for ads",
        "defender.asr.office" => "Booby-trapped Office files starting harmful programs",
        "defender.asr.ransomware_usb" => {
            "Ransomware or a harmful USB stick locking or stealing your files"
        }
        "defender.network_protection" => {
            "Programs connecting to known harmful websites and servers"
        }
        "defender.cloud_block_level" => {
            "Brand-new harmful files slipping through before anyone has judged them"
        }
        "net.stack_hardening" => "Strangers on your network steering or spoofing your PC's traffic",
        "net.netbios" => "Strangers on your network learning your PC's name and tricking it",
        "net.mdns" => "Devices on your network answering lookups with fake replies",
        "net.wpad" => "A stranger on your network pointing your PC at a rogue proxy",
        "firewall.outbound_smb_internet" => {
            "Your PC sending sign-in details to a file server on the internet"
        }
        "tls.legacy_protocols" => "Old, breakable secure connections being forced on your PC",
        "ntlm.extras" => "Weak stored copies of your password being cracked",
        "driver.vulnerable_blocklist" => {
            "Attackers using a flawed driver to switch off your security"
        }
        "system.exploit_mitigations" => "A program bug being turned into a full break-in",
        "ps.v2_engine" => "Attackers running harmful scripts through an old back door",
        "printer.spooler_remote" => {
            "Someone on your network using printing flaws to take over your PC"
        }
        "services.legacy_remote" => {
            "Strangers reaching your PC through forgotten remote-access tools"
        }
        "session.lock_on_wake" => "Anyone nearby opening your PC while you are away",
        "update.store_autoupdate_policy" => {
            "Store apps staying out of date and open to known flaws"
        }
        "update.paused" => "Security fixes waiting while known flaws stay open",
        "smartscreen.apps" => "Fake installers and harmful downloads starting with one click",
        "privacy.recall" => "Pictures of your screen, passwords included, being kept on this PC",
        "privacy.diagnostic_data_level" => {
            "More details about how you use your PC leaving it than needed"
        }
        "privacy.delivery_optimization" => {
            "Your PC sending files to strangers over your connection"
        }
        "privacy.clipboard_sync" => "What you copy showing up on your other devices",
        "defender.exclusions_risky" => "Malware hiding in places your antivirus skips",
        "accounts.autologon" => "Anyone who turns on your PC getting straight into your account",
        "remote_desktop.disabled" => "Strangers trying to sign in to your PC from far away",
        "smb1.disabled" => "Old file-sharing flaws that let malware spread between PCs",
        "vbs.memory_integrity" => "Harmful drivers taking over the core of Windows",
        "vbs.kernel_stack_protection" => {
            "Attackers hijacking the core of Windows through a driver bug"
        }
        "services.unquoted_paths" => {
            "A planted program being started with full power instead of the real one"
        }
        "firewall.user_dir_inbound_allow" => {
            "A harmful download letting strangers connect straight to your PC"
        }
        "net.hosts_file" => "Trusted websites quietly sending you to fake ones",
        "persistence.run_and_tasks" => {
            "A harmful program starting again every time you turn on your PC"
        }
        "accounts.stale_enabled" => "Forgotten accounts letting someone sign in unseen",
        "smb.shares_exposed" => "Strangers on your network opening or changing your shared files",
        "smartscreen.browser_policy" => "Scam and virus websites opening with no warning",
        "recovery.winre_enabled" => "Being stuck without a way to repair Windows if it stops starting",
        _ => "",
    }
}

pub fn finding_impact(title: &str) -> &'static str {
    match title {
        "Windows lifecycle" => "Running Windows that no longer gets security fixes",
        "Device encryption" => "Strangers reading your files if your PC is lost or stolen",
        "Secure Boot" => "Hidden malware starting before Windows does",
        "Windows updates" => "Known security holes staying open on your PC",
        "Remote Desktop" => "Strangers trying to sign in to your PC from far away",
        "SMB1" => "Old file-sharing flaws that let malware spread between PCs",
        "SmartScreen" => "Scam websites and unrecognized apps you open by mistake",
        "Local accounts" => "Weak or shared sign-ins that are easier to guess or steal",
        "Memory integrity" | "Memory integrity not running" | "A device may not be working" => {
            "Harmful drivers taking over the core of Windows"
        }
        "Kernel stack protection not running" => {
            "Attackers hijacking the core of Windows through a driver bug"
        }
        "Automatic logon" => "Anyone who turns on your PC getting straight into your account",
        _ => "",
    }
}

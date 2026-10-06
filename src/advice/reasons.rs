//! Why a check is not offered, how to repair it, and who manages it.
/// Calm facts, not faults: they never lower the score.
pub(super) fn not_offered(reason: &str) -> Option<&'static str> {
    use secblitz::vbs as v;
    // The driver reason also names the drivers after a colon; the names are
    // shown in "More details", never matched here.
    if v::is_driver_reason(reason) {
        return Some(
            "A driver on this PC may not work with it, so we leave this alone. To look yourself, open Windows Security, then Device security, then Core isolation details.",
        );
    }
    Some(match reason {
        r if r == v::NOT_SUPPORTED => {
            "This PC doesn't support it, or virtualization is off in its start-up settings, so we leave this alone."
        }
        r if r == v::LOCKED => {
            "It is locked in your PC's start-up settings, so we leave this alone."
        }
        r if r == v::DRIVERS_UNREADABLE => "We couldn't check your drivers, so we leave this alone.",
        r if r == v::NEEDS_MEMORY_INTEGRITY => {
            "Turn on Core system protection first, then check again."
        }
        r if r == v::NEEDS_RESTART => {
            "Restart your PC to finish turning on Core system protection, then check again."
        }
        r if r == v::SET_BY_HAND => {
            "Virtualization security was set up by hand on this PC, so we leave it alone."
        }
        r if r == v::OLD_WINDOWS => {
            "This version of Windows doesn't have it, so there is nothing to turn on."
        }
        r if r == v::NO_SHADOW_STACKS => {
            "This PC's processor doesn't support it, so there is nothing to turn on."
        }
        r if r == v::UNREADABLE => "We couldn't check this PC's support for it, so we leave it alone.",
        "Not offered: Secure Boot is off" => {
            "This protection needs Secure Boot, which is off on this PC. We leave it alone."
        }
        "Not offered: Smart App Control is on" => {
            "Smart App Control already guards this part of Windows, so we leave it alone."
        }
        "Not offered: some sign-in add-ons would stop working" => {
            "Some sign-in add-ons would stop working, so we leave this alone."
        }
        "Not offered: sign-in add-ons from other companies are installed" => {
            "Sign-in add-ons from other companies are installed, so we leave this alone."
        }
        "Not offered: no other administrator account is enabled"
        | "Not offered: no other administrator account could be confirmed" => {
            "This may be the only administrator account, so we keep it switched on."
        }
        "Not offered: Defender real-time protection is off" => {
            "Turn on live virus protection first, then check again."
        }
        "Not offered: Defender cloud protection is off" => {
            "Turn on cloud threat lookups first, then check again."
        }
        "Not offered: this PC uses Configuration Manager" => {
            "Your organization's tools manage this, so we leave it alone."
        }
        "Not offered: Microsoft Office was not found" => {
            "Microsoft Office isn't installed here, so there is nothing to protect."
        }
        "Not offered: this edition of Windows does not include it" => {
            "This version of Windows doesn't include this protection."
        }
        "Not offered: Windows Home cannot accept Remote Desktop connections" => {
            "Windows Home can't accept Remote Desktop connections, so there is nothing to turn off."
        }
        "Not offered: this PC is set up as a kiosk" => {
            "This PC is set up as a kiosk that signs in by itself, so we leave this alone."
        }
        "Not offered: Defender behavior monitoring is off" => {
            "Turn on suspicious app detection first, then check again."
        }
        "Not offered: the old file-sharing version could not be checked" => {
            "We couldn't check something this depends on, so we leave it alone."
        }
        "Not offered: the old file-sharing version (SMB1) is still on" => {
            "Old file sharing is still on here, so we leave this alone."
        }
        "Not offered: a shared folder or drive may rely on the old name service" => {
            "A shared folder or drive here may need this, so we leave it alone."
        }
        "Not offered: Recall is not available on this PC" => {
            "Recall is not on this PC, so there is nothing to change."
        }
        "Not offered: this setting is not available on Windows Home" => {
            "Windows Home does not support this setting, so we leave it alone."
        }
        "Not offered: a printer on this PC is shared with other computers" => {
            "A printer on this PC is shared with others, so we leave this alone."
        }
        "Not offered: printing is busy right now" => {
            "Something is waiting to print, so we leave this for now. Try again when printing is finished."
        }
        "Not offered: you are connected to this PC from another device right now" => {
            "You are connected from another device, so we leave this alone. Turning it off would cut you off."
        }
        "Not offered: something is using the old file sharing right now" => {
            "Something is using the old file sharing right now, so we leave this alone."
        }
        "Not offered: your account has no password" => {
            "Give your account a password first, then check again."
        }
        "Not offered: Secblitz cannot tell who is signed in" => {
            "We could not tell which account is signed in, so we leave this alone."
        }
        "Not offered: a locked sign-in would stay locked until an administrator unlocks it" => {
            "A locked account here would stay locked until an administrator unlocks it, so we leave this alone."
        }
        "Not offered: the hosts file could not be found" => {
            "The hosts file is missing, so there is nothing for us to change."
        }
        "Not offered: the hosts file is too large to change safely" => {
            "The hosts file is too big to change safely, so we leave it alone."
        }
        "Not offered: the hosts file uses a format we cannot keep exactly" => {
            "The hosts file is saved in a format we can't keep exactly, so we leave it alone."
        }
        "Not offered: too many items to switch off safely at once" => {
            "There are too many to switch off safely at once, so we leave this alone."
        }
        "Not offered: a shared folder would be left with no one who can open it" => {
            "A shared folder would be left that nobody can open, so we leave this alone."
        }
        "Not offered: a shared folder would be left that only administrators can open" => {
            "A shared folder would be left that only administrators can open, so we leave this alone."
        }
        "Not offered: a shared folder has permissions that could not be put back exactly" => {
            "A shared folder has permissions we could not put back exactly, so we leave this alone."
        }
        "Not offered: the recovery tools are missing from this PC" => {
            "The files the recovery tools need are missing from this PC, so we leave this alone."
        }
        _ => return None,
    })
}

pub(super) fn repair_help(id: &str) -> &'static str {
    match id {
        "firewall.domain.enabled"
        | "firewall.private.enabled"
        | "firewall.public.enabled"
        | "firewall.domain.inbound"
        | "firewall.private.inbound"
        | "firewall.public.inbound" => {
            "We can fix this. It stops uninvited connections to your PC."
        }
        "permissions.service.bits" | "permissions.service.wuauserv" => {
            "We can fix this. It keeps Windows updates from being tampered with."
        }
        "wdigest.use_logon_credential" => {
            "We can fix this. Your password will no longer be kept where other programs can read it."
        }
        "uac.enabled" => "We can fix this. Windows will ask before big changes are made.",
        "uac.consent" => "We can fix this. Windows will ask for approval before big changes.",
        "installer.always_install_elevated" => {
            "We can fix this. App installers will no longer get full control of your PC."
        }
        "lsa.restrict_anonymous_sam" => {
            "We can fix this. Other devices on the network will no longer see your account names."
        }
        "lsa.limit_blank_password_use" => {
            "We can fix this. Accounts without a password can no longer be used over the network."
        }
        "defender.pua" => "We can fix this. Junk apps bundled with downloads will be blocked.",
        "defender.script_nis" => "We can fix this. It turns scanning for harmful scripts back on.",
        "defender.asr.standard" => {
            "We can fix this. It blocks common ways harmful programs read passwords."
        }
        "net.public_sharing_exposure" => {
            "We can fix this. Your shared files and printers stay hidden on public Wi-Fi."
        }
        "printer.point_and_print" => {
            "We can fix this. Printer drivers will only be installed with your permission."
        }
        "net.llmnr" => "We can fix this. Wrong name-lookup answers will be ignored.",
        "accounts.lockout_policy" => {
            "We can fix this. Too many wrong passwords will lock sign-in for a few minutes."
        }
        "system.exploit_mitigations" => {
            "We can fix this. It switches Windows' built-in memory protections back on."
        }
        "recovery.winre_enabled" => {
            "We can fix this. It turns the recovery tools back on, so Windows can repair itself if it stops starting."
        }
        _ => "We can fix this. It turns this protection on.",
    }
}

pub(super) fn managed(reason: &str) -> bool {
    // Exact backend reason keys only; an unknown authority is not management.
    let reason = reason
        .strip_prefix("Service permissions preserved: ")
        .unwrap_or(reason);
    matches!(
        reason,
        "Domain-managed machine: assessment only"
            | "Device is registered with MDM: assessment only"
            | "Applied computer Group Policy: assessment only"
            | "Relevant resultant Group Policy: assessment only"
            | "Firewall profile has resultant Group Policy: assessment only"
            | "Configured management/security policy: assessment only"
            | "Relevant policy is configured: assessment only"
            | "Other machine Installer policy is configured: assessment only"
            | "Applied computer policy settings: service permissions are assessment only"
    )
}

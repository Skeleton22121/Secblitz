//! Presentation-only advice. These are translation source keys, never commands
//! or mutation authority. Only a control outcome of `attention` offers a fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextStep {
    None,
    Repair,
    OpenWindowsSecurity,
    OpenWindowsUpdate,
    OpenEncryption,
    OpenAccounts,
    OpenRemoteDesktop,
    ReviewFirmware,
    ReviewWindowsFeatures,
    ReviewWithAdministrator,
    ReviewUndo,
    Restart,
    CheckAgain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Recommended,
    Protected,
    Choice,
    Information,
}

pub struct Advice {
    pub label: &'static str,
    pub status: &'static str,
    pub next: &'static str,
    pub step: NextStep,
    pub group: Group,
    /// Translation source key: a noun phrase naming the concrete threat this
    /// check guards against. Empty string means no impact line is rendered.
    pub impact: &'static str,
    /// True for a choice the person makes: never pre-selected, always shown
    /// with its one-line consequence (`next`) before anything is changed.
    pub ask: bool,
}

impl Advice {
    /// Returns the translation source key for the prefix rendered before the
    /// impact phrase. Empty when `impact` is empty.
    pub fn impact_prefix(&self) -> &'static str {
        if self.impact.is_empty() {
            return "";
        }
        match self.group {
            Group::Recommended => "Leaves you open to:",
            Group::Protected => "Protects you from:",
            _ => "Why it matters:",
        }
    }
}

/// Returns the translation source key for the concrete threat this control
/// guards against, or "" for aggregate / fallback / unrecognized ids.
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
        _ => "",
    }
}

/// Returns the translation source key for the concrete threat the finding
/// title guards against, or "" for informational / audit / unknown titles.
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
        "Memory integrity" => "Harmful drivers taking over the core of Windows",
        "Automatic logon" => "Anyone who turns on your PC getting straight into your account",
        _ => "",
    }
}

pub fn control_label(id: &str) -> &'static str {
    match id {
        "readiness" => "Device check",
        "defender.realtime" => "Live virus protection",
        "defender.behavior" => "Suspicious app detection",
        "defender.ioav" => "Downloaded file checks",
        "defender.archive" => "Compressed file checks",
        "firewall.domain.enabled" => "Work network firewall",
        "firewall.private.enabled" => "Home network firewall",
        "firewall.public.enabled" => "Public network firewall",
        "firewall.domain.inbound" => "Work network incoming connections",
        "firewall.private.inbound" => "Home network incoming connections",
        "firewall.public.inbound" => "Public network incoming connections",
        "uac.enabled" => "Permission prompts",
        "uac.consent" => "Administrator approval",
        "installer.always_install_elevated" => "App installation permissions",
        "lsa.restrict_anonymous_sam" => "Account name privacy",
        "lsa.limit_blank_password_use" => "Remote sign-in safeguards",
        "wdigest.use_logon_credential" => "Sign-in secret protection",
        "permissions.service.bits" => "Update download protection",
        "permissions.service.wuauserv" => "Windows Update tamper protection",
        "defender.cloud_protection" => "Cloud threat lookups",
        "defender.pua" => "Junk app blocking",
        "defender.script_nis" => "Script and network attack checks",
        "defender.asr.standard" => "Extra shields against password theft",
        "defender.asr.web_script_email" => "Risky script and attachment warnings",
        "lsa.run_as_ppl" => "Sign-in process shield",
        "net.public_sharing_exposure" => "Hide your PC on public Wi-Fi",
        "printer.point_and_print" => "Printer driver safety",
        "net.llmnr" => "Fake name-lookup blocking",
        "accounts.lockout_policy" => "Password guessing lockout",
        "autorun.disabled" => "USB stick auto-start",
        "wifi.risky_profiles" => "Wi-Fi networks that join by themselves",
        "lsa.restrict_anonymous" => "Anonymous account listing",
        "remote_assistance.disabled" => "Remote Assistance invitations",
        "wsh.disabled" => "Old script files",
        "update.auto_policy_disabled" => "Automatic updates switched off",
        "ntlm.lm_compat_level" => "Old sign-in methods",
        "accounts.builtin_administrator" => "Hidden Administrator account",
        "privacy.activity_history" => "Activity history",
        "privacy.advertising_id" => "Ad tracking ID",
        "findings" => "Additional protection checks",
        _ => "Protection check",
    }
}

fn control_help(id: &str) -> (&'static str, NextStep) {
    use NextStep::*;
    match id {
        "defender.realtime" | "defender.behavior" | "defender.ioav" | "defender.archive" => (
            "Open Windows Security and make sure virus protection is on.",
            OpenWindowsSecurity,
        ),
        "firewall.domain.enabled"
        | "firewall.private.enabled"
        | "firewall.public.enabled"
        | "firewall.domain.inbound"
        | "firewall.private.inbound"
        | "firewall.public.inbound" => (
            "Open Windows Security and make sure the firewall is on.",
            OpenWindowsSecurity,
        ),
        "uac.enabled" | "uac.consent" => (
            "We can't change this one safely for you. If you're not sure, leave it as it is.",
            ReviewWithAdministrator,
        ),
        "installer.always_install_elevated" => (
            "We can't change this one safely for you. If you're not sure, leave it as it is.",
            ReviewWithAdministrator,
        ),
        "lsa.restrict_anonymous_sam" => (
            "We can't change this one safely for you. If you're not sure, leave it as it is.",
            ReviewWithAdministrator,
        ),
        "lsa.limit_blank_password_use" => (
            "Make sure every account on this PC has a password.",
            OpenAccounts,
        ),
        "wdigest.use_logon_credential" => (
            "We can't change this one safely for you. If you're not sure, leave it as it is.",
            ReviewWithAdministrator,
        ),
        "permissions.service.bits" | "permissions.service.wuauserv" => (
            "We can't change this one safely for you. If you're not sure, leave it as it is.",
            ReviewWithAdministrator,
        ),
        "defender.cloud_protection" | "defender.pua" | "defender.script_nis" => (
            "Open Windows Security and make sure virus protection is fully turned on.",
            OpenWindowsSecurity,
        ),
        "accounts.builtin_administrator" => (
            "Check who can sign in to this PC and switch off accounts nobody uses.",
            OpenAccounts,
        ),
        "update.auto_policy_disabled" => (
            "Open Windows Update and make sure updates are allowed to install.",
            OpenWindowsUpdate,
        ),
        "defender.asr.standard"
        | "defender.asr.web_script_email"
        | "lsa.run_as_ppl"
        | "net.public_sharing_exposure"
        | "printer.point_and_print"
        | "net.llmnr"
        | "accounts.lockout_policy"
        | "autorun.disabled"
        | "wifi.risky_profiles"
        | "lsa.restrict_anonymous"
        | "remote_assistance.disabled"
        | "wsh.disabled"
        | "ntlm.lm_compat_level"
        | "privacy.activity_history"
        | "privacy.advertising_id" => (
            "We can't change this one safely for you. If you're not sure, leave it as it is.",
            ReviewWithAdministrator,
        ),
        _ => (
            "Check again in a moment. Nothing has been changed.",
            CheckAgain,
        ),
    }
}

/// True for choices the person makes themselves. They are offered with a
/// one-line consequence and are never ticked by default.
pub fn is_choice(id: &str) -> bool {
    secblitz::hardening::is_ask(id)
}

/// One plain line telling the person what changes if they say yes.
pub fn choice_consequence(id: &str) -> &'static str {
    match id {
        "defender.cloud_protection" => {
            "Windows Defender will send details about suspicious files to Microsoft to catch new threats faster."
        }
        "defender.asr.web_script_email" => {
            "Windows will ask before running scripts or email attachments that look risky, and you can allow them."
        }
        "lsa.run_as_ppl" => {
            "Sign-in add-ons from other companies may stop working. Needs a restart."
        }
        "autorun.disabled" => {
            "Plugging in a USB stick or disc will no longer pop up a menu. Open it from File Explorer instead."
        }
        "wifi.risky_profiles" => {
            "Saved risky Wi-Fi networks stop joining by themselves. You can still connect by hand."
        }
        "lsa.restrict_anonymous" => {
            "Very old devices may stop showing your shared folders. Needs a restart."
        }
        "remote_assistance.disabled" => {
            "Nobody can invite a helper to take over this PC. Quick Assist still works."
        }
        "wsh.disabled" => "Old .vbs and .js script files will stop running when you open them.",
        "update.auto_policy_disabled" => {
            "Windows will go back to installing security updates by itself."
        }
        "ntlm.lm_compat_level" => {
            "Very old network drives or scanners may stop signing in. Needs a restart."
        }
        "accounts.builtin_administrator" => {
            "The hidden Administrator account is switched off. Your own account is not affected."
        }
        "privacy.activity_history" => "Windows stops keeping a list of what you did on this PC.",
        "privacy.advertising_id" => "Apps will show less relevant ads. Nothing else changes.",
        _ => "",
    }
}

/// Exact backend reasons for "this protection is not offered on this PC right
/// now". They are calm facts, not faults, so they never lower the score.
fn not_offered(reason: &str) -> Option<&'static str> {
    Some(match reason {
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
        _ => return None,
    })
}

fn repair_help(id: &str) -> &'static str {
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
            "We can fix this. Your password will no longer be kept where it can be stolen."
        }
        "uac.enabled" => "We can fix this. Windows will ask before big changes are made.",
        "uac.consent" => "We can fix this. Windows will ask for approval before big changes.",
        "installer.always_install_elevated" => {
            "We can fix this. App installers will no longer get full control of your PC."
        }
        "lsa.restrict_anonymous_sam" => {
            "We can fix this. Strangers on the network will no longer see your account names."
        }
        "lsa.limit_blank_password_use" => {
            "We can fix this. Accounts without a password can no longer be used over the network."
        }
        "defender.pua" => "We can fix this. Junk apps bundled with downloads will be blocked.",
        "defender.script_nis" => "We can fix this. It turns scanning for harmful scripts back on.",
        "defender.asr.standard" => {
            "We can fix this. It blocks common tricks used to steal passwords."
        }
        "net.public_sharing_exposure" => {
            "We can fix this. Your shared files and printers stay hidden on public Wi-Fi."
        }
        "printer.point_and_print" => {
            "We can fix this. Printer drivers will only be installed with your permission."
        }
        "net.llmnr" => "We can fix this. Fake name-lookup answers will be ignored.",
        "accounts.lockout_policy" => {
            "We can fix this. Too many wrong passwords will lock sign-in for a few minutes."
        }
        _ => "We can fix this. It turns this protection on.",
    }
}

fn managed(reason: &str) -> bool {
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

fn base(label: &'static str, status: &str, help: (&'static str, NextStep)) -> Advice {
    let mut a = Advice {
        label,
        status: "Needs your choice",
        next: help.0,
        step: help.1,
        group: Group::Choice,
        impact: "",
        ask: false,
    };
    match status {
        "compliant" | "ok" => {
            a.status = "Good to go";
            a.next = "Nothing to do here.";
            a.step = NextStep::None;
            a.group = Group::Protected;
        }
        "unknown" | "error" => {
            a.status = "Couldn't check";
        }
        _ => {}
    }
    a
}

pub fn for_control(id: &str, status: &str, detail: &str) -> Advice {
    let mut a = base(control_label(id), status, control_help(id));
    match status {
        "attention" if a.label != "Protection check" && id != "findings" => {
            a.status = "Can fix";
            a.next = repair_help(id);
            a.step = NextStep::Repair;
            a.group = Group::Recommended;
            if is_choice(id) {
                // Never pre-selected: the person decides, knowing what changes.
                a.status = "Your choice";
                a.next = choice_consequence(id);
                a.ask = true;
                // Privacy tidy-ups are optional extras, not protection gaps.
                a.group = if id.starts_with("privacy.") {
                    Group::Information
                } else {
                    Group::Choice
                };
            }
        }
        "applied" => {
            a.status = "Fixed";
            a.next = "This setting was updated and checked.";
            a.step = NextStep::None;
            a.group = Group::Protected;
        }
        "unchanged"
            if matches!(
                detail,
                "Target preference already present"
                    | "Target preference already present; original before image retained"
            ) =>
        {
            a.status = "Good to go";
            a.next = "Nothing to do here.";
            a.step = NextStep::None;
            a.group = Group::Protected;
        }
        "restored" => {
            a.next = "Your earlier setting was restored.";
            a.step = NextStep::CheckAgain;
        }
        "pending" => {
            a.next = "Undo your last fixes before making new ones.";
            a.step = NextStep::ReviewUndo;
        }
        "conflict" => {
            a.next = "This setting changed again after our fix, so we left it alone.";
            a.step = NextStep::ReviewUndo;
        }
        "skipped" if managed(detail) => {
            a.status = "Managed elsewhere";
            a.next = "This PC's owner controls this setting, so we leave it as it is.";
            a.step = NextStep::ReviewWithAdministrator;
        }
        "skipped" if not_offered(detail).is_some() => {
            a.status = "Not offered";
            a.next = not_offered(detail).unwrap_or_default();
            a.step = NextStep::None;
            a.group = Group::Information;
        }
        "skipped"
            if matches!(
                detail,
                "Preserving absent or nonzero UAC preference"
                    | "Preserving absent or already-safe machine preference"
            ) =>
        {
            a.next =
                "We kept your current setting. It may already protect you, so nothing was changed.";
        }
        "skipped"
            if matches!(
                detail,
                "Revert the active transaction before starting another apply"
                    | "Revert the active transaction before applying again"
            ) =>
        {
            a.next = "Undo your last fixes before making new ones.";
            a.step = NextStep::ReviewUndo;
        }
        _ => {}
    }
    if (status == "applied" && detail == "Preference applied; restart required")
        || (status == "restored" && detail == "Original preference restored; restart required")
    {
        a.status = "Restart needed";
        a.group = Group::Choice;
        a.next = "Save your work and restart your PC to finish this change.";
        a.step = NextStep::Restart;
    }
    a.impact = control_impact(id);
    a
}

/// Typed firewall evidence refines presentation only. Status and the engine's
/// live eligibility checks remain mutation authority; prose is not evidence.
pub fn for_outcome(outcome: &secblitz::engine::Outcome) -> Advice {
    use secblitz::model::{Authority, EffectiveFirewall, InboundAction};
    let mut a = for_control(&outcome.id, &outcome.status, &outcome.detail);
    let enabled = matches!(
        outcome.id.as_str(),
        "firewall.domain.enabled" | "firewall.private.enabled" | "firewall.public.enabled"
    );
    let inbound = matches!(
        outcome.id.as_str(),
        "firewall.domain.inbound" | "firewall.private.inbound" | "firewall.public.inbound"
    );
    if !enabled && !inbound {
        return a;
    }
    if outcome.authority == Some(Authority::Managed) {
        a.status = "Managed elsewhere";
        a.next = "This PC's owner controls this setting, so we leave it as it is.";
        a.step = NextStep::ReviewWithAdministrator;
        a.group = Group::Choice;
        return a;
    }
    let verified = matches!(outcome.effective, Some(EffectiveFirewall::Enabled(_))) && enabled
        || matches!(outcome.effective, Some(EffectiveFirewall::Inbound(_))) && inbound;
    if outcome.authority != Some(Authority::Local) || !verified {
        if matches!(
            outcome.status.as_str(),
            "attention" | "compliant" | "ok" | "unchanged" | "applied"
        ) {
            a.status = "Couldn't check";
            a.next =
                "We couldn't confirm your firewall setting. Check again before making changes.";
            a.step = NextStep::CheckAgain;
            a.group = Group::Choice;
        }
        return a;
    }
    if matches!(
        outcome.status.as_str(),
        "compliant" | "ok" | "unchanged" | "applied"
    ) {
        let protected = matches!(
            outcome.effective,
            Some(
                EffectiveFirewall::Enabled(true) | EffectiveFirewall::Inbound(InboundAction::Block)
            )
        );
        if protected && outcome.status != "applied" {
            a.status = "Protected by Windows";
            a.next = "Windows is already blocking these connections. Nothing to do.";
            a.step = NextStep::None;
            a.group = Group::Protected;
        } else if !protected {
            a.status = "Needs your choice";
            a.next =
                "We couldn't confirm your firewall setting. Check again before making changes.";
            a.step = NextStep::CheckAgain;
            a.group = Group::Choice;
        }
    }
    a
}

pub fn for_finding(title: &str, status: &str, _detail: &str) -> Advice {
    use NextStep::*;
    let (label, next, step) = match title {
        "Security providers" => ("Your security apps", "Open Windows Security to make sure your antivirus is on and working.", OpenWindowsSecurity),
        "Windows Firewall" => ("Network protection", "Open Windows Security and make sure the firewall is on.", OpenWindowsSecurity),
        "Defender" => ("Virus protection", "Open Windows Security to make sure virus protection is on and up to date.", OpenWindowsSecurity),
        "Windows lifecycle" => ("Windows support", "Open Windows Update to check your version of Windows still gets security updates.", OpenWindowsUpdate),
        "Device encryption" => ("Protection if your PC is lost", "Open encryption settings. Save your recovery key somewhere safe before you change anything.", OpenEncryption),
        "Secure Boot" => ("Startup protection", "This is set when your PC starts up. Follow your PC maker's guide before changing it.", ReviewFirmware),
        "Windows updates" => ("Windows updates", "Open Windows Update and install anything that is waiting.", OpenWindowsUpdate),
        "Remote Desktop" => ("Remote access", "Remote access lets someone sign in to this PC from elsewhere. Turn it off in Settings if you don't use it.", OpenRemoteDesktop),
        "SMB1" => ("Older file sharing", "An old way of sharing files is still on. Turn it off in Windows Features unless an old device needs it.", ReviewWindowsFeatures),
        "SmartScreen" => ("Unsafe app and website warnings", "Open Windows Security and make sure warnings about risky apps and websites are on.", OpenWindowsSecurity),
        "Local accounts" => ("Account sign-in safety", "Check who can sign in to this PC. Give each account its own strong password.", OpenAccounts),
        "Memory integrity" => ("Core system protection", "Open Windows Security and look at the extra protection for the core of Windows. Some older devices don't work with it.", OpenWindowsSecurity),
        "Management and mutation eligibility" => ("Who manages this PC", "If you're not sure who manages this PC, look at work or school accounts in Settings.", ReviewWithAdministrator),
        "Automatic logon" => ("Automatic sign-in", "Your PC signs in by itself. Turn that off if other people can get to it.", OpenAccounts),
        "Service permissions: BITS" => ("Update download permissions", "We leave this one alone. If a fix is available, it appears under Needs your attention.", ReviewWithAdministrator),
        "Service permissions: wuauserv" => ("Windows Update permissions", "We leave this one alone. If a fix is available, it appears under Needs your attention.", ReviewWithAdministrator),
        "Service permissions: WinDefend" => ("Antivirus service permissions", "We leave this one alone. If a fix is available, it appears under Needs your attention.", ReviewWithAdministrator),
        "Service permissions: Schedule" => ("Scheduled task service permissions", "We leave this one alone. If a fix is available, it appears under Needs your attention.", ReviewWithAdministrator),
        "Service permissions: SecblitzMonitor" => ("Protection monitor permissions", "We leave this one alone. If a fix is available, it appears under Needs your attention.", ReviewWithAdministrator),
        "Journal recovery" => ("Saved changes", "Undo your last fixes before making new ones.", ReviewUndo),
        "Assessment unavailable" | "Service permission audit" => ("Additional protection checks", "Check again in a moment. Nothing has been changed.", CheckAgain),
        _ => ("Protection check", "Check again in a moment. Nothing has been changed.", CheckAgain),
    };
    let mut a = base(label, status, (next, step));
    if status == "info" {
        a.group = Group::Information;
        a.status = "For your information";
    }
    a.impact = finding_impact(title);
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_recognized_control_and_finding_has_non_empty_impact() {
        // Every control id handled by control_label (except fallback) gets a phrase.
        for id in [
            "defender.realtime",
            "defender.behavior",
            "defender.ioav",
            "defender.archive",
            "firewall.domain.enabled",
            "firewall.private.enabled",
            "firewall.public.enabled",
            "firewall.domain.inbound",
            "firewall.private.inbound",
            "firewall.public.inbound",
            "uac.enabled",
            "uac.consent",
            "installer.always_install_elevated",
            "lsa.restrict_anonymous_sam",
            "lsa.limit_blank_password_use",
            "wdigest.use_logon_credential",
            "permissions.service.bits",
            "permissions.service.wuauserv",
        ] {
            let a = for_control(id, "attention", "");
            assert!(!a.impact.is_empty(), "missing impact for control: {id}");
            let a_ok = for_control(id, "compliant", "");
            assert!(
                !a_ok.impact.is_empty(),
                "missing impact for compliant control: {id}"
            );
        }
        // Fallback and aggregate have no impact phrase.
        assert!(for_control("unknown.id", "attention", "").impact.is_empty());
        assert!(for_control("findings", "attention", "").impact.is_empty());

        // Every recognized finding title with a concrete protection gets a phrase.
        for title in [
            "Windows lifecycle",
            "Device encryption",
            "Secure Boot",
            "Windows updates",
            "Remote Desktop",
            "SMB1",
            "SmartScreen",
            "Local accounts",
            "Memory integrity",
            "Automatic logon",
        ] {
            let a = for_finding(title, "attention", "");
            assert!(!a.impact.is_empty(), "missing impact for finding: {title}");
        }
        // Informational / audit findings have no impact phrase.
        for title in [
            "Security providers",
            "Windows Firewall",
            "Defender",
            "Journal recovery",
            "Assessment unavailable",
        ] {
            assert!(
                for_finding(title, "attention", "").impact.is_empty(),
                "unexpected impact for finding: {title}"
            );
        }
    }

    #[test]
    fn every_extended_control_has_plain_label_impact_help_and_the_right_kind_of_offer() {
        for spec in secblitz::hardening::all() {
            let id = spec.id;
            assert_ne!(control_label(id), "Protection check", "{id}");
            assert!(!control_impact(id).is_empty(), "{id}");
            assert_eq!(is_choice(id), spec.ask, "{id}");
            // Safe or default-safe state is protected.
            let ok = for_control(id, "compliant", "");
            assert_eq!(ok.group, Group::Protected, "{id}");
            assert!(!ok.ask);
            // Unsafe state is repairable; choices are never pre-selected.
            let a = for_control(id, "attention", "Eligible");
            assert_eq!(a.step, NextStep::Repair, "{id}");
            assert_eq!(a.ask, spec.ask, "{id}");
            if spec.ask {
                assert_eq!(a.status, "Your choice");
                assert_eq!(a.next, choice_consequence(id));
                assert!(a.next.ends_with('.') && !a.next.contains('\n'));
                assert!(a.next.len() < 130, "{id}: consequence must stay one short line");
                assert_eq!(
                    a.group,
                    if id.starts_with("privacy.") { Group::Information } else { Group::Choice }
                );
            } else {
                assert_eq!(a.status, "Can fix");
                assert_eq!(a.group, Group::Recommended);
                assert!(choice_consequence(id).is_empty());
            }
            // Management and capability vetoes are never offered as a fix.
            let managed = for_control(id, "skipped", "Applied computer Group Policy: assessment only");
            assert_eq!(managed.status, "Managed elsewhere", "{id}");
            assert_ne!(managed.step, NextStep::Repair);
            // Restart-needed controls say so after applying.
            let applied = for_control(id, "applied", "Preference applied; restart required");
            assert_eq!(applied.status, "Restart needed", "{id}");
        }
        assert!(!is_choice("uac.enabled") && !is_choice("unknown.id"));
    }

    #[test]
    fn not_offered_reasons_are_calm_facts_that_never_count_against_the_score() {
        for reason in [
            "Not offered: Secure Boot is off",
            "Not offered: Smart App Control is on",
            "Not offered: some sign-in add-ons would stop working",
            "Not offered: sign-in add-ons from other companies are installed",
            "Not offered: no other administrator account is enabled",
            "Not offered: no other administrator account could be confirmed",
            "Not offered: Defender real-time protection is off",
            "Not offered: Defender cloud protection is off",
            "Not offered: this PC uses Configuration Manager",
        ] {
            let a = for_control("lsa.run_as_ppl", "skipped", reason);
            assert_eq!(a.status, "Not offered", "{reason}");
            assert_eq!(a.group, Group::Information);
            assert_ne!(a.step, NextStep::Repair);
        }
        // Unknown reasons are not trusted as "not offered".
        assert_ne!(
            for_control("lsa.run_as_ppl", "skipped", "Not offered: anything").status,
            "Not offered"
        );
    }

    #[test]
    fn impact_prefix_follows_group_and_is_empty_when_impact_is_empty() {
        // Protected → "Protects you from:"
        let a = for_control("uac.enabled", "compliant", "");
        assert_eq!(a.group, Group::Protected);
        assert_eq!(a.impact_prefix(), "Protects you from:");

        // Recommended → "Leaves you open to:"
        let a = for_control("uac.enabled", "attention", "");
        assert_eq!(a.group, Group::Recommended);
        assert_eq!(a.impact_prefix(), "Leaves you open to:");

        // Choice → "Why it matters:"
        let a = for_control("uac.enabled", "unknown", "");
        assert_eq!(a.group, Group::Choice);
        assert_eq!(a.impact_prefix(), "Why it matters:");

        // Restart needed → Choice group → "Why it matters:"
        let a = for_control(
            "uac.enabled",
            "applied",
            "Preference applied; restart required",
        );
        assert_eq!(a.group, Group::Choice);
        assert_eq!(a.impact_prefix(), "Why it matters:");

        // Empty impact → empty prefix regardless of group
        let a = for_control("unknown.id", "compliant", "");
        assert!(a.impact.is_empty());
        assert_eq!(a.impact_prefix(), "");
    }

    #[test]
    fn applied_firewall_with_contradictory_evidence_is_not_protected() {
        use secblitz::{
            engine::Outcome,
            model::{Authority, EffectiveFirewall, InboundAction},
        };
        for (id, effective) in [
            ("firewall.public.enabled", EffectiveFirewall::Enabled(false)),
            (
                "firewall.private.inbound",
                EffectiveFirewall::Inbound(InboundAction::Allow),
            ),
        ] {
            let outcome = Outcome {
                id: id.into(),
                status: "applied".into(),
                effective: Some(effective),
                authority: Some(Authority::Local),
                ..Default::default()
            };
            let advice = for_outcome(&outcome);
            assert_eq!(advice.group, Group::Choice);
            assert_eq!(advice.step, NextStep::CheckAgain);
            assert_ne!(advice.status, "Fixed");
        }
        assert_eq!(
            for_control(
                "uac.enabled",
                "skipped",
                "Relevant policy is configured: assessment only"
            )
            .status,
            "Managed elsewhere"
        );
    }

    #[test]
    fn firewall_protection_uses_typed_evidence_never_reason_text() {
        use secblitz::{
            engine::Outcome,
            model::{Authority, EffectiveFirewall, InboundAction},
        };
        let mut outcome = Outcome {
            id: "firewall.public.inbound".into(),
            status: "compliant".into(),
            detail: "untrusted prose saying defaults are safe".into(),
            effective: Some(EffectiveFirewall::Inbound(InboundAction::Block)),
            authority: Some(Authority::Local),
            ..Default::default()
        };
        assert_eq!(for_outcome(&outcome).status, "Protected by Windows");
        assert_eq!(for_outcome(&outcome).step, NextStep::None);
        outcome.authority = Some(Authority::Managed);
        assert_eq!(for_outcome(&outcome).status, "Managed elsewhere");
        assert_eq!(for_outcome(&outcome).group, Group::Choice);
        for authority in [None, Some(Authority::Unknown)] {
            outcome.authority = authority;
            assert_ne!(for_outcome(&outcome).group, Group::Protected);
        }
        outcome.authority = Some(Authority::Local);
        for evidence in [
            None,
            Some(EffectiveFirewall::Enabled(true)),
            Some(EffectiveFirewall::Inbound(InboundAction::Allow)),
        ] {
            outcome.effective = evidence;
            assert_ne!(for_outcome(&outcome).group, Group::Protected);
        }
        outcome.status = "attention".into();
        assert_eq!(for_outcome(&outcome).step, NextStep::Repair);
        outcome.status = "skipped".into();
        assert_ne!(for_outcome(&outcome).step, NextStep::Repair);
    }

    #[test]
    fn informational_findings_do_not_count_as_problems_or_protection() {
        for title in ["Windows updates", "Journal recovery", "Local accounts"] {
            assert_eq!(for_finding(title, "info", "").group, Group::Information);
            assert_eq!(for_finding(title, "pending", "").group, Group::Choice);
        }
    }

    #[test]
    fn findings_never_offer_repairs_or_infer_health_from_info() {
        for title in [
            "Windows Firewall",
            "Defender",
            "Service permissions: BITS",
            "Windows updates",
            "Local accounts",
        ] {
            for status in ["attention", "review", "info", "unknown"] {
                let a = for_finding(title, status, "Everything is fine; eligible");
                assert_ne!(a.step, NextStep::Repair);
                assert_ne!(a.group, Group::Protected);
                assert!(!a.next.contains("Secblitz can fix"));
            }
        }
    }

    #[test]
    fn skips_restores_and_restart_do_not_claim_protection() {
        let a = for_control(
            "uac.enabled",
            "skipped",
            "Preserving absent or nonzero UAC preference",
        );
        assert_eq!(a.status, "Needs your choice");
        assert!(a.next.contains("may already protect"));
        assert_eq!(
            for_control(
                "uac.enabled",
                "skipped",
                "Domain-managed machine: assessment only"
            )
            .status,
            "Managed elsewhere"
        );
        assert_eq!(
            for_control(
                "uac.enabled",
                "skipped",
                "Group Policy authority is unknown: assessment only"
            )
            .status,
            "Needs your choice"
        );
        for (status, detail) in [
            ("restored", "Original preference restored"),
            ("unchanged", "Original preference already present"),
            ("skipped", "new reason"),
        ] {
            assert_eq!(
                for_control("uac.enabled", status, detail).group,
                Group::Choice
            );
        }
        assert_eq!(
            for_control(
                "wdigest.use_logon_credential",
                "applied",
                "Preference applied; restart required"
            )
            .status,
            "Restart needed"
        );
        assert_eq!(
            for_control("unknown.control", "attention", "eligible").step,
            NextStep::CheckAgain
        );
        assert_eq!(
            for_control("firewall.public.enabled", "attention", "").step,
            NextStep::Repair
        );
    }
}

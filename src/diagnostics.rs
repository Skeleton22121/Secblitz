//! Read-only, bounded diagnostics; deliberately independent of the repair engine.
//!
//! Integration: `diagnostics::collect(Profile::Everyday, &Context::default())`
//! returns a serializable machine-scoped snapshot, including per-probe failures.
//! Set `context.original_user = OriginalUserScope::VerifyCurrentDesktopUser` to
//! request an attached browser inventory. Native token checks are mandatory and
//! cannot be overridden by a caller-supplied account, SID, path or command.
//!
//! Collection does not elevate, scan online, refresh update metadata, enable
//! protections, run software, repair disks, test restores or write output files.
//! Callers choose whether/where to serialize the returned report. Timestamps are
//! UTC Unix seconds and describe observation, not freshness of cached evidence.
//! Rule references are advisory mappings, not a compliance certification.

#[path = "diagnostics/types.rs"]
mod types;
pub use types::*;
#[path = "diagnostics/checks.rs"]
mod checks;
#[cfg(any(windows, test))]
#[path = "diagnostics/parse.rs"]
mod parse;
#[path = "diagnostics/rules.rs"]
mod rules;
#[cfg(test)]
#[path = "diagnostics/tests.rs"]
mod tests;
#[cfg(windows)]
#[path = "diagnostics/windows.rs"]
mod windows;

pub const SCHEMA_VERSION: u32 = 1;
pub const RULE_MAPPING_VERSION: &str = "2026-10-05.1";
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_ITEMS: usize = 512;
pub const PROBE_TIMEOUT_SECONDS: u64 = 15;
pub const COLLECTION_TIMEOUT_SECONDS: u64 = 180;

/// Conservative, versioned product-lifecycle mapping. All releases of these
/// exact legacy products have ended support. Other products are not assessed.
/// Registration strings are inventory evidence, not binary authenticity.
pub fn support_assessment(app: &Application) -> SupportAssessment {
    let lifecycle = if matches!(
        app.publisher.as_str(),
        "Adobe Systems Incorporated" | "Adobe"
    ) && matches!(
        app.name.as_str(),
        "Adobe Flash Player 32 NPAPI"
            | "Adobe Flash Player 32 PPAPI"
            | "Adobe Flash Player 32 ActiveX"
    ) {
        Some((
            "2020-12-31",
            "https://www.adobe.com/products/flashplayer/end-of-life.html",
        ))
    } else if app.publisher == "Microsoft Corporation"
        && app.name == "Microsoft Silverlight"
        && app.version.starts_with("5.")
        && app.version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        Some((
            "2021-10-12",
            "https://learn.microsoft.com/lifecycle/products/silverlight-5",
        ))
    } else {
        None
    };
    lifecycle.map_or(SupportAssessment::NotAssessed, |(date, url)| {
        SupportAssessment::KnownEndOfSupport {
            ended_on: date.into(),
            reference: url.into(),
        }
    })
}

/// Partial failures are data, not an all-or-nothing error. No machine identifiers,
/// account names, network addresses, credentials or recovery keys are returned.
pub fn collect(profile: Profile, context: &Context) -> Report {
    #[cfg(windows)]
    let probes = windows::collect(context);
    #[cfg(not(windows))]
    let probes = ProbeId::ALL
        .iter()
        .map(|&id| unavailable(id, UnknownReason::PlatformUnsupported))
        .collect();
    assemble(profile, context, probes)
}

fn now() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|v| v.as_secs())
}

fn unavailable(id: ProbeId, reason: UnknownReason) -> Diagnostic {
    Diagnostic {
        id,
        scope: id.scope(),
        observed_at_unix_seconds: now(),
        source: id.source().into(),
        status: if reason == UnknownReason::PlatformUnsupported {
            Status::Unsupported
        } else {
            Status::Unknown
        },
        evidence: None,
        failure: Some(reason),
        assessments: vec![],
    }
}

fn assemble(profile: Profile, context: &Context, mut probes: Vec<Diagnostic>) -> Report {
    // Missing probes are explicit even when a collector exits early.
    for &id in ProbeId::ALL {
        if !probes.iter().any(|p| p.id == id) {
            probes.push(unavailable(id, UnknownReason::NotCollected));
        }
    }
    let management = rules::management(&probes);
    let mut recommendations = Vec::new();
    for probe in &mut probes {
        if probe.evidence.is_some() {
            probe.assessments = rules::assess(probe);
            probe.status = rules::aggregate(probe.assessments.iter().map(|a| a.status));
        } else {
            probe.assessments = vec![rules::unavailable_assessment(probe)];
        }
        for assessment in &probe.assessments {
            if matches!(assessment.status, Status::Attention | Status::Unknown) {
                recommendations.push(rules::recommendation(
                    assessment,
                    profile,
                    management,
                    &context.compatibility,
                ));
            }
        }
    }
    recommendations.extend(rules::profile_recommendations(
        profile,
        management,
        &context.compatibility,
    ));
    let coverage = Coverage {
        probes_with_evidence: probes.iter().filter(|p| p.evidence.is_some()).count(),
        probes_without_evidence: probes.iter().filter(|p| p.evidence.is_none()).count(),
        assessments_unknown: probes
            .iter()
            .flat_map(|p| &p.assessments)
            .filter(|a| a.status == Status::Unknown)
            .count(),
        total_probes: ProbeId::ALL.len(),
    };
    let mut omissions = vec![
        Omission::new(Scope::Machine, "Updates", "Offline WUA cache and bounded local history only; no fresh online scan, entitlement or complete missing-update assurance."),
        Omission::new(Scope::Machine, "Software", "HKLM uninstall registrations only; portable apps, Store apps and per-user installs omitted. Support is unknown except exact products in the versioned end-of-support mapping."),
        Omission::new(Scope::Machine, "Recovery", "Backup events and local shadow copies do not establish data coverage, off-device copies, recovery-key escrow or a successful restore. No restore is tested."),
        Omission::new(Scope::Machine, "WinRE", "Only an unambiguous English REAgentC status line is recognized. Localized/unrecognized output is Unknown; recovery locations and BCD identifiers are discarded."),
        Omission::new(Scope::Machine, "Network", "No connectivity requests, packet capture, web history, credentials, IP/DNS/proxy/VPN endpoints, interface names or private paths. Machine WinHTTP default is not every application's effective proxy."),
        Omission::new(Scope::OriginalUser, "User settings", "Per-user software, backup settings, VPN connections, browser policies, proxy/PAC settings and other profiles are not inspected."),
        Omission::new(Scope::Machine, "Permissions", "Fixed-service broad-principal ACE audit only; no token AccessCheck, filesystem-wide audit or proof of exploitability."),
        Omission::new(Scope::Machine, "Detect-only checks", "Hosts file, Defender exclusions and threats, shares, firewall rules, services and accounts are reduced to counts and fixed categories on the device; no paths, names, host entries, SSIDs or file contents are collected, and nothing is changed."),
        Omission::new(Scope::Machine, "Exposure","Local configuration and TCP listeners do not prove remote reachability; firewall, upstream NAT, credentials and group nesting may alter effective access."),
    ];
    let browsers = probes
        .iter()
        .find(|p| p.id == ProbeId::BrowserExtensions)
        .unwrap();
    omissions.push(Omission::new(Scope::OriginalUser, "Browser extensions", if browsers.evidence.is_some() {
        "Original desktop user and default local profile roots only. Chromium manifest presence does not prove an extension is enabled; policy/force-install state, sync, unpacked extensions, custom profile roots and other browsers are omitted. No history, cookies or preferences are read."
    } else {
        "Inventory omitted: original non-elevated desktop-user identity was not established, not requested, unavailable, or the probe failed. Elevated administrator profiles are never substituted."
    }));
    Report {
        schema_version: SCHEMA_VERSION,
        rule_mapping_version: RULE_MAPPING_VERSION.into(),
        collected_at_unix_seconds: now(),
        scope: Scope::Machine,
        profile,
        compatibility: context.compatibility.clone(),
        management,
        status: rules::aggregate(
            probes
                .iter()
                .filter(|p| p.scope == Scope::Machine)
                .map(|p| p.status),
        ),
        probes,
        recommendations,
        coverage,
        omissions,
    }
}

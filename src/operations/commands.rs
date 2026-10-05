use super::*;

pub(super) fn command_spec(kind: OperationKind) -> Result<(&'static str, &'static [&'static str])> {
    use OperationKind::*;
    Ok(match kind {
        DismCheckHealth => (
            "System32/dism.exe",
            &[
                "/Online",
                "/Cleanup-Image",
                "/CheckHealth",
                "/English",
                "/NoRestart",
            ],
        ),
        DismScanHealth => (
            "System32/dism.exe",
            &[
                "/Online",
                "/Cleanup-Image",
                "/ScanHealth",
                "/English",
                "/NoRestart",
            ],
        ),
        DismRestoreHealth => (
            "System32/dism.exe",
            &[
                "/Online",
                "/Cleanup-Image",
                "/RestoreHealth",
                "/English",
                "/LimitAccess",
                "/NoRestart",
            ],
        ),
        SfcVerify => ("System32/sfc.exe", &["/verifyonly"]),
        SfcRepair => ("System32/sfc.exe", &["/scannow"]),
        DefenderQuickScan => bail!("Defender requires the compiled scan script"),
    })
}

pub(super) fn script(defender: bool, action: &str) -> Result<String> {
    ensure!(
        matches!(action, "probe" | "scan" | "verify") && (defender || action != "scan"),
        "Invalid compiled script action"
    );
    let source = include_str!("../platform/backend.ps1");
    let delimiter = "\ntry {\n    switch -CaseSensitive ($action) {";
    ensure!(
        source.matches(delimiter).count() == 1,
        "Embedded policy helper boundary changed"
    );
    let definitions = source
        .split_once(delimiter)
        .context("Missing policy helpers")?
        .0;
    let kind = if defender { "defender" } else { "servicing" };
    Ok(format!("$inputJson=$null\n{definitions}\n$maintenanceKind='{kind}'\n$maintenanceAction='{action}'\n{}", include_str!("probe.ps1")))
}

pub(super) fn dism_evidence(bytes: &[u8]) -> Evidence {
    // /English is a compiled argument. Accept only an unambiguous exact health
    // line, never an exit-zero inference or a substring inside localized errors.
    let text = if bytes.starts_with(&[0xff, 0xfe])
        || bytes.iter().take(64).filter(|b| **b == 0).count() > 8
    {
        let bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
        if !bytes.len().is_multiple_of(2) {
            return Evidence::Inconclusive;
        }
        match String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        ) {
            Ok(s) => s,
            Err(_) => return Evidence::Inconclusive,
        }
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => return Evidence::Inconclusive,
        }
    };
    let mut findings = text.lines().filter_map(|line| match line.trim() {
        "No component store corruption detected." => Some(Evidence::ComponentStoreHealthy),
        "The component store is repairable." => Some(Evidence::ComponentStoreRepairable),
        "The component store cannot be repaired." => Some(Evidence::ComponentStoreNonRepairable),
        _ => None,
    });
    let first = findings.next().unwrap_or(Evidence::Inconclusive);
    if findings.next().is_some() {
        Evidence::Inconclusive
    } else {
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn servicing_gate_only_trusts_documented_repair_source_values() {
        // Windows writes CountryCode under ...\Policies\Servicing on stock PCs;
        // that must never be mistaken for a configured repair source.
        let text = script(false, "probe").unwrap();
        assert!(!text.contains("HasValues $p) { throw 'Configured servicing"));
        assert!(text.contains("ServicingSourcePolicyConfigured $p"));
        for name in [
            "LocalSourcePath",
            "RepairContentServerSource",
            "UseWindowsUpdate",
        ] {
            assert!(text.contains(&format!("'{name}'")), "{name}");
        }
        assert!(!text.contains("'CountryCode'"));
    }
    #[test]
    fn exact_commands_and_no_download_restart_or_arbitrary_source() {
        for kind in [
            OperationKind::DismCheckHealth,
            OperationKind::DismScanHealth,
            OperationKind::DismRestoreHealth,
            OperationKind::SfcVerify,
            OperationKind::SfcRepair,
        ] {
            let (exe, args) = command_spec(kind).unwrap();
            assert!(matches!(exe, "System32/dism.exe" | "System32/sfc.exe"));
            if exe.contains("dism") {
                assert!(args.contains(&"/NoRestart") && args.contains(&"/English"));
            }
            assert!(!args
                .iter()
                .any(|a| a.contains("Source") || a.contains("Reboot") || a.contains("--all")));
        }
        assert!(command_spec(OperationKind::DismRestoreHealth)
            .unwrap()
            .1
            .contains(&"/LimitAccess"));
        assert!(command_spec(OperationKind::DefenderQuickScan).is_err());
    }
    #[test]
    fn health_requires_exact_unambiguous_english_evidence() {
        assert_eq!(
            dism_evidence(b"No component store corruption detected.\r\n"),
            Evidence::ComponentStoreHealthy
        );
        assert_eq!(
            dism_evidence(b"The operation completed successfully."),
            Evidence::Inconclusive
        );
        assert_eq!(
            dism_evidence(b"prefix No component store corruption detected."),
            Evidence::Inconclusive
        );
        assert_eq!(
            dism_evidence(
                b"No component store corruption detected.\nThe component store is repairable."
            ),
            Evidence::Inconclusive
        );
        let text: Vec<u8> = "No component store corruption detected.\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(dism_evidence(&text), Evidence::ComponentStoreHealthy);
        assert_eq!(dism_evidence(&[0xff, 0xfe, 65]), Evidence::Inconclusive);
        assert_eq!(
            dism_evidence(b"No component store corruption detected.\n\xff"),
            Evidence::Inconclusive
        );
    }
    #[test]
    fn script_boundary_and_action_injection_are_closed() {
        for action in ["probe", "scan", "verify"] {
            let script = script(true, action).unwrap();
            assert!(!script.contains("switch -CaseSensitive ($action)"));
            assert!(script.contains("function MdmRegistered"));
            assert!(script.contains("function MaintenanceGate"));
            assert!(!script.contains("Restart-Computer"));
        }
        for action in ["", "write", "scan'; exit", "probe\0", "SCAN", "verify "] {
            assert!(script(true, action).is_err());
        }
        assert!(script(false, "scan").is_err());
        for spec in capabilities().operations {
            assert_eq!(
                spec.kind.as_str().parse::<OperationKind>().unwrap(),
                spec.kind
            );
        }
        for input in [
            "dism_restore_health;cmd",
            "DismScanHealth",
            "sfc_repair ",
            "upgrade --all",
            "C:\\sfc.exe",
        ] {
            assert!(input.parse::<OperationKind>().is_err());
        }
    }
}

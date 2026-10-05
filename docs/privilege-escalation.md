# Reversible privilege-escalation hardening

## Scope and contract

Secblitz adds four fixed machine-registry controls to its existing twelve preferences. These reduce an MSI elevation route, credential exposure, and account reconnaissance/remote blank-password access. They are defense in depth, not a claim to prevent every local privilege escalation or to remediate an already compromised machine.

See [control-handoff.md](control-handoff.md) for exact IDs, paths, target JSON and engine integration requirements. Each control repairs **only an explicit unsafe REG_DWORD**. An absent key/value or already-safe value is preserved. Unsupported types and values outside 0/1 are unknown/error, never coerced or normalized. Journals use the existing exact `{present,value}` representation; absent requires `value:null`. Restoration checks that the current value still equals the recorded target, restores the original DWORD or removes only the named value, and re-reads the result. Sibling values and keys are preserved.

“Nondisruptive” here means no forced restart, sign-out, service interruption, account/password change or blanket permissions rewrite. The intended security restrictions still change unsafe behavior; the compatibility effects below are disclosed in the control descriptions.

## Controls and rationale

| Fixed ID | Explicit repair | Rationale and compatibility |
| --- | --- | --- |
| `installer.always_install_elevated` | `AlwaysInstallElevated`: 1 → 0 | Microsoft warns that this policy is equivalent to granting administrative rights. The vulnerable behavior requires both machine and user flags to be 1; disabling the machine flag breaks that conjunction for every user. Users can set their own flag after the machine flag is enabled, so repair does not depend on inspecting the invoking administrator's HKCU. Ordinary administrator-authorized/managed installation remains available; installations relying on this unsafe automatic elevation will require appropriate authorization. No Installer service or other MSI preference is changed. [1] |
| `lsa.restrict_anonymous_sam` | `RestrictAnonymousSAM`: 0 → 1 | Require authentication for SAM account enumeration, reducing account-name harvesting for guessing/social engineering. Microsoft lists Enabled as the effective client default and recommends it. Legacy anonymous enumeration and one-way-trust workflows can be affected; domain-managed machines are excluded. This is not the broader `RestrictAnonymous` policy and does not change share ACLs. Microsoft documents no restart requirement. [2, 5] |
| `lsa.limit_blank_password_use` | `LimitBlankPasswordUse`: 0 → 1 | Restore the documented client baseline: local accounts with blank passwords cannot use covered remote interactive/network logons. Physical console logon is unaffected, and no password contents are read or changed. Blocking remote blank credentials is an intentional security improvement. Domain accounts are not affected; Microsoft notes that some third-party applications may bypass the setting. No restart required. [3, 5] |
| `wdigest.use_logon_credential` | `UseLogonCredential`: 1 → 0 | Disable WDigest credential caching explicitly re-enabled on a supported client. Microsoft documents absence as disabled on Windows 8.1 and later; absence remains untouched. Digest applications may request credentials more often. Stored DWORD readback does **not** demonstrate removal of credentials from running LSASS. Existing sessions may require sign-out/restart; the conservative reboot flag communicates deferred effect without performing either operation. [4, 6] |

## Management and precedence

Repair and restore both execute the existing Windows client/x64 capability and native management gate. Unreadable or unavailable native MDM registration, domain, policy or RSOP probes remain blocking failures. Enrollment templates alone do not mean management, but active registration, domain membership, OMADM accounts and cloud join evidence veto mutations.

Scoped PolicyManager current/provider checks cover Installer (`ADMX_MSI` and `ApplicationManagement`), the individual LSA security-option names, and `MSSecurityGuide/WDigestAuthentication`. Explicit inactive current metadata remains the existing exception; unknown or relevant provider metadata blocks. Unrelated LSA/UAC/security-guide policies do not automatically veto each other.

The machine `AlwaysInstallElevated` preference itself is **not** management evidence: otherwise an explicit unsafe preference could never be repaired, and the tool's own target would prevent rollback. Other machine Installer values/subkeys conservatively veto Installer mutation. Existing `Registry.pol`/`gpt.ini` checks and relevant RSOP settings still take precedence; the new controls also veto local security templates and registry-preference XML artifacts. RSOP matching for the new controls is scoped to their key and value name. No policy file is edited.

The probes establish conservative eligibility, not omniscience about external software. The engine's journal/drift checks and the backend's immediate current-value guards are both required. Every new native DWORD setter/removal is followed by strict registry readback; failure is surfaced so the engine retains recovery state rather than reporting success.

## Advisory and deferred controls

Automatic logon is advisory only. The assessment reads only the nonsecret `AutoAdminLogon` string and enumerates value names to detect `DefaultPassword` presence. It never reads that password, identities, or LSA secrets. Missing Winlogon password data does not prove there is no LSA-stored autologon secret. This avoids breaking kiosk/sign-in workflows and avoids collecting credentials. [7]

`EnableInstallerDetection` is not changed because defaults and applicability differ by environment. `SafeDllSearchMode` is not automatically repaired because application loader compatibility needs separate review. Broader anonymous-access restrictions (`RestrictAnonymous`), service executable/ACL analysis, and blanket ACL repairs are outside these four controls. Dangerous service/permissions findings are integrated separately. Existing UAC, Defender, firewall and legacy journals retain their IDs and schemas.

## Primary Microsoft sources

Fetched directly with `webfetch` on 2026-10-02:

1. [AlwaysInstallElevated (Windows Installer)](https://learn.microsoft.com/en-us/windows/win32/msi/alwaysinstallelevated): REG_DWORD, both HKLM/HKCU flags, administrative-rights warning, managed versus unmanaged installation.
2. [Network access: Do not allow anonymous enumeration of SAM accounts](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/network-access-do-not-allow-anonymous-enumeration-of-sam-accounts): Windows 10/11 applicability, baseline, impact and restart behavior.
3. [Accounts: Limit local account use of blank passwords to console logon only](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/accounts-limit-local-account-use-of-blank-passwords-to-console-logon-only): local/remote distinction, client baseline, policy precedence and application limitations.
4. [Microsoft Security Advisory 2871997 - WDigest settings](https://support.microsoft.com/help/2871997): exact registry path, 0/1 meaning, absent default on Windows 8.1 and later, more frequent credential prompts. The legacy update itself is not installed or required by Secblitz on Windows 10/11.
5. [LocalPoliciesSecurityOptions Policy CSP](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-localpoliciessecurityoptions): scoped LSA policy names and 0/1 values. CSP availability does not redefine the older native registry setting's applicability.
6. [MSSecurityGuide Policy CSP - WDigestAuthentication](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-mssecurityguide#wdigestauthentication): relevant management area and SecGuide ADMX mapping.
7. [Configure Windows to automate logon](https://learn.microsoft.com/en-us/troubleshoot/windows-server/user-profiles-and-logon/turn-on-automatic-logon): Winlogon values, cleartext credential risk and alternative LSA-secret storage.
8. [ADMX_MSI Policy CSP](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-msi): machine Installer policy area and related policies that must be preserved.

## Validation

Linux-only validation performed for this platform change:

- `cargo test platform::tests -- --nocapture`: 5 tests passed, including exact targets and strict original/target schema validation for all four IDs.
- `pwsh -NoLogo -NoProfile -File src/platform/backend.tests.ps1`: 186 common/gate checks plus 334 private privilege-registry checks passed. The script imports production function ASTs only; all registry operations are in-memory doubles.
- Fixtures exercise absent keys/values, safe values, malformed types/ranges, exact rollback and absence restoration, unrelated-value preservation, write/readback mismatches, native-gate failures, local policy artifacts, scoped RSOP/PolicyManager evidence and autologon secret non-disclosure.

The existing Windows-only `backend.binding.tests.ps1` retains forced `-WhatIf` native firewall binding checks and loads the new fixed-spec helper used by `WriteControl`. It was not run on Linux. Windows-native registry/provider behavior and integrated engine/journal tests require the coordinated engine allowlist change and Windows validation. No guest modifications were performed.

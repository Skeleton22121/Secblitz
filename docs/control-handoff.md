# Platform / engine handoff

## Native service-permission gate contract

`pub fn platform::permission_gate(id: &str) -> anyhow::Result<()>` is available for **exactly** `permissions.service.bits` and `permissions.service.wuauserv`. The native permissions wrapper must invoke it during observation before setting `eligible=true`, and again immediately before every DACL repair or restore. Do not cache a prior success. Native service identity/owner/ACL parsing, elevation, narrowly scoped mutation, drift checks and readback verification remain the wrapper's responsibility.

The platform's `controls()` remains **16**; only the native wrapper adds the two service controls. These IDs are accepted exclusively by the fixed PowerShell `permission_gate` action, never platform `observe`/`write`. No value payload is accepted. Success must be exactly `{"ok":true}`; Unix and all probe/transport/acknowledgment failures return errors.

The gate repeats Windows client/x64, domain, native MDM, cloud/OMADM, policy-artifact and RSOP checks. It uses SystemServices plus the corresponding BITS/Update policy areas, never Firewall. Any resultant policy-setting instance vetoes service mutation, including security/service settings under a LocalGPO; an empty LocalGPO alone is permitted. Only the established exact missing-RSOP-namespace exception remains. No guessed `RSOP_SystemService` class or invalid-class bypass is used.

### i18n handoff: new fixed gate messages

Please cover these exact production messages (the platform does not edit the localization catalog):

- `Service permission eligibility requires Windows`
- `Unknown service permission control id`
- `Invalid platform action arguments`
- `Service permission gate was not acknowledged`
- `Computer Group Policy evidence: service permissions are assessment only`
- `Applied computer policy settings: service permissions are assessment only`
- `Local computer service policy artifacts: assessment only`
- `Invalid service permission gate request`
- `Service permissions require the native wrapper`

Initial coordinated check: all 45 Rust library tests passed, Windows GNU `cargo check --all-targets` passed; main tests failed only the two translation-coverage tests for these new messages. PowerShell fixtures passed 186 common + 334 registry + 248 service-gate checks. Native Windows tests are not executed on this Linux host; findings count has been corrected from 13 to 14 in the native read-only test.

Follow-up after the concurrent native wrapper/engine work landed: **56 library tests passed**, **13/15 main tests passed** (the same two gate-message localization failures), and Windows GNU all-target checking passed again. Platform-only rustfmt checking passed. Whole-repository formatting and strict Windows Clippy reported concurrent engine/permissions formatting and permissions `manual_is_multiple_of` warnings; those files are owned separately. Detailed authority review: [platform-gate-review.md](platform-gate-review.md).

## Registry controls

The platform adds these **four fixed IDs** to the existing twelve controls. Engine coordination is required: extend its typed allowlist and explicit-disabled eligibility rules; do not assume all registry targets are 1 or all repairable originals are 0.

| ID | HKLM key | DWORD name | Repairable original | Target | reboot flag |
| --- | --- | --- | --- | --- | --- |
| `installer.always_install_elevated` | `SOFTWARE\Policies\Microsoft\Windows\Installer` | `AlwaysInstallElevated` | 1 | 0 | false |
| `lsa.restrict_anonymous_sam` | `SYSTEM\CurrentControlSet\Control\Lsa` | `RestrictAnonymousSAM` | 0 | 1 | false |
| `lsa.limit_blank_password_use` | `SYSTEM\CurrentControlSet\Control\Lsa` | `LimitBlankPasswordUse` | 0 | 1 | false |
| `wdigest.use_logon_credential` | `SYSTEM\CurrentControlSet\Control\SecurityProviders\WDigest` | `UseLogonCredential` | 1 | 0 | true |

Every target is exactly `{"present":true,"value":0}` or `{"present":true,"value":1}` as listed. Every journal original uses the existing strict registry schema: exactly `present: bool` and `value: u32|null`; present values accept **only 0 or 1**, absent requires explicit null. Preserve absence and already-safe settings (ineligible for repair). Restore is permitted only from the current target, including restoration to absence. Retain legacy journal compatibility and all twelve existing IDs/targets.

The installer machine flag alone breaks the vulnerable machine/user conjunction. No HKCU mutation. Its own registry target is not evidence of policy authority: native domain/MDM checks, scoped PolicyManager evidence, policy artifacts and RSOP still veto. All gates apply on repair and restore, and probe failures stay fail-closed. WDigest readback proves stored configuration only; no claim of clearing existing LSASS credentials. Reboot flag communicates deferred effect; the tool never restarts Windows or signs users out.

Implementation scope: `src/platform.rs`, `src/platform/backend.ps1`, backend tests and private test files, plus this handoff and `docs/privilege-escalation.md`. Engine and dangerous service/ACL findings integration are owned separately. No guest changes.

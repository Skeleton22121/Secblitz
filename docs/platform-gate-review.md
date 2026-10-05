# Platform gate review and service-permission integration

Review date: 2026-10-02. Scope: platform request validation, native PowerShell launch/acknowledgment boundary, shared management gates, new service-policy branch, existing registry guards and platform test integration. This is a code/fixture review on Linux, not a Windows guest validation or a review of the native ACL parser's correctness.

## Interface and ownership

`platform::permission_gate(&str) -> anyhow::Result<()>` accepts exactly:

- `permissions.service.bits`
- `permissions.service.wuauserv`

The gate is a read-only management/authority check. Its Windows implementation calls the fixed `permission_gate` action through the existing inbox PowerShell transport and requires the exact JSON acknowledgment `{"ok":true}`. Unix returns an explicit unsupported-platform error. No target value, service name, script or registry path can be supplied. The platform catalog remains 16 controls. The separately owned native permissions adapter adds its two controls and performs service identity/owner/ACL validation, mutation and exact readback.

At review time the native wrapper calls this interface before accepting observation eligibility, before opening the write handle, and again immediately before its final descriptor drift check and native DACL setter. These call sites were inspected, not edited by the platform work. All restores must use the same write path; a previously successful observation does not authorize a later mutation.

## Review findings and disposition

| Area | Review result / implemented protection |
| --- | --- |
| Action/ID confusion | The prior launcher checked IDs against one catalog independently of the action. `validate_request` now validates the entire action/ID/value combination. Service IDs are accepted only for `permission_gate`, with no payload; they are rejected for `observe`, `write`, `machine`, and `findings`. Existing 16 control requests retain their typed validation. |
| Case/injection aliases | Rust exact matches and PowerShell case-sensitive service mapping reject uppercase aliases, arbitrary services, suffixes, whitespace and executable strings. Dispatcher action matching is case-sensitive too. |
| Independent PowerShell isolation | `PermissionGate` validates the exact ID and rejects data before any native probes. `ReadControl` and `WriteControl` explicitly reject permission IDs rather than reaching the legacy UAC/default setter. |
| Native transport | Reuses the trusted Windows-directory inbox interpreter, cleared environment, fixed module roots, private script pipe, bounded job, 90-second timeout, 2 MiB output bound and error/exit rejection. The new gate introduces no alternate launch mechanism or child executable. |
| Success acknowledgment | Exact object equality rejects false, null, numeric/string truthiness, omitted fields, arrays and extra fields. Any native/transport/deserialization failure is an error. |
| Capability and domain | Every invocation repeats the existing Windows client, supported build and 64-bit process checks, plus readable Boolean domain membership. Domain membership blocks mutation. |
| Native MDM availability | Every invocation repeats `IsDeviceRegisteredWithManagement`. Registered state, nonzero HRESULT, invalid registration flags/types and unavailable API remain blocking. No service-control exception bypasses this probe. |
| Enrollment/cloud evidence | OMADM accounts and CloudDomainJoin evidence independently veto even when the MDM API returns unregistered. Inbox enrollment/provider templates alone remain insufficient evidence. |
| Policy fall-through | Service IDs get an explicit policy branch: `SystemServices`, plus `ADMX_BITS` for BITS or `Update`/`ADMX_WindowsUpdate` for wuauserv. They never use Firewall/Defender capability or policy probes. |
| Current/provider authority | Existing strict PolicyManager metadata rules apply. Relevant current values without explicit inactive metadata, contradictions, and staged/provider values veto. The entire SystemServices area is conservatively relevant. Corresponding machine BITS/WindowsUpdate policy trees also veto when populated. These are policy stores, not the service object's own DACL, so a successful DACL repair does not itself create a policy veto. |
| Local policy artifacts | Both service IDs retain machine `Registry.pol` and `gpt.ini` vetoes and add machine `Microsoft\Windows NT\SecEdit\GptTmpl.inf`, registry-preference XML and service-preference XML vetoes. The paths are rooted in the trusted Windows directory; file presence blocks and access errors propagate. No file is parsed, rewritten or applied. |
| GPO containers | A malformed/unreadable GPO ID or Boolean metadata is unknown. For permission controls, any nonlocal GPO container conservatively vetoes, including disabled/filtered remnants. Merely finding an otherwise readable LocalGPO is insufficient to veto. |
| Actual local policy | After GPO enumeration, service controls enumerate `RSOP_PolicySetting` in `root\rsop\computer`. It is Microsoft's documented base for extension policy settings; any returned derived instance vetoes. Thus an actual service/security setting, other nonempty policy, orphaned setting or malformed result cannot be mistaken for an empty LocalGPO. No guessed `RSOP_SystemService` query is necessary. |
| RSOP errors | Only the existing exact missing namespace result from the initial GPO query is treated as absence, after all independent gates. Access denied, invalid class/provider, and a namespace disappearing during the subsequent policy-setting query remain failures. |
| Registry-control regression | `PermissionService` returns no mapping for all 16 ordinary IDs, leaving their scoped policy logic and target schemas intact. The four new binary controls still preserve absent keys/values and safe settings, reject wrong kinds/ranges, guard repair/restore transitions and re-read setters/removals. |
| Findings regression | The native read-only findings test expected 13 before the automatic-logon advisory was added. It now expects 14; service advisories belong to the separate native wrapper. |

## Why enumerate the base policy class?

Microsoft describes `RSOP_PolicySetting` as the abstract base for client-side extension policy classes, with each derived instance corresponding to an actual setting. `RSOP_GPO` is a container and can represent applied, denied or disabled policy. Checking only a GPO container count is therefore too broad for a clean empty LocalGPO and too narrow to establish which settings exist.

The requested service mutation rule is conservative: permit an empty local policy, refuse actual nonempty policy. Enumerating actual settings through the documented base avoids depending on an unverified service-specific class name, and avoids an unsafe “invalid class means no policy” fallback. It intentionally blocks even policy settings unrelated to these two services once they appear in the resultant computer policy store. PolicyManager SystemServices also uses a conservative area-wide veto; the published CSP is primarily startup-mode policy, not a complete service-DACL management schema.

## Evidence and remaining integration checks

- PowerShell fixtures: **186 common/gate + 334 binary-registry + 248 permission-gate checks passed**. All run production AST functions with native boundaries mocked. The permission fixtures execute the actual new dispatcher clause, demonstrate repeated native MDM calls, and fail immediately on accidental Firewall/Defender native probes.
- Shared Rust validation tests exercise both accepted IDs, cross-action rejection, missing IDs, unexpected payloads, uppercase/whitespace/NUL/injection input, exact acknowledgment shape, all 16 legacy/static-control request shapes, and Unix failure behavior.
- Initial full Rust run: **45 library tests passed; 13 of 15 main tests passed**. Two catalog-coverage tests failed for new fixed gate messages; those exact messages are recorded in [control-handoff.md](control-handoff.md) for the localization owner.
- Follow-up after the concurrent native wrapper and engine changes: **56 library tests passed; 13 of 15 main tests passed** with the same two localization failures. The platform's six tests all passed, including exact action isolation; engine/permission tests also passed in that run.
- Windows GNU `cargo check --all-targets` passed using `/tmp/opencode/secblitz-cross-env.sh`. This compiles Windows branches/tests but does not execute native probes.
- Broad formatting and strict Clippy checks exposed concurrent work outside this platform scope: formatting in engine/permissions files and `manual_is_multiple_of` findings in permissions descriptor/state code. Platform formatting corrections are applied via patches; no other owner's files are reformatted.
- Focused `rustfmt --edition 2021 --check --config skip_children=true src/platform.rs src/platform/windows.rs` passed using the supplied cross environment. Windows GNU all-target checking passed again after the wrapper integration.
- Windows PowerShell 5.1 binding/native RSOP/MDM behavior still requires Windows execution. The native firewall binding fixture imports the two fixed helpers now called by `WriteControl`; its native setters remain forced to `-WhatIf`.

No guest changes or permission mutations were performed. A gate success is a point-in-time authority check, not a lock against future management changes. The wrapper's immediate repeat and descriptor drift/readback checks are required, but cannot make external policy management and an SCM DACL update one atomic transaction.

## Primary Microsoft references fetched for this review

- [RSOP_PolicySetting class](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/policy/rsop-policysetting): extension-class inheritance, actual policy instances and computer namespace.
- [RSOP_GPO class](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/policy/rsop-gpo): GPO categories, identity, access and filter metadata.
- [SystemServices Policy CSP](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-systemservices): published System Services policy area and Security Settings/System Services mapping.
- [Enumerating WMI](https://learn.microsoft.com/en-us/windows/win32/wmisdk/enumerating-wmi): local class-instance enumeration and authority/access limitations.

Registry control rationale and Microsoft references remain in [privilege-escalation.md](privilege-escalation.md).

## Independent agent 2/4 follow-up

Reviewed the owned platform files again on 2026-10-02. This follow-up changed only `backend.ps1`, `backend.tests.ps1`, `backend.privilege.tests.ps1` and this report.

### Substantiated fix

The shared Defender gate used PowerShell truthiness for `AMServiceEnabled` and `AntivirusEnabled`; nonempty strings such as `"False"` could pass as enabled. Post-write checks used coercing comparisons, which could accept `"True"` or numeric `1`. Both locations now require actual `[bool]` values before checking their enabled state. Null/string/numeric regression cases cover both fields at observation gating and after writes for every Defender control. Runtime feature-status fields already required Boolean values; the fix retains those checks. Existing diagnostic messages are reused, so this fix adds no localization keys.

### Registry, service and findings conclusions

- Independently pinned all four registry specifications in fixtures to their literal HKLM path, value name, unsafe original and target; the assertions do not derive their expected paths from the production specification. Absence remains absent, safe values remain ineligible, wrong types/ranges fail, and repair/restore both run gates and verify the resulting named value. Restore to absence removes only the value. No HKCU access or caller-selected path is introduced.
- UAC consent remains a `0 -> 5` repair only; nonzero settings are preserved. Installer/WDigest targets remain zero, unlike the two LSA targets. WDigest readback is explicitly stored configuration rather than proof of clearing credentials from running LSASS.
- Service gate IDs remain exact and payload-free. Rust requires exactly `{"ok":true}`; Boolean/string/numeric substitutes and additional fields fail. The wrapper's current observation and repeated pre-write calls were inspected read-only. Gate success does not establish effective access or substitute for native identity, owner, transition, drift and readback checks.
- Service PolicyManager areas remain `SystemServices` plus BITS or Update areas, not Firewall. Inbox enrollment templates and provider/default catalogs are not automatically management evidence. Native registration, cloud/OMADM evidence, relevant applied/provider policy and exact machine policy-artifact paths are checked independently. The initial `root\rsop\computer` query permits only the documented exact invalid-namespace exception; invalid class/access/provider failures and a namespace disappearing later still fail. Any resultant base-policy instance vetoes service mutation, while an empty LocalGPO is permitted.
- Automatic-logon fixtures now reject every value-data read except `AutoAdminLogon`, including username/domain/password data, and assert the exact Winlogon HKLM path. They cover enabled, disabled, absent, invalid flag and wrong-kind cases. This remains an advisory: disabled/absent states return `info`, not a credential-security success claim; malformed values produce `unknown`. LSA secrets are not inspected.
- Catalog accounting remains **16 platform controls**, **14 platform findings**, and **19 findings with the five native service advisories**. The existing Windows-only platform read-only test correctly expects 14, not 13 or 19.

### Checks executed in this follow-up

- `/tmp/opencode/secblitz-powershell/pwsh -NoLogo -NoProfile -NonInteractive -File src/platform/backend.tests.ps1`: **226 common + 348 registry + 248 service-gate checks passed**. These are mocked host-side fixtures, not native Windows execution.
- With `source /tmp/opencode/secblitz-cross-env.sh`: **59 Rust library tests passed**; `cargo check --target x86_64-pc-windows-gnu --all-targets` passed; focused platform rustfmt checking passed.
- `cargo test --bin secblitz`: **13 passed, 2 failed**. Both failures are localization coverage: `backend_fixed_errors_titles_and_advice_are_translated` and `fixed_rust_diagnostic_prose_has_catalog_coverage`. The latter currently reports engine and native-permissions prose as well as platform gate messages. These are not runtime gate successes or failures.

Localization handoff remains the nine exact platform messages listed in `control-handoff.md`, including `Service permission gate was not acknowledged` and the service policy vetoes. The Boolean fix reuses `Defender unavailable, passive, or tamper protected: assessment only` and `Defender became unavailable or passive; mutation outcome requires review`. Effective firewall mismatch handling retains `Firewall preference/effective readback did not match; mutation outcome requires review`; no effective-state success is inferred from stored `NotConfigured`.

No Windows guest command, native binder fixture, native PowerShell test, service mutation or registry mutation was executed by this follow-up. Native Windows 10/11 behavior and actual guest selection remain with the coordinating runtime owner. Cross-compilation and mocked fixtures do not validate those native outcomes.

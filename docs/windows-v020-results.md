# Secblitz 0.2.0 - native Windows 11 results

Date: 2026-10-02. **Eligible runtime repairs, exact-state rollback, conflict/retry, and the exact packaged installer passed. BITS repair was correctly refused on this guest's unsupported baseline ACE flags; it is not a demonstrated repair success.**

Only `Secblitz-W11-Test` was modified. Windows 11 Enterprise Evaluation build 26200, x64, all eight network adapters disabled, no shared folders. **Windows 10 was not tested.** No Rust, PowerShell backend, or installer source was edited by the runtime tester. This document supersedes neither the historical v0.1 artifacts nor their hashes; it records the separate v0.2.0 artifact below.

## Snapshot and scope

- Powered-off snapshot: `secblitz-pre-expanded-v020`.
- Snapshot UUID: `c680e300-1a4a-47e8-b048-7777855b67df`.
- Release binary tested: SHA-256 `b6c7a8514b4a56fb1514ebc3e0b873acd6eb649603ec64d3fa620fa22280079f`.
- Actual surface: **18 controls** (16 platform controls plus 2 fixed service-DACL controls), **19 findings**.
- No exploit/payload, service reconfiguration command, credential reading, new account, password logon, or actual privilege escalation was performed. Registry-fixture operations used one existing guest-control session and child processes inheriting its token. No reboot occurred while unsafe registry values were staged.

## Native tests / preflight

| Check | Result |
| --- | --- |
| Windows library test executable | **70 passed**, exit 0 |
| Windows CLI test executable | **16 passed**, exit 0 |
| Italian CLI help | exit 0 |
| Baseline audit | Valid JSON, exit 2; review findings retained |
| Windows Update service identity / DACL gate | Eligible |
| BITS service identity / DACL gate | Preserved/skipped: baseline contains a flagged ACE |

The first baseline BITS ACE has flag **0x02 (ContainerInherit)**. Production rejects flagged/unsupported ACE semantics for automatic repair. That flag was not removed, normalized, or bypassed to manufacture eligibility. Both service identity paths were otherwise observed through the native code; no service type/account/path workaround was applied.

## Four registry controls - actual mutation and exact restoration

Original value presence, key presence, DWORD type and exact little-endian bytes were captured independently before mutation. Only fixed names in `docs/control-handoff.md` were accessed.

| Control | Original baseline | Deliberate fixture | Applied | Tool revert |
| --- | --- | --- | --- | --- |
| Machine AlwaysInstallElevated | Absent | DWORD 1 | DWORD 0 | Exact DWORD 1 |
| RestrictAnonymousSAM | DWORD 1 | DWORD 0 | DWORD 1 | Exact DWORD 0 |
| LimitBlankPasswordUse | DWORD 1 | DWORD 0 | DWORD 1 | Exact DWORD 0 |
| WDigest UseLogonCredential | Absent | DWORD 1 | DWORD 0 | Exact DWORD 1 |

All four apply/revert results were independently verified. The final cleanup restored the actual baseline, including **absence** for AlwaysInstallElevated and WDigest, exact original DWORD bytes for the other two, and removal of a fixture-created empty key when originally absent. HKCU was not mutated. WDigest results explicitly reported restart required: only stored configuration was tested, not post-reboot behavior or clearing existing credentials. LSASS/credential contents were never queried.

The normal all-controls apply also changed the three firewall persistent inbound defaults from NotConfigured to Block; tool revert restored NotConfigured exactly. **Eight controls actually changed**: these three, four registry controls, and Windows Update's DACL. No claim is made that all 18 controls changed.

## Native service-DACL fixtures and independent access simulation

Fixtures were installed with documented `QueryServiceObjectSecurity` / `SetServiceObjectSecurity` APIs against only **BITS** and **wuauserv**. Queries requested OWNER|GROUP|DACL (7); writes requested **DACL only (4)**. SACL was neither requested nor set. Baseline owner/group bytes, descriptor flags, DACL bytes and ACE order were saved outside the repository before any fixture.

Each fixture appended an explicit Everyone ALLOW ACE with mask **0x00060002**:

- `SERVICE_CHANGE_CONFIG` (0x00000002), dangerous;
- `WRITE_DAC` (0x00040000), dangerous;
- `READ_CONTROL` (0x00020000), deliberately benign to verify preservation.

Independent `AuthzAccessCheck` evaluations used a synthetic standard-user SID with explicit Everyone, Authenticated Users, Builtin Users and Interactive groups. No administrator group/privileges, token impersonation, real account creation or credentialed logon was used. These are **simulated effective-access checks**, not actual attempts to change service configuration or escalate privileges. Independent broad-ALLOW mask counting is limited to these two service objects and is not a system-wide vulnerability count.

| Stage | BITS risky broad ACE count / simulated two rights | Windows Update risky broad ACE count / simulated two rights |
| --- | --- | --- |
| Original baseline | 0 / both denied | 0 / both denied |
| Unsafe fixture | 1 / both allowed | 1 / both allowed |
| After Secblitz apply | **1 / both allowed: correctly skipped** | **0 / both denied: repaired** |
| After tool revert | 1 / both allowed; unchanged exact fixture | 1 / both allowed; exact fixture restored |
| After final manual fixture cleanup | 0 / both denied | 0 / both denied |

The Windows Update ACE changed **0x00060002 → 0x00020000**. Independent byte checks confirmed every pre-existing ACE remained byte-for-byte unchanged, benign READ_CONTROL remained, and owner/group/descriptor flags were preserved. Revert restored the exact unsafe fixture DACL bytes. BITS retained its exact fixture throughout automatic apply/revert and was restored manually to baseline in cleanup. **Automatic BITS repair is an explicit environment/coverage limitation**, not a silent success.

## Idempotence, drift, retry and final restoration

- Applied twice: complete 18-control snapshots after both calls match exactly.
- Reverted twice: both snapshots match the staged fixture-before image exactly, including service DACL bytes, owner/group and flags.
- Applied again, then appended a **benign QUERY_STATUS-only** Everyone ACE to Windows Update. Revert reported `permissions.service.wuauserv` as **conflict**, preserved the drifted DACL and restored the other changed controls.
- Returned Windows Update to the recorded applied DACL using the fixture harness; explicit retry restored its original staged unsafe DACL exactly. No force/bypass option was used in Secblitz.
- Manual `finally` cleanup restored unsafe registry values first, then both service DACLs and original firewall preferences. **All 18 final controls match the baseline snapshot byte-for-byte.**
- History returned exit 0; all five retained transactions, including earlier-version history, are **reverted**. No pending transaction remains.
- Fixture audit/apply/repeated apply/revert/repeated revert/drift apply/conflict revert/retry all returned **exit 2 with valid JSON**, reflecting review/advisory/skipped/conflict statuses. No operational exit 1 occurred.

## Exact v0.2.0 installer and monitor

Compiled the current unmodified `installer/setup.iss` with official Inno Setup **6.7.3**, `/DAppVersion=0.2.0`, the exact guest-tested executable and an explicit output directory.

| Stage | Result |
| --- | --- |
| ISCC compilation | **exit 0**, six language resources included |
| Fresh optional-monitor setup, `/LANG=it /TASKS=monitor` | **exit 0** |
| Fresh service state | Stopped, as intended |
| Installed executable | SHA-256 matches the tested binary |
| Start monitor and await a new report | Running; schema 1, **18 observations, 19 findings, incomplete=false** |
| Uninstall while monitor running | **exit 0** |
| Final installation state | No installed application executable, no monitor registration; report retained |
| Actual transaction journal files | Every file hash unchanged before install versus after uninstall |
| Preferences / service descriptors | All 18 after-package snapshots match original pre-fixture baseline exactly |

### Italian custom-message correction - final targeted retest passed

The initial package emitted two missing-Italian-custom-message warnings. The coordinating agent added `it.Monitor` and `it.Failed`; no executable or maintenance logic changed. The runtime tester rebuilt the installer using the unchanged executable hash above and verified both exact Italian entries in the source extracted inside the guest. **ISCC compilation returned 0 with no warnings or missing-translation diagnostics.**

The **final installer bytes** were then tested with `/LANG=it /TASKS=monitor`: fresh installation exit **0**, service registered and correctly **Stopped**, installed executable hash unchanged, uninstall exit **0**, no service/application executable remaining. All 18 captures still match the pre-fixture baseline exactly, and every actual journal-file hash is unchanged. The monitor scan and running-uninstall tests in the preceding table were not needlessly repeated for this two-string-only change; their executable remains identical. A complete scan is not an assertion that all findings are healthy or accessible.

Final targeted evidence: `/tmp/opencode/secblitz-v020-italian-fixed/` and `secblitz-v020-italian-fixed.zip`, including source-message capture, clean compiler log, install/uninstall logs, unchanged installed hash, all-18 captures and final state. The older package hash `336783ce3f59393a58ecaea3939848cdc6cc6677afc8a22cb5d51e5090db7d22` is historical; the updated hash below identifies the fixed final installer. No source code was edited by the runtime tester.

## Artifacts and hashes

Current `dist/SHA256SUMS` covers **only the exact v0.2.0 artifacts**, and verification passed:

```text
b6c7a8514b4a56fb1514ebc3e0b873acd6eb649603ec64d3fa620fa22280079f  secblitz.exe
df0b0e81056176f7391cac6b566ca55b48b54995091af3029a6a2045d7a5348c  secblitz-0.2.0-windows-x64-setup.exe
```

The previous v0.1.0 executable, installer and matching checksum file are preserved in **`dist/archive/0.1.0/`**, independently checksum-verified. The old versioned installer also remains in dist for compatibility; it is not included in the new checksum file. All artifacts are unsigned GNU-cross-built development artifacts, not signed/native-MSVC release provenance.

## Evidence / final VM state

Raw nonsecret test captures remain outside the repository:

- `/tmp/opencode/secblitz-v020-preflight/`
- `/tmp/opencode/secblitz-v020-runtime/` and `secblitz-v020-runtime.zip`
- `secblitz-v020-runtime/independent-dacl-verification.json`
- `/tmp/opencode/secblitz-v020-package/` and `secblitz-v020-package.zip`
- Repro harnesses: `/tmp/opencode/secblitz-v020-native.ps1`, `secblitz-v020-runtime.ps1`, `secblitz-v020-verify.ps1`, `secblitz-v020-package.ps1`.

Final clone state: **running**, all eight NICs disabled, no shared folders, no installed Secblitz executable/service, no unsafe fixture or unresolved transaction. Defender/tamper protection stayed enabled. Real reverted journals, monitor reports and earlier unrelated-file preservation fixtures remain intentionally. No original/other VM was modified. The snapshot remains available; it was not restored after this passing run so the real v0.2.0 journal evidence remains. Expired evaluation licensing remains a possible interruption for future long tests. **Windows 10 is untested.**

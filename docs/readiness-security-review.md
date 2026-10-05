# Readiness security review - independent 0.5 review 2/4

Reviewed 2026-10-03. Owned scope: `src/readiness.rs`,
`src/readiness/windows.rs`, and this review. Native execution is pending on the
main Windows agent; Linux execution and Windows x64 cross-linking are recorded
below. No model, UI, platform, updater, service, or credential files were edited.

## Findings and changes

1. **All-unknown power was falsely tagged Known - fixed.** A successful native
   call with unknown AC and unknown/invalid battery flags now produces overall
   `Unknown`. Partial valid facts remain usable. BatteryFlag 255 is checked
   before 128; no-battery never has a percentage. Only 0–100 is a percentage;
   AC values other than 0/1 remain unknown.
2. **Remote reparse traversal preceded the remote-volume rejection - fixed
   conservatively.** `GetVolumePathNameW` follows junction chains. A DOS drive
   prefix and an initial `DRIVE_FIXED` check alone did not prevent traversal of
   an attacker-controlled journal path to a remote target. The native collector
   now opens the drive root and each existing directory component with
   `OPEN_EXISTING`, `FILE_READ_ATTRIBUTES`, `FILE_FLAG_OPEN_REPARSE_POINT`, and
   `FILE_FLAG_BACKUP_SEMANTICS`, sharing only read access. It rejects reparse
   points before opening children and retains ancestor handles through the
   volume queries. It never follows a newly created missing leaf: it queries
   the last existing, pinned ancestor instead. Sharing/access/API failures are
   Unknown. Reserved DOS device components are rejected too.
3. **HKLM ProgID lookup was not itself a proof of safe COM activation -
   constrained and documented.** The worker checks the process integrity token
   before initializing COM. Only high/system integrity proceeds. Low, medium,
   medium-plus, and unreadable tokens return Unknown, preventing even
   same-privilege loading of an HKCU override. The bounded HKLM REG_SZ lookup is
   additionally pinned to the SDK's SystemInformation CLSID. It cannot select
   an unrelated machine-registered COM class.

Directory-mounted volumes, junctions, symbolic links, and other reparse paths
now conservatively yield Unknown, including legitimate local ones. Ordinary
local fixed-drive paths still use `GetVolumePathNameW` and `GetDriveTypeW` before
space/flags queries. This availability tradeoff preserves the no-remote-traversal
boundary; it is not evidence that a mounted volume is unhealthy.

## COM trust proof and supported operation

Microsoft's [UAC: COM Per-User Configuration][com-uac] states:

> Beginning with Windows Vista® and Windows Server® 2008, if the integrity level
> of a process is higher than Medium, the COM runtime ignores per-user COM
> configuration and accesses only per-machine COM configuration.

This is the relevant documented rule for high-integrity administrators and
system-integrity SYSTEM/service processes, rather than an inference from a
successful test. The current [COM elevation documentation][com-elevation] also
states that a UAC-elevated process does not load per-user classes. The
[HKCR merged-view documentation][hkcr] separately covers administrators with UAC
disabled. The implementation requires at least high integrity, a conservative
subset of the documented condition. Account names or environment variables are
not used to infer integrity. The new worker has no inherited thread
impersonation token; it queries its process token.

The application still trusts the OS and administrator-controlled machine COM
registration. It is not defending against a malicious administrator, modified
Windows binaries, or already-injected code. No registry-hijack fixture was
installed or executed during this review.

The supported WUA operation is
[`ISystemInformation.RebootRequired`][wua-reboot], reached through the documented
[`Microsoft.Update.SystemInfo` coclass][wua-system]. Activation is exclusively
`CLSCTX_INPROC_SERVER`, with no aggregation, shell, moniker, ProgID creation,
remote activation, update session, Search, download, install, or settings write.
The GUID is `C01B9BA0-BEA7-41BA-B604-D0A36F469133`, verified against Microsoft's
[`wuapi.h`][sdk]. The IDispatch IID is
`00020400-0000-0000-C000-000000000046`; Invoke uses IID_NULL and property-get only.

Existing PowerShell WUA usage elsewhere does not establish this native helper's
security. The new in-process activation is independently restricted as above;
the helper never calls the PowerShell backend.

## Unsafe ABI and ownership audit

| Area | Result |
| --- | --- |
| Power | SDK `SYSTEM_POWER_STATUS`, 12 bytes, zero-initialized; failure discards output. |
| Volume sizes | SDK `GetDiskFreeSpaceExW` first output is caller/quota-aware `u64`; it is not total free space. Rust/serde preserve values above 4 GiB. |
| Volume flags | Initialized `u32`; failure returns Unknown even when available bytes succeeded. A failed call cannot become `read_only=false`. `FILE_READ_ONLY_VOLUME` is `0x00080000`. |
| Paths | Native UTF-16 stays UTF-16; no environment fallback. Absolute DOS drive paths only; embedded NUL, UNC, device prefixes, ADS, relative/dot components, wildcards, and reserved device components rejected. |
| Known folder | `SHGetKnownFolderPath(FOLDERID_ProgramData, KF_FLAG_DONT_VERIFY)` without CREATE. The API's allocated NUL-terminated-string contract is required; bounded copy does not invent an allocation length. `CoTaskMemFree` runs exactly once, including a non-null failure output. |
| Handles | Every successful directory/token handle has RAII `CloseHandle`; no ownership/ACL repair. Failed handles are never wrapped. Directory metadata stays internal. |
| Integrity token | Query-only token handle; aligned, initialized buffer with exact byte capacity. Returned length and SID pointer range checked before reading the S-1-16-RID bytes. Failure blocks activation. |
| Registry | `RegGetValueW` uses the fixed HKLM Classes path and process-native registry view, REG_SZ only, 80-byte buffer; requires the 78-byte braced GUID representation and expected CLSID. No registry handles are returned or leaked. |
| Dispatch | `repr(C)`, seven slots in IUnknown/IDispatch order, `extern "system"`, 32-bit HRESULT/DISPID/LCID/ULONG, 16-bit invocation flags. One successful activation reference, one Release on every property exit. |
| VARIANT | SDK layout, 24 bytes on x64 / 16 on x86; initialized VT_EMPTY. Only exact VT_BOOL is read, accepting VARIANT_FALSE=0 and VARIANT_TRUE=-1. Unexpected type or Boolean encoding is Unknown. `VariantClear` precedes apartment teardown. Only x64 was cross-built. |
| EXCEPINFO | SDK layout, zero-initialized; all three returned BSTRs freed with `SysFreeString`. No error strings escape, and deferred exception text is not requested. |
| Apartment | Successful CoInitializeEx (including S_FALSE) has exactly one CoUninitialize on that worker. RPC_E_CHANGED_MODE borrows the existing apartment without uninitializing it. Other failures stop the probe. Objects/results drop before the apartment. |

Readiness is evidence, not journal authorization. No ownership or writable-ACL
claim is made from volume facts. Existing platform journal owner, DACL, reparse,
and handle checks must still run independently before mutation; Unknown does not
override them. Advisory/storage-blocking policy belongs to the model/engine
owners and was not changed here.

## Bounded worker and read-only scope

The WUA wait is at most two seconds, excluding scheduling overhead. This is not
a two-second deadline for the whole collector or local kernel filesystem calls.
One global slot retains a timed-out worker receiver. Repeated/concurrent calls
cannot accumulate outstanding COM probes. A permanently stuck OS call retains
at most one worker and its native resources until process exit; forcibly killing
that thread would be unsafe. A late result is discarded before a fresh query.
Native objects/apartment are released before completion is published. Losing
the receiver does not bypass cleanup. Spawn, contention, disconnect, and poison
failures yield Unknown.

The helper requests no file/directory creation, registry mutation, child process,
network operation, or WUA search/update operation. File opens are OPEN_EXISTING
metadata queries. COM queries can have OS-internal logging/cache behavior;
absence of every OS-internal write is not established by a source audit. The
native trace described below must distinguish those from application mutations.

The returned schema contains only counts, booleans, and Unknown states. Paths,
usernames, SIDs, hostnames, registry contents, volume identifiers, and native
error strings are not copied into reports. No new diagnostic text is introduced
by these files. The other owners should preserve `None` when there are no errors
and avoid adding user-identifying paths to any copy/export diagnostic.

## Validation performed

Commands used `source target/build-tools/cross-env.sh` and offline dependencies.

- `cargo test --offline readiness::`: **9 passed**. Covers independent failures,
  known false vs Unknown, power sentinels, path rejection, failed volume flags,
  zero/low quota and >4 GiB, serde round trips, 100 calls while a worker remains
  outstanding, stale-result discard, lock contention, and lost-receiver cleanup.
- `cargo test --offline`: library **128 passed, 1 ignored**; CLI/UI **62 passed,
  2 failed, 2 ignored** at the time of this shared-worktree check. Failures outside
  this scope: `ui::tests::readiness_is_informational_wraps_and_does_not_inflate_protection_totals`
  (Option unwrap at `src/ui.rs:828`) and
  `i18n::tests::fixed_rust_diagnostic_prose_has_catalog_coverage` (missing coverage
  for `{gb} GB free` and `Apply {} effective protection is unverified; pending
  transaction {} retained`). Hand off to their owners; do not interpret as a
  complete-suite pass.
- `cargo test --offline --target x86_64-pc-windows-gnu --no-run`: **passed**,
  library and CLI test binaries linked. Includes native ABI assertions, pinned
  registry-string parsing cases, and a fake IDispatch exercising correct
  property selection, valid/invalid return types, HRESULT failures, and exactly
  one Release. This is compile/link evidence, not Windows runtime evidence.

## Windows-agent follow-up

1. Run `readiness::windows::tests` on native x64 Windows, then explicitly run
   `readiness::windows::tests::native_readonly_smoke` with
   `--exact --ignored --nocapture`. The smoke fixture only calls the helper and
   prints its path-free JSON. Run as standard user, elevated administrator,
   SYSTEM, and LocalService. Expect WUA Unknown at medium/low integrity; record
   actual behavior at high/system integrity without treating Unknown as false.
2. Exercise the helper directly with a minimal SYSTEM environment. Confirm
   Windows-directory and known-folder results without substituting a fake
   GetKnownFolder response, inferred `C:\\ProgramData`, or environment fallback.
   Full CLI environment/backend initialization failures are a separate finding,
   not evidence about this helper's native APIs.
3. Use controlled native fixtures for absent journal leaves, inaccessible
   directories, local directory mounts, remote symlinks/junction chains, and
   concurrent reparse/rename attempts. Reparse paths must be Unknown without
   opening their target. Check handle counts after repeated success/failure.
4. Check real quota/read-only volumes and power transitions. Confirm caller quota
   semantics, large counters, and partial/unknown output against native APIs.
5. Trace the elevated/SYSTEM COM activation with a controlled per-user override
   fixture prepared by the Windows test owner. Confirm the override DLL is not
   loaded; for standard users confirm no activation occurs. Observe no helper
   process launch, remote traversal, update search, or app-state mutation.

Call-site audit: `WindowsBackend::readiness` delegates to this collector, and the
engine collects it for audit and apply readiness. Guided checks flow through
that engine. The subsequent background-scanner integration is now reviewed:
the LocalService scanner **does call readiness**, as detailed below. CLI updater
check/status/install-staged route to the updater and do not invoke this backend
collector. Changes to broader status/environment reporting belong to their
respective owners.

## Background-scanner integration follow-up - approved at source level

Reviewed the updated `src/service/windows.rs::scan` and its delegation,
serialization, and service-loop boundaries on 2026-10-03. **No confirmed security
bug found in this call-site addition.** This follow-up changes documentation
only. Native service execution remains pending; earlier test results above are
historical results, not a rerun or runtime certification of this integration.

- After backend construction, `scan` calls `backend.readiness()` once before
  control observations, unless stop is already requested. The permissions
  adapter forwards the call directly to `WindowsBackend::readiness` and the
  reviewed native collector. The new call does not invoke writes, eligibility
  changes, repair decisions, or settings operations.
- Root JSON `readiness` is an Option: `null` when not attempted (stop or backend
  construction failure), otherwise an object containing independently tagged
  Known/Unknown probes. Unknown is a completed observation with unavailable
  evidence. It does not by itself set `incomplete=true`, and `incomplete=false`
  is scan completion, **not a health score or proof of protection**. Existing
  backend/observation/findings failures and budget/stop handling can still set
  incomplete. A stopped service loop discards the finished scan instead of
  publishing it over the previous report.
- The configured service account remains `NT AUTHORITY\LocalService`. Its name
  does not establish its actual integrity level, nor grant WUA access. The
  collector reads the process token and requires a validated S-1-16 integrity
  RID of at least high (`0x3000`). Unreadable/lower integrity returns Unknown
  before COM initialization. At high/system integrity, denied WUA activation
  or property access also returns Unknown; there is no HKCU, PowerShell,
  alternate-class, or elevation fallback. Record the real service token during
  native validation rather than assuming LocalService is high/system.
- `BUDGET` remains five minutes, with its timer starting before backend creation
  and readiness. Readiness time therefore counts toward later budget checks.
  The new entry guard checks stop, not elapsed budget; subsequent control and
  findings phases check both. The two-second bound is the COM receiver wait,
  with at most one outstanding native COM worker across scans. Neither BUDGET
  nor stop preempts a synchronous volume/kernel call, and stop arriving after
  entry does not cancel readiness. This integration does not establish a hard
  whole-scan or whole-collector deadline.
- Successful control rows now include typed optional `effective` and `authority`
  evidence from the existing observation. Missing values serialize as `null`;
  neither is synthesized from eligibility, reason text, or readiness. These
  bounded enum/Boolean fields introduce no path or user-identifier payload.
- Readiness and the additional evidence fields are inside the same capped
  serialization writer: the entire encoded report remains limited to 64 KiB.
  Overflow returns the existing small `incomplete=true`, `status=unknown`
  summary, rather than truncated JSON or an unbounded report. The service loop
  checks the byte limit again before its existing protected report-file write.
  That existing reporting write is distinct from the read-only collector.
  No ACL, account, scheduling, or report-path change is required by this addition.

### Targeted native service-report test plan

For the main runtime agent, using the existing service install/start procedure
and controlled Windows test fixtures:

1. **Actual service context and schema:** install/start the service as configured
   LocalService, verify its real process account and integrity without copying
   user-identifying token data into the report, and inspect the next completed
   `Monitor/latest.json`. Confirm root `readiness` is an object when attempted,
   all four probes retain their tagged schema, and successful observation rows
   contain typed `effective`/`authority` values or null. No all-clear assertion
   may be inferred from `incomplete=false`.
2. **Expected LocalService denial:** exercise a host/controlled fixture where WUA
   access is denied, unavailable, or the token gate cannot pass. Expect
   `windows_update_reboot: {"status":"unknown"}`, not Known(false), a scanner
   crash, privilege escalation, or an alternate activation path. Confirm other
   facts/observations remain independent. With other phases completing normally,
   this Unknown alone must not make the report incomplete. Distinguish separate
   observation/access failures that legitimately do make it incomplete.
3. **Skipped vs unknown:** in the native scan fixture, arrange stop before entry
   and backend-construction failure separately. Expect null readiness and no
   readiness invocation. Verify stop suppresses publication in the actual
   service loop. A completed all-Unknown collector must instead serialize an
   object; null must not be substituted for unknown measurements.
4. **Slow WUA and stop:** use a controlled blocking probe fixture to verify a
   roughly two-second COM wait, repeated scans with only one outstanding COM
   worker, late-result discard, and eventual cleanup. Request stop before and
   during readiness. Check service stop checkpoints and no post-stop report
   publication; do not assert a two-second bound on unrelated filesystem calls.
   Test a budget-expired scan fixture to confirm later phases are skipped and
   marked incomplete without extending/resetting the timer.
5. **Size and privacy:** in a service-report serialization fixture, place the
   encoded report near and above 64 KiB, including readiness and control
   metadata. Require complete valid JSON at or below the cap, or the explicit
   overflow summary; include escaping/multibyte text in existing string fields.
   Verify large `available_bytes` values survive and the new fields contain no
   paths, native error strings, or user identifiers.
6. **Side effects:** trace the new readiness phase separately from the scanner's
   pre-existing observation/findings subprocesses and report-file write. Require
   no readiness-triggered child process, network/update search, settings write,
   directory/registry creation, or per-user COM DLL load. Confirm the service
   configuration, ACLs, and scan interval remain as installed.

[com-uac]: https://learn.microsoft.com/en-us/previous-versions/bb756926(v=msdn.10)
[com-elevation]: https://learn.microsoft.com/en-us/windows/win32/com/the-com-elevation-moniker
[hkcr]: https://learn.microsoft.com/en-us/windows/win32/sysinfo/merged-view-of-hkey-classes-root
[wua-system]: https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nn-wuapi-isysteminformation
[wua-reboot]: https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-isysteminformation-get_rebootrequired
[sdk]: https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/wuapi.h

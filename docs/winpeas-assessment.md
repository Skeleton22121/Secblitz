# Scoped winPEAS assessment - baseline availability

Date: 2026-10-02. Scope: defensive permission/privilege-escalation configuration measurement on **a disposable Windows 11 VM only**. No exploit or payload was executed, and no privilege escalation was attempted.

## Current result: benchmark blocked; Defender restored and verified

| Measurement | Observed result |
| --- | --- |
| Official executable retrieved and hash checked | Yes |
| Attempt with Defender enabled | Launch did not start; executable subsequently removed |
| Matching Defender detections | **1** |
| Matching detections reporting action success | **1** |
| Supported real-time-protection disable request | Command completed; **effective protection did not turn off** |
| Interactive desktop available for supported UI | **No** logged-on interactive desktop user |
| Completed winPEAS help/check processes | **0** |
| Weak-permission / policy finding counts | **Unavailable**, not zero |
| Standard-user baseline | Not run; no disposable account created yet |
| After cleanup | Pre-assessment snapshot restored; independently verified original protection state |

The supported `Set-MpPreference -DisableRealtimeMonitoring $true` call returned without an error, but `Get-MpComputerStatus.RealTimeProtectionEnabled` stayed **true**, `Get-MpPreference.DisableRealtimeMonitoring` stayed **false**, and tamper protection stayed **true**. This is a refused/ineffective disable request, not an antivirus-off run. No exclusions, service changes, registry bypasses, altered/obfuscated binary, or tamper bypass were used. A supported interactive UI change would require an actual desktop session; none was available in this headless guest. Existing credential contents were not read or reported.

## Tool provenance

- Official repository: [peass-ng/PEASS-ng](https://github.com/peass-ng/PEASS-ng).
- Pinned release: [20261002-82d9fad1](https://github.com/peass-ng/PEASS-ng/releases/tag/20261002-82d9fad1), published 2026-10-02.
- Asset: non-obfuscated **winPEASx64.exe**, 11,352,576 bytes.
- SHA-256: **`c2b1b403f0dc71768d944100e86fa93213e5eb7c396d9b23f0fb57db3a81a973`**.
- Downloaded on the host with `gh release download`; measured hash exactly matched the release API asset digest. This establishes matching release bytes, not a separate publisher signature/reproducible-build claim.
- Executable retained outside the repository; parent directory mode **0700**. Host executable was never run. No guest network or shared folder was enabled.

## Reviewed invocation and collection limits

The release's README, argument parser, `PrintUsage`, and SystemInfo/ServicesInfo source were inspected. Native `-h` execution was attempted first but blocked before a help process could start. Exact source-verified narrow invocation for a later supported run:

```text
winPEASx64.exe servicesinfo notcolor quiet dont-check-hostname
```

`servicesinfo` selects the service module; `notcolor` avoids ANSI setup, `quiet` suppresses the banner, and `dont-check-hostname` disables the default external hostname check. File search, browser, Windows-credentials, registry-credential, event-history, cloud, network-scan, package-online-lookup and domain-enumeration switches are not selected. Guest NICs remain disabled independently of flags.

**Do not run bundled `systeminfo` for this task.** In this pinned release it includes `PrintRegistryCreds`, environment-variable collection, transcript discovery and other collection outside the requested no-secrets scope. Its MITRE option filters whole modules, not individual functions, so it does not safely narrow that module. Relevant machine policy values should instead be queried individually through the forthcoming native interface, without reading credential values.

Service output can still contain local names/paths. Keep any future raw output in the restricted external evidence directory and publish only category counts/statuses. Run as a disposable **standard-user token** for useful permission findings: administrator-writable service results are not evidence of a standard-user escalation path. No standard account or service fixture was created in this blocked baseline phase; prepare it only after the next interface is coordinated. A read-only service access-rights probe suffices - do not start an exploit service or test payload.

## Snapshot and independent cleanup verification

- Snapshot created while powered off: **secblitz-pre-winpeas**.
- Snapshot UUID: `e2fd9e4d-8174-4dba-86f8-78b867566d86`.
- Requested supported restoration with `Set-MpPreference -DisableRealtimeMonitoring $false`, then shut down the clone cleanly, restored that snapshot, and restarted only the clone.
- Independent post-restore capture exactly matches the pre-assessment capture:
  - Antivirus, antimalware service, real-time, behavior and IOAV protection **true**.
  - Tamper protection **true**.
  - DisableRealtimeMonitoring, DisableBehaviorMonitoring, DisableIOAVProtection and DisableArchiveScanning **false**.
- Original BASE and all other VMs were neither modified nor started/stopped. The clone is returned running with all eight NICs disabled and no shared folders. Snapshot restoration removes the transferred tool and assessment-side guest changes while retaining the earlier application-test baseline/journals.

## Evidence and next coordination

Nonsecret captures outside the repository:

- `protection-before.json`
- `enabled-help-attempt.json`
- `supported-disable-attempt.json`
- `blocked-summary.json`
- `protection-after-restore.json`

No winPEAS findings output exists because execution was blocked. The next 16-control implementation has **not** been benchmarked. Once a supported interactive protection configuration and the new interface are available, repeat the identical narrow standard-user measurement before/after, restore the snapshot, and independently recheck protection. WinPEAS heuristic counts describe configuration indicators; they do **not** prove or quantify malware prevention or successful privilege escalation.

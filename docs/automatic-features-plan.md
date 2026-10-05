# Automatic features - 0.5.0 implementation plan

## Release gate and scope

**Implementation authorized after deployed 0.4.3 security fixes and genuine 0.4.2 to 0.4.3 E2E passed.** Engine ownership: `src/model.rs`, `src/engine.rs`, `src/permissions.rs`, and integration notes in this file. No VM work.

Deliver effective-firewall assessment, automatic verification, readiness checks, and **Fix recommended**, following [hardening research](hardening-research.md). Its historical “no selectable plan” limitation is superseded by existing guided checkboxes/`apply_selected`.

Inspected model, engine, platform/native/PowerShell, permission wrappers, UI/advice, and guided Session. Audit currently compares raw targets; guide requires manual rescanning after apply/undo. Existing write readback is not a fresh assessment.

## Typed contracts and effective protection

Choose **two optional typed fields**, not arbitrary effective JSON:

```rust
// Both Observation fields: #[serde(default, skip_serializing_if = "Option::is_none")]
effective: Option<EffectiveFirewall>,
authority: Option<Authority>,
// serde snake_case enums:
// EffectiveFirewall::{Enabled(bool), Inbound(InboundAction)}
// InboundAction::{Block, Allow}; Authority::{Local, Managed, Unknown}
```

Preserve raw `value`; `None` means unverified. Keep `Backend::observe` signature; add shared `validate_observation(id, &Observation)` at Windows decoding and Engine's trait boundary. Only six compiled firewall IDs accept matching effective variants. Managed/Unknown cannot accompany `eligible=true`; metadata grants no eligibility. Non-firewall producers initially use `None`.

PowerShell independently reads ActiveStore, validates one matching profile and running BFE/MpsSvc, and emits typed gate authority without parsing exception messages. Management yields Managed; probe failures yield Unknown. Preserve write gates.

Inbound audit: eligible Local + raw `NotConfigured` + verified effective Block becomes `compliant`/Protected. Eligible effective Allow recommends Block. Explicit Local Allow remains a candidate unless ineligible; contradictory/missing evidence requires review. Managed ActiveStore Block/local Allow stays `skipped`/**Managed elsewhere**, never written. Unverified defaults never become Protected. Enabled checks also require matching effective evidence; this proves profile settings, not rule-level reachability.

Before transaction creation/Prepare, the same default predicate returns `unchanged`: zero writes, new WALs, or touched profiles. Recheck before writes; post-Prepare races retain recovery intent. Owned-control conflicts and undo compare **raw exact values**.

Add optional effective/authority fields to `Outcome`; advice accepts `&Outcome`, using typed firewall evidence before legacy fallbacks. Preserve status strings/IDs. Migrate every affected struct literal/test constructor; serde defaults only cover deserialization. **Schema 1, static targets, before-images, and restore payloads stay identical.**

## Fresh verification and approval lifecycle

Keep mutation APIs. Guided Session adds `OperationAttempt { result: Result<Report>, events }` and `AssessmentSnapshot { generation, report }`. Invalidate the snapshot before approved apply/undo; retain its result/error, then audit automatically once on Ok or Err, before rendering can short-circuit verification. No automatic retries/undo/reboots.

Successful audit cannot erase partial failure or pending WAL. Keep mutation/verification errors separately; callbacks are not durable success. Failed audit leaves no actionable snapshot; exclude individual probe errors. Pending recovery blocks batches. Storage failure preventing audit means verification unavailable, never bypassed journal checks.

Bind approval to snapshot generation; refreshes/operations/state-changing extras invalidate it. Refresh readiness before execution; Engine reobserves selected IDs under lock. Newly appearing candidates require approval. UAC/WDigest deferred effects remain restart-needed.

## Read-only repair readiness

Add `Backend::readiness() -> Readiness`, default all-Unknown; forward through **both** PermissionBackend and AuditedBackend. Fixed fields: `system_volume`, `journal_volume`, `power`, `windows_update_reboot`. Frozen contract: `Probe<T> = Known(T) | Unknown`, with a unit Unknown variant (no code field), tagged `status`, content `value`, snake_case. Volume fields are `available_bytes: u64`/`read_only: bool`; power fields are `ac_connected: Option<bool>`, `battery_percent: Option<u8>`, `battery_present: Option<bool>`; reboot is bool. Independent failures retain other evidence. No labels, serials, accounts, or secrets.

Add `Report.readiness: Option<Readiness>` at assessment/pre-apply. Resolve trusted local system/journal volumes; reject remote paths before querying; no write-test files. Microsoft APIs reviewed 2026-10-03:

| Signal | Local read-only API / interpretation |
| --- | --- |
| Available disk | [GetDiskFreeSpaceExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw): caller-available bytes, quota-aware, never truncated |
| Volume flags | [GetVolumeInformationW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationw): `FILE_READ_ONLY_VOLUME`; clear flag does not prove write permission |
| Battery/power | `GetSystemPowerStatus`, [SYSTEM_POWER_STATUS](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-system_power_status): battery flag 128 = absent/not-applicable; 255 = unknown, checked before bit tests |
| Update restart | [ISystemInformation::get_RebootRequired](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-isysteminformation-get_rebootrequired): local `Microsoft.Update.SystemInfo`; no update search/download; false is not “fully patched” |

Statuses: Ready/Notice/Blocked/Unknown/NotApplicable, **not repairs**. Confirmed read-only system/journal volume or zero journal space blocks new apply before WAL; other space/power/reboot evidence is informational. Existing storage/eligibility/undo gates remain authoritative. Show one compact readiness line plus details, excluded from Protected/NeedsChoice totals. Absent battery is not green protection; failed probes stay Unknown.

## Fix recommended

Add **Fix recommended** beside **Choose what to fix**. Both use one candidate builder: fresh `attention` outcomes, compiled available IDs, repair advice, no pending recovery. Recommended opens the exact batch review directly; **Change selection** uses existing individual checkboxes. One explicit approval invokes `apply_selected` once, then automatic verification. Decline/Esc/EOF changes nothing; empty plans do nothing. Preserve restart/compatibility copy and Undo.

No extra actions enter this batch. Existing explicitly consented updaters retain their network behavior; assessment adds no network calls or DNS changes. Conditional Defender signature updates can be a later separately consented outside-WAL step using the existing action. No AV expansion, SFC/DISM repairs, broad app repairs, passwords, or irreversible remediation here; those need a later phase orchestrator.

## Four-agent ownership and integration

After security handoff, freeze contracts first, then parallelize:

1. **Engine/contracts:** `model.rs`, `engine.rs`, `permissions.rs`; enums, validation, report plumbing, wrapper forwarding, raw-state/WAL invariants and engine tests. Land constructor migration atomically across owners before parallel edits.
2. **Windows evidence:** `platform.rs`, `platform/windows.rs`, `platform/backend.ps1`, associated PS tests; effective/typed authority emission and readiness delegation.
3. **Readiness module:** new `readiness.rs`/`readiness/windows.rs`, module export and necessary Cargo features; bounded native probes, normalization, injected failure tests. Expose a helper to agent 2; do not edit its files.
4. **Guided experience:** `guided.rs`, `ui.rs`, `advice.rs`, `i18n.rs`; batch review, generations, verification lifecycle, informational readiness rendering and session tests.

Agent 1 integrates after all four, rechecking the post-security baseline. Shared-file changes require owner handoff, not concurrent edits. No agents are launched during this design task.

## Acceptance tests

- Replay saved/default firewall cases as sanitized typed fixtures: all three PersistentStore NotConfigured/ActiveStore Block profiles yield **0 unneeded default-firewall fixes**, writes, new WALs, or touched profiles.
- Genuine Local Allow/ActiveStore Allow recommends Block; managed ActiveStore Block/local Allow reports Managed elsewhere, with zero writes. Missing/malformed/wrong-ID evidence cannot claim Protected.
- Actual NotConfigured→Block→undo restores exact NotConfigured; existing schema-1 journals still load, owned-state drift still conflicts, effective state never replaces originals.
- Approved apply/undo, no-op, partial failure, and unknown write outcome each trigger exactly one fresh assessment. Failed verification blocks stale selection; successful verification preserves mutation failure and pending recovery. Newly appearing candidates never inherit consent.
- Disk counters above 4 GiB, quotas, read-only volume, access denial, zero bytes, AC/battery unknown/absent, and WUA failure/true/false normalize correctly; informational rows add zero NeedsChoice/Protected counts. Probe spies verify no mutation/network calls.
- One recommended approval means one exact selected batch; individual omissions, cancellation, recovery blocks, and ExtraActions isolation remain intact. Run targeted Rust and mocked PS suites, then full Rust checks and Windows-target compilation after implementation; no VM execution in this task.

## Frozen engine integration contract (0.5.0)

- Definitions are available in `src/model.rs`. `EffectiveFirewall` uses `#[serde(tag = "kind", content = "value", rename_all = "snake_case")]`; `Enabled(bool)` and `Inbound(InboundAction)` are the only variants. `InboundAction::{Block, Allow}` and `Authority::{Local, Managed, Unknown}` serialize snake_case.
- `Observation` derives Default, with optional defaulted/omitted `effective` and `authority`. `validate_observation(id, &obs)` is public and shared. Default raw `value` is null, so fixture callers still supply their actual raw value.
- `Probe<T>` defaults to the unit Unknown. `Readiness` derives Default with all probes Unknown. `Backend::readiness()` returns Readiness directly, not Result, and both permission adapters forward exactly once.
- `Outcome` and `Report` derive Default. Outcome adds optional omitted `effective`/`authority`; Report adds optional omitted `readiness`. Other owners must migrate literals (for example with `..Default::default()`). No WAL/schema/restore-payload changes.
- Progress uses `("readiness", "pending")` then `("readiness", "complete")` for audit and pre-apply readiness. `readiness` is a phase identifier, not a selectable control. Audit collects readiness once. New-apply readiness is refreshed under the operation lock, before any new WAL record; known read-only system/journal or zero journal bytes returns skipped outcomes with readiness evidence. Undo has no readiness gate.
- Translation handoff: `Firewall evidence is invalid for this control`; `Firewall evidence does not match the control`; `Nonlocal firewall authority cannot be eligible`; `Firewall authority is unavailable`; `Effective firewall evidence is unavailable`; `Firewall evidence contradicts the local preference`; `Repair readiness blocks new changes`. UI should label the `readiness` progress phase. Unknown evidence is never a Protected result.

### Engine implementation and verification notes

The engine now uses typed effective evidence only for fresh firewall assessment and new-apply eligibility. Eligible Local NotConfigured/Block observations produce compliant audit rows and unchanged apply rows without creating a transaction. Explicit local settings must agree with their effective evidence. Managed/Unknown cannot authorize writes; missing Local evidence or contradictory explicit settings yields review/error. Outcomes carry validated observation metadata, including actual readback metadata after successful writes.

Owned targets and undo still compare exact raw schema-1 values. Effective Block cannot hide raw drift from Block to NotConfigured. The pre-write gate reobserves eligibility/evidence; a default becoming protected after Prepare fails with pending recovery instead of silently skipping or sealing. Existing ACL original-derived target checks and updater reserved-entry validation remain intact.

Readiness defaults do not block apply. Only confirmed read-only system/journal volumes or zero caller-available journal bytes block. The engine collects no network diagnostics, performs no readiness write tests, and adds no readiness check to undo. New apply reports include the refreshed readiness; pending/owned-conflict early returns do not initiate new writes and may omit it.

Regressions added cover all three inherited default-Block profiles (no writes/WAL), genuine Allow and NotConfigured gaps with exact raw undo, missing/contradictory/managed evidence, typed wrong-ID/wrong-kind rejection, raw owned drift, post-Prepare evidence loss/default change, readiness refresh/blocking/informational signals and undo, and exactly-once forwarding through both permission adapters.

Final owned-scope checks with `target/build-tools/cross-env.sh`:

- Host library suite passed: **124 passed, 0 failed, 1 intentionally ignored live public updater audit**; concurrent readiness/platform tests included.
- Host library Clippy with `-D warnings` passed; owned-file rustfmt check passed.
- Windows GNU library Clippy/compilation remains blocked at `src/permissions/windows.rs:242`: the external owner must migrate the Observation construction with `..Observation::default()`. No Windows build success is claimed for this change until that migration lands.
- Binary test compilation reported missing fields in `src/main.rs:1121` (Report) and `:1143` (Outcome); the main owner must migrate those literals with defaults. These constructor sites are outside this task's ownership.
- No live network audit or VM operation was invoked.

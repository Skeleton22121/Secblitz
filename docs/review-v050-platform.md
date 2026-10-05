# v0.5.0 independent platform review, owner 3 of 4

## Scope

Reviewed `src/platform.rs`, `src/platform/windows.rs`, and
`src/platform/backend*.ps1`. Corrections are confined to the PowerShell backend
and its fixtures. This review adds no enum, dependency, Cargo feature, VM
configuration, or changes to another owner's source files.

The Rust boundary already validates raw values and calls
`model::validate_observation` after decoding. Firewall metadata is scoped to the
six exact firewall IDs. Enabled metadata remains a JSON boolean; inbound
metadata accepts only the typed block/allow variants. Non-firewall observations
do not gain these fields. `WindowsBackend::readiness` delegates to the readiness
module API. The bounded interpreter timeout, normal process flags, fixed
permission-control entry points, and Defender verification remain intact.

## Findings and corrections

1. **Wrapped management authority could be lost.** Observation previously read
   only the outer exception's `Data`. Added `GateAuthority`, which checks the
   exception data first and follows `InnerException` and native PowerShell
   `ErrorRecord` wrappers. Traversal is bounded and cycle-aware. Only the exact
   string marker written by `ThrowGate` establishes managed authority. Exception
   messages, access failures, and untagged wrappers remain unknown. Tests include
   a wrapper whose only tagged evidence is in its nested error record.
2. **The mutation gate accepted weaker ActiveStore evidence.** It did not require
   exactly one matching profile and allowed an unresolved inbound action.
   `Gate` now uses the same `ReadEffectiveFirewall` validation as observation.
   That helper requires one correctly named profile, concrete enabled/inbound
   flags, and running MpsSvc/BFE services. Missing, duplicate, malformed, wrong
   profile, NotConfigured effective action, and service failures fail closed.
3. **An earlier snapshot could outlive its authority checks.** Successful local
   observations refresh effective evidence after `Gate`. Firewall apply and
   restore call a fresh `ObserveControl` immediately before the setter, checking
   current raw/effective consistency and management authority again. Readback
   also uses the strict effective-profile helper within the existing bounded
   retry loop. The earlier engine snapshot is never used as write permission.
4. **Pipeline output must not contaminate JSON.** Module bootstrap, `Load`,
   gate calls at observation/write/permission entry points, and the firewall
   setter explicitly suppress success-stream output. Exception traversal emits
   only its authority result. Fixtures inject dictionary output from module
   import and setter mocks and require clean output. Observe dispatch still
   emits exactly one JSON string.

Confirmed policy classification remains evidence-based and scoped. Applied
domain/MDM/cloud evidence, relevant configured policy paths, active current
provider metadata, and relevant resultant policy can establish managed
authority. Provider catalogs, ambiguous staged metadata, local policy artifacts,
foreign security providers, and failed probes veto mutation without being
misclassified as local or proven managed. Relevant registry dictionary keys
remain explicitly enumerated with `.Keys`.

Raw `NotConfigured` is preserved in observations and restoration values.
Effective Block does not rewrite that before-image. Explicit Allow with effective
Block becomes ineligible/unknown rather than being normalized. ActiveStore
NotConfigured does not invent Block, and stopped MpsSvc does not claim effective
protection. Boolean writes are checked before enum-token conversion; the existing
True/False binder contract remains unchanged.

## Verification

Executed locally with the persistent PowerShell 7.4.13 executable at
`/tmp/opencode/secblitz-powershell-7.4.13/pwsh`. All Windows probes and setters in
the executed PowerShell fixtures are mocked.

| Check | Result |
| --- | --- |
| `pwsh -NoLogo -NoProfile -NonInteractive -File src/platform/backend.tests.ps1` | Passed: platform 226, privilege 348, permission 248, firewall 551 checks |
| Firewall veto and observation fixtures | `insecureSettingChanges=0`; all malformed-count/profile, late-policy, late-effective, contradiction, and invalid-boolean cases stop before the setter |
| Real firewall preflight with mock setter | Five apply/restore calls passed, preserving raw inbound values and enum binder tokens; no success-stream leakage |
| `cargo test --offline platform::tests` | 8 passed |
| `cargo test --offline firewall` | 6 library and 2 UI tests passed |
| `cargo test --offline all_default_block_profiles_are_protected_without_preference_or_wal_changes` | Passed: all three inherited Block profiles unchanged, no writes or transaction |
| `cargo check --offline --target x86_64-pc-windows-gnu --tests` | Blocked outside owned files: `src/readiness/windows.rs:393` compares `GUID` with `==`, but windows-sys 0.59 GUID does not implement PartialEq |

Rust commands used `source target/build-tools/cross-env.sh`. The existing
Windows-only binder fixture was updated to mock the new preflight/readback
dependencies, but was not executed. No live Windows WriteControl, native setter,
VM test, or PowerShell 5.1 execution was performed in this review.

## Integration notes and remaining limits

- Readiness owner: resolve the GUID field comparison compile error and rerun the
  Windows target check. Native compilation and runtime behavior are not certified
  by the passing Linux-host fixtures.
- Owner 4: retain localized presentation for `EffectiveFirewallUnavailable` and
  `EffectiveFirewallMismatch`. Review presentation of the boundary diagnostic
  `Firewall enabled value must be a boolean`; no CLI or i18n source was edited.
- Engine integration tests confirm inherited Block means unchanged and that raw
  schema-1 originals survive repair/undo. Metadata does not replace journal values.
- Repeated probes are snapshots, not an atomic compare-and-write API. Policy,
  service, or preference changes can still race the final check and setter.
  Strict readback detects observable inconsistencies; it cannot eliminate that
  operating-system race or prove that no transient change occurred.

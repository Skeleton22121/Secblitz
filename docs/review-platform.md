# Platform review - independent wave 1, agent 1

## Native firewall binder follow-up (2026-10-02)

The main runtime report in `docs/windows-test-results.md` establishes a real
PowerShell 5.1 setter defect: NetSecurity's `GpoBoolean` parameter rejects a
`System.Boolean`. The production change is deliberately limited to mapping the
already validated boolean into the literal enum-name token `True` or `False`
when assembling `Set-NetFirewallProfile -Enabled`. The same branch handles
apply and restore. Eligibility, before-images, readback and timeout behavior
are unchanged.

The portable firewall mock now rejects boolean/non-token Enabled arguments.
Added `src/platform/backend.binding.tests.ps1`, a Windows-only native binder
regression test. It loads production `WriteControl`, simulates only its gate
and readback, and forwards its assembled arguments to the actual inbox
NetSecurity setter with **`-WhatIf` forcibly supplied** for both true and false.
It compares all three profiles' enabled/inbound/outbound snapshots from both
PersistentStore and ActiveStore before and after. No real setter is called
without WhatIf.

Executed results:

- Linux PowerShell fixtures: **138 passed**.
- Windows PowerShell 5.1 fixtures on `Secblitz-W11-Test`: **138 passed**.
- Native binder test on that clone: **both enum tokens accepted**, two WhatIf
  operations reported, all profile snapshots unchanged.
- `cargo test --locked`: **49 passed** (36 library + 13 CLI), zero doctests.
  The owning reviewer has supplied the earlier scoped-policy i18n coverage.
- `cargo build --locked --release --target x86_64-pc-windows-gnu`: **passed**.
- Windows library-test cross-compilation: **passed**.

Release artifact:
`/tmp/opencode/secblitz-windows-target/x86_64-pc-windows-gnu/release/secblitz.exe`

SHA-256: `f126d1ee17901a2a80145ef7c729c5c4fcb8e71b1ab996920e9a55d0a754e25c`.
No `dist` artifacts were replaced by this reviewer.

The previous failing candidate had already changed Domain/Private inbound from
NotConfigured to Block before reaching this binder failure. Per the main
runtime evidence, **tool revert successfully restored those fields to
NotConfigured and closed the retained transaction as reverted**. Independent
all-12-control capture after cleanup matched the original baseline, including
exact UAC consent DWORD bytes; tamper protection stayed enabled. That is real
partial-transaction recovery evidence, not a passing full apply/idempotence/
revert/drift run. The main runtime reviewer will rerun that full sequence with
this corrected artifact. This follow-up performed only fixtures and read-only/
WhatIf guest checks.

## Wave 3 - scoped policy authority and firewall verification (2026-10-02)

This section supersedes earlier blanket PolicyManager/RSoP gating descriptions.
Repository edits in this wave are limited to `src/platform/backend.ps1`,
`src/platform/backend.tests.ps1`, and this document. The main/service reviewer
retains ownership of guest preference mutations and apply/revert integration.

### Read-only guest evidence

Inspected `docs/windows-test-results.md` and queried only `Secblitz-W11-Test`.
No management artifacts, firewall preferences, UAC values, enrollment, or service
state were changed. Diagnostic scripts were copied under Windows Temp.

- Relevant `PolicyManager\current\device` areas `LocalPoliciesSecurityOptions`,
  `Defender`, and `Firewall` are absent. The populated provider areas observed
  are `default\device\knobs`, containing power/energy/processor defaults and
  last-write metadata, not relevant applied security policy.
- RSoP has one `LocalGPO`, version 0, enabled, accessible, filter allowed, no
  extension IDs. Registry policy/security-setting queries are empty. Neither
  machine `Registry.pol` nor `GroupPolicy\gpt.ini` exists.
- `Get-NetFirewallProfile -PolicyStore RSOP` succeeds with zero objects. The
  same query with `-Name Public` throws the exact
  `CmdletizationQuery_NotFound_Name,Get-NetFirewallProfile` error. Active Public
  is enabled/Block/Allow; Persistent Public is enabled/NotConfigured/NotConfigured.
- A deliberately absent WMI namespace returns a genuine `CimException` with
  `NativeErrorCode=3`, generic CLR HRESULT `-2146233088`. The new classifier
  recognizes that exact native missing-namespace status. A HRESULT-only test
  against `0x8004100E` would miss this real CimCmdlets representation.

Read-only production observations after the changes:

| Control | Result |
| --- | --- |
| UAC enabled / consent | Expected preserve reason for existing nonzero values; management gate passed |
| Public firewall enabled / inbound | `eligible=true`, current values correctly returned |
| Defender realtime | Still ineligible because unavailable/passive/tamper-protected capability gate |

This demonstrates usable eligibility, **not** a completed real mutation/rollback.
Main must run the reversible fixtures; this reviewer did not perform those writes.

### Scheme implemented

1. **Device-wide management vetoes remain:** native MDM registration, domain,
   OMADM accounts, cloud join, and failures to establish their states.
2. **Policy evidence is scoped:** UAC checks `UserAccountControl_*` in
   `LocalPoliciesSecurityOptions`; Defender checks its Defender/ADMX areas and
   Defender policy registry tree; firewall checks its area, WindowsFirewall
   policy tree and firewall MDM store. An unrelated Defender policy no longer
   vetoes UAC/firewall. Other areas such as provider power knobs are not treated
   as relevant security policy.
3. **Provider existence is not applied policy:** inspect relevant per-provider
   device areas, not every value in the entire provider tree. A relevant staged
   or orphaned provider value remains a conservative veto. In current metadata,
   only an explicit DWORD `_ProviderSet=0` with an absent or empty-string winning
   provider is accepted as inactive. A configured/default-valued policy,
   nonzero/unknown marker, nonempty winner, inconsistent type, or missing
   relevant authority metadata fails closed. No provider GUID is hardcoded as
   universally trusted. Registry access failures propagate.
4. **RSoP records are not assumed to be applied settings:** accessible LocalGPO
   containers are followed by actual `RSOP_RegistryPolicySetting.registryKey`
   and `RSOP_RegistryValue.Path` checks for the affected family. Enabled,
   filter-allowed nonlocal GPOs remain a device-wide veto; inaccessible/unknown
   GPO metadata fails closed. Relevant resultant settings veto mutation.
5. **Only exact missing namespace is optional:** MI result 3 or native WMI
   `WBEM_E_INVALID_NAMESPACE` from a typed native exception permits absence,
   after the other management checks. Access denied, invalid class, provider
   failure, and generic errors are not absence. Local policy artifacts still
   conservatively veto changes, including when the namespace is missing.
6. **Firewall RSoP absence:** enumerate the complete RSoP store without `-Name`,
   then inspect the requested profile. A successful empty enumeration is no
   policy; no failed firewall provider query is silently converted to absence.
7. **Firewall acknowledgement:** verify PersistentStore plus ActiveStore using
   bounded polling for enabled/disabled and explicit Block/Allow writes. A
   successful setter with ineffective runtime state now fails. Restore to
   `NotConfigured` must read back that exact stored preference and a concrete
   effective Block/Allow result; it never writes an invented default or replaces
   the before image with the current effective action.

### Research and boundaries

Official references:

- [Policy CSP](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-configuration-service-provider): distinguishes per-source configuration from the resulting policy enforced on the device.
- [LocalPoliciesSecurityOptions](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-localpoliciessecurityoptions): documented UAC policy area and scope.
- [Defender policy result class](https://learn.microsoft.com/en-us/windows/win32/dmwmibridgeprov/mdm-policy-result01-defender02): documented Defender policy area; the device WMI bridge is in the local-system partition, so it is not substituted blindly for an elevated-user probe.
- [RSOP_GPO](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/policy/rsop-gpo): explicitly includes applied, inaccessible and disabled GPO records; presence/count alone is insufficient.
- [WMI error constants](https://learn.microsoft.com/en-us/windows/win32/wmisdk/wmi-error-constants): exact invalid-namespace, access-denied, invalid-class and provider-failure distinctions.
- [Get-NetFirewallProfile](https://learn.microsoft.com/en-us/powershell/module/netsecurity/get-netfirewallprofile?view=windowsserver2025-ps): documented PersistentStore, ActiveStore and RSoP semantics.

The official CSP contract does **not** publish a stable exhaustive meaning for
every internal PolicyManager provider registry value/GUID. We do not claim it
does. The provider-default conclusion here is supported by read-only guest
evidence and relevant-area scoping, not an undocumented blanket GUID exemption.
Internal current metadata is supplemental; ambiguous relevant entries block
rather than establish absence. Windows Home/Windows 10 were not available for
native verification in this wave. Local policy artifact files remain a broad,
conservative veto until their complete source/authority can be established.
RSoP may be stale; management probes and setting writes are not atomic against
concurrent privileged policy changes. A restored NotConfigured preference does
not prove historical effective state equality (that is not in the WAL).

### Tests and build

- PowerShell fixture suite: **138 checks passed on Linux and native Windows
  PowerShell 5.1**. All OS setters are mocked. Added cases cover power-provider
  baseline, inactive/current versus enforced/unknown policy, unrelated Defender
  policy, relevant provider residue, policy access failure, empty LocalGPO,
  relevant UAC RSoP, missing namespace versus access/class/provider errors,
  policy artifacts with absent namespace, firewall empty/resultant/error stores,
  successful/no-op/ineffective writes, and NotConfigured restoration.
- Linux library tests: **36 passed**.
- Full Linux `cargo test`: library passed; CLI **11 passed, 1 failed** on the
  expected missing i18n coverage for newly added platform prose. Main was
  notified; this reviewer did not edit its catalog.
- Full Windows GNU debug build: **passed**.
- Windows library tests: **cross-compiled successfully**.

Temporary read-only evidence scripts are under `/tmp/opencode/`:
`secblitz-policy-wave3-readonly.ps1`, `secblitz-rsop-wave3-readonly.ps1`,
`secblitz-rsop-schema-wave3-readonly.ps1`,
`secblitz-platform-wave3-observe.ps1`, and
`secblitz-missing-rsop-wave3-readonly.ps1`. Backend/fixture guest copies use unique
`secblitz-platform-wave3-*` names to avoid the main reviewer's test files.

### Exact i18n handoff

- `Relevant policy is configured or its authority is unknown: assessment only`
- `Group Policy authority is unknown: assessment only`
- `Relevant resultant Group Policy: assessment only`
- `Firewall preference/effective readback did not match; mutation outcome requires review`
- `No device-management registration or UAC policy authority found by the available probes. Each control repeats scoped policy and capability checks before mutation.`

## Wave 2 - runtime follow-up (2026-10-02)

This section supersedes wave 1's enrollment-heuristic description and records
the follow-up requested after the main reviewer's Windows VM execution.

### Fixes

1. **PowerShell 5.1 outer-host progress corrupted stderr.** The runtime reviewer
   observed 420 bytes of CLIXML progress with exit code zero on the real backend.
   Script-local `$ProgressPreference` did not suppress module-initialization
   progress in the outer scope. The encoded bootstrap now sets
   `$global:ProgressPreference = 'SilentlyContinue'` **before** script-block
   creation/invocation. Genuine stderr remains fatal; no CLIXML/error filtering
   or exit-code-only success shortcut was added.
2. **Built-in enrollment templates were incorrectly treated as active MDM.**
   The runtime VM had 33 enrollment subkeys. Counting all children beneath
   `HKLM:\SOFTWARE\Microsoft\Enrollments` cannot distinguish inbox templates
   from active enrollment. That heuristic is replaced by the documented
   `IsDeviceRegisteredWithManagement` API, called within each `Gate`.
   The API's optional UPN output is omitted (`0`, null). Only a successful zero
   result and a typed BOOL of 0/1 are accepted. Registered devices are blocked;
   failed calls, missing DLL/entry point, unknown or malformed output fail closed.
   Existing domain, OMADM-account, Entra `CloudDomainJoin\JoinInfo`, policy and
   RSoP gates remain in place.
3. **A failed findings probe could leak partial output.** `Finding` previously
   streamed a probe directly, so output produced before an exception remained
   in the overall findings array beside its fallback result. It now buffers the
   probe, validates that exactly one correctly shaped finding was returned, and
   emits one `unknown` finding for failed/malformed probes.

### MDM API implementation and research

Microsoft documents this API for Windows 8.1 and later, including the supported
Win10/11 clients:

- [IsDeviceRegisteredWithManagement (mdmregistration.h)](https://learn.microsoft.com/en-us/windows/win32/api/mdmregistration/nf-mdmregistration-isdeviceregisteredwithmanagement)

The implementation uses an in-process .NET Framework `Reflection.Emit` P/Invoke
stub with the documented ABI (`HRESULT`, `BOOL*`, `DWORD`, `LPWSTR`) and preserves
the return status. `MDMRegistration.dll` is addressed under the launcher-provided
Windows `System32` directory. This avoids PowerShell 5.1 `Add-Type`'s external C#
compiler, which conflicts with the intentional one-process job. The API call
stays inside the child process's existing timeout/job, rather than introducing
an unbounded native query in the parent process. No enrollment registry is
modified, and no gate is disabled to make the VM eligible.

The emitter targets the shipped Windows PowerShell 5.1/.NET Framework host.
Linux PowerShell fixtures mock the native query; they do not validate that ABI.

### Verification and runtime handoff

- `backend.tests.ps1`: **97 checks passed** on Linux PowerShell. Added checks
  cover the 33-template clean-client case, active MDM, failed/nonzero API status,
  malformed BOOL, API unavailability, independent OMADM/Entra rejection, and
  suppression of partial/malformed finding output.
- Final full `cargo test`: **44 tests passed** (36 library, 8 binary), including
  other reviewers' tests present at execution time; zero doctests.
- `cargo build --lib --target x86_64-pc-windows-gnu`: **passed**.
- Full `cargo build --target x86_64-pc-windows-gnu`: **passed**.
- `cargo test --lib --target x86_64-pc-windows-gnu --no-run`: **passed**.
- Initial full `cargo test` was blocked by concurrent code outside this scope:
  `main.rs:185` passed `service::StatusDetails` to `ui.rs:63`'s `&str` parameter.
  The owning reviewer resolved that mismatch; the subsequent full test/build
  results above passed. Those files were not edited by this reviewer.

Two Windows-native tests were added, both read-only:

- `platform::windows::tests::inbox_powershell_actual_backend_read_only` exercises
  actual machine identity, UAC observation, and all 13 findings through the real
  launcher, production dispatcher and inbox module imports. This is the
  regression coverage missing from the earlier synthetic `Console.Write` test.
- `platform::windows::tests::inbox_powershell_native_mdm_registration` invokes the
  production native-query functions directly and requires a readable boolean,
  accepting either managed or unmanaged. It cannot accidentally pass because
  `observe` swallowed an emitter/API error as merely ineligible.

The main runtime reviewer should run the new test executable and repeat the
actual audit/apply/revert scenario. At this entry's creation the new native tests
have been cross-compiled, **not executed on Windows by this reviewer**. Remaining
conservative policy/OMADM/RSoP blockers must be assessed individually; an MDM API
negative alone does not prove global management absence.

Date: 2026-10-02. Scope: `src/platform.rs`, `src/platform/*`;
`engine.rs` and `model.rs` inspected for the calling/rollback contract only.
No engine/model/service/tools/UI files were edited by this reviewer.

## Findings fixed

### 1. Defender acknowledgement did not establish available runtime state

Previously `WriteControl` acknowledged `Set-MpPreference` immediately. Rust
subsequently compared only `Get-MpPreference` against the requested value.
Consequently a stored `DisableRealtimeMonitoring=false` could be acknowledged
even while `RealTimeProtectionEnabled=false`. Equivalent gaps existed for
behavior monitoring and IOAV. An observation's failed eligibility gate also did
not prevent that preference-only acknowledgement.

The Defender write branch now checks the preference plus the corresponding
boolean from `Get-MpComputerStatus`, for both apply and restore. It polls at most
ten times with nine 500 ms sleeps to accommodate propagation, within the existing
90-second process deadline. Unknown/enabled tamper protection, an unavailable
service/antivirus, or passive mode after the setter fail the write. A silent
setter no-op or mismatching/unknown runtime boolean cannot be acknowledged.
Failure leaves recovery to the engine's existing durable pending transaction.

Archive scanning has no corresponding runtime boolean in this API. Its
preference is verified, with the same post-write service/mode/tamper checks;
effective archive scanning is **not** claimed to have been measured.

### 2. UAC restore lacked a platform-local drift check

The engine checks the target before rollback, but the original `WriteControl`
guard applied only to target writes. A non-target restore could overwrite an
absent or third-party-modified value after the engine's last observation.

After the repeated management gate, UAC now reads the current value and requires
the compiled target for every non-target restoration, including deletion to
restore absence. Repair still requires an explicitly present zero. This narrows
the probe/write race and prevents restoring over drift already visible to the
platform. It is not a registry compare-and-swap: another privileged writer can
still race the final read and setter.

## Additional areas audited

- **Typed/injection boundary:** fixed control IDs; boolean/enum/strict numeric
  registry values validated before script interpolation. No accepted value can
  contain a PowerShell quote or executable payload. Elevation uses Windows
  argument quoting and `ShellExecuteW`, not a command shell.
- **PowerShell 5.1 transport:** UTF-16LE `EncodedCommand` contains only a fixed
  bootstrap; the full script is UTF-8 over stdin, avoiding the Windows command
  line length limit. Profiles/autoloading are disabled; environment, working
  directory, interpreter, and module search path are explicitly established.
  The added script uses syntax available in Windows PowerShell 5.1.
- **Spawn/timeouts:** a kill-on-close, one-process job is assigned before stdin
  releases the script. Concurrent bounded pipe draining avoids stdout/stderr
  deadlock; the writer is separate, output is capped at 2 MiB, and response and
  exit waits share a deadline. stderr, nonzero exit, malformed JSON, and limits
  fail closed. Refactored the same runner into private `run_script` to exercise
  these properties without invoking security setters.
- **Eligibility:** management/capability gates repeat inside each mutation.
  Unknown domain/RSOP/security-provider/tamper state does not become a negative
  probe. UAC's exact preserve-reason string remains compatible with the engine's
  narrowly scoped restore exception; the write still repeats `Gate`.
- **Journal trust:** known-folder resolution rather than environment paths;
  local fixed drive; component-by-component no-reparse inspection; trusted
  ancestor ownership; protected root DACL; only SYSTEM/Administrators full-control
  ACEs accepted for the root/entries; restrictive propagation; hard-link rejection;
  pinned ancestor/root handles without delete sharing. Recursive inspection is
  bounded. Existing untrusted trees are rejected rather than repaired/adopted.
  No demonstrated journal ACL bypass was found within the stated unprivileged
  attacker model. Native ACL enforcement still needs the VM adversarial tests.

## Tests and build results

Executed on Linux:

```sh
source /tmp/opencode/secblitz-cross-env.sh
cargo test
cargo build --target x86_64-pc-windows-gnu
cargo test --lib --target x86_64-pc-windows-gnu --no-run
/tmp/opencode/secblitz-powershell/pwsh -NoLogo -NoProfile -NonInteractive \
  -File src/platform/backend.tests.ps1
```

- Linux Rust tests: **41 passed** (28 library, 13 binary; zero doctests).
  This count includes concurrent reviewers' tests present at execution time.
- Windows GNU build: **passed**.
- Windows library test executable cross-compilation: **passed**.
- PowerShell fixtures: **83 checks passed** under Linux PowerShell. They load
  the production function ASTs, stub OS APIs, and never import the production
  backend modules or execute its dispatcher. Coverage includes UAC apply,
  restore, absence, drift, and management rejection; Defender apply/restore,
  ignored setters, runtime disagreement, malformed runtime state, changed tamper
  state, passive mode; actual eligibility gates for client builds, server/old OS,
  domain uncertainty, policy, unavailable RSOP, absent/competing AV registrations.

Windows-native runner test added:
`platform::windows::tests::inbox_powershell_pipe_encoding_and_output_bounds`.
It exercises Unicode transport, stderr rejection, oversized output and a
two-second deadline against a sleeping child. It is non-mutating and does not
require elevation. **Cross-compiled, not executed by this reviewer.**

## VM handoff and remaining limitations

The main reviewer owns Windows VM execution in parallel. Run:

```powershell
powershell.exe -NoLogo -NoProfile -NonInteractive -File src\platform\backend.tests.ps1
# Or copy backend.ps1 alongside backend.tests.ps1 into the guest first.
secblitz-e829e2b902df8244.exe platform::windows::tests --nocapture
```

The executable produced here is under
`/tmp/opencode/secblitz-windows-target/x86_64-pc-windows-gnu/debug/deps/`;
its hash/name may change after other reviewers' builds. Native Windows 5.1,
Defender timing/real tamper behavior, actual journal ACLs/reparse rejection and
real apply/revert remain unverified by this reviewer.

- Missing/unavailable RSOP, enrollment/policy artifacts, and unfamiliar provider
  registrations deliberately make mutations ineligible. This can conservatively
  skip otherwise unmanaged installations; fixtures do not establish which
  artifacts exist on a clean Win10/11 image.
- The interpreter/module locations come from the OS Windows directory and rely
  on normal Windows protection of inbox files. This code does not recursively
  attest their signatures/ACLs. Journal ancestors have a different, explicit
  validation/pinning contract. Already privileged administrators are outside
  the documented journal trust boundary.
- The one-process job can prevent modules from launching helpers (notably
  optional-feature/DISM assessment); such findings can be unknown. It is not
  relaxed merely to make an advisory probe succeed.
- A provider API can block until the outer deadline; timeout/readback failure
  does not prove that a mutation did not occur. Existing pending WAL recovery is
  retained. Process creation itself is a synchronous Win32 call and is not
  covered by the post-spawn 90-second deadline.
- Preferences/effective status may drift after verification; Windows exposes no
  atomic transaction spanning these providers and the local journal. Slow
  Defender propagation beyond the bounded poll conservatively reports failure.

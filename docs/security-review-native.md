# Native security review: website-to-binary automatic updates

## Subsequent 0.4.3 security deployment gate

The published 0.4.3 release completed the genuine 0.4.2 to 0.4.3 SYSTEM update,
current checking, protected release-floor persistence, valid status JSON,
18-control/19-finding audit and real-guide busy-session deferral. Cleanup
preserved all original controls and WAL hashes. See
[the 0.4.3 live results](windows-v043-results.md) and
[the persistence regression supplement](security-adversarial.md).

This additional live evidence does not expand the standard-token/task-RPC,
website control-plane or exhaustive adversarial coverage claimed below. The
native adversarial cases below retain their original tested-version scope.

## Verdict

**No privilege-escalation, unauthorized staged-file modification, or unsigned
installer execution was confirmed in the tested native boundaries.** The
current 0.4.2 worker rejected both invalid-signature cases, the old worker was
rejected against a different installed build, and a genuine older worker
rejected a corrupted authenticated newer installer before execution.

This is a bounded Windows runtime/source review, not a claim that the entire
website, signing infrastructure or product is vulnerability-free. In particular,
Scheduler authorization was checked with a real standard-user token and
Windows `AccessCheck`; alternate-user task RPC execution could not be exercised
successfully through this runner. Coverage limits are explicit below.

| Severity | Confirmed result |
| --- | --- |
| Critical / High | None confirmed in this review |
| Medium / Low security defects | None confirmed in the exercised cases |
| Informational | Users have task read/run authorization, but not task modification/deletion or code-selection authority; this is the configured policy, not an escalation finding |

## Scope, inputs and isolation

- Target: **Secblitz 0.4.2**, Windows 11 UI clone
  `Secblitz-W11-UI-Test` / `4b70288b-b64d-4796-a725-006da3162d0f` only.
- Pre-review snapshot: `secblitz-pre-security-review`, UUID
  `5f08351c-0de0-45ff-bf36-32afec26eb22`.
- All scripts, fixtures and runtime evidence are under
  **`target/security-adversarial/`**. Guest workspace:
  `C:\Windows\Temp\SecblitzSecurityA`.
- The guest remained **offline throughout**. Cached public artifacts and the
  previously downloaded production-signed 0.4.2 manifest were used. The original
  user VM was never operated on.
- Original app/data directories were archived before disposable state was
  created. No legitimate journal was poisoned, edited or deleted.
- No Rust/application source, immutable release artifact or production feed was
  modified. No malware, exploit executable or Defender disabling was used.
- Disposable standard-account credentials were generated in memory, never
  logged or written to a credential file. Only a temporary **test** signing key
  was generated in host memory for one wrong-key fixture; no private key was
  saved or sent to the guest. The production private key was not accessed.

### Immutable artifacts

| Artifact | SHA-256 |
| --- | --- |
| Current 0.4.2 EXE | `78c666e456b3f72fbe503e4aad6213b1baec4f7172e3e8cacd17f9bf893b4672` |
| Current 0.4.2 installer | `28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4` |
| Genuine archived 0.4.1 EXE | `f686dbf3f40289a5e2153178ccf96c9c8eee7a6fd230f6b636f5f35ae6b6f253` |
| Genuine archived 0.4.1 installer | `9713aff6c0f9d2029e8a0ee4b9c8c7013abd856a3f26717acb36cc6cab17bbcc` |

All four remained unchanged after the review. The original production manifest
signature was independently verified against the public-key asset on the host;
its signed installer hash and size matched the current immutable installer.

## 1. Standard-user filesystem boundary - PASS

A newly created local account was added only to the standard Users group. A
real Windows logon token was obtained and impersonated **on the same native
thread performing the filesystem calls**. The effective SID was checked against
that account, and administrator membership was false. This was not merely a
restricted administrator token or a synthetic SID-only simulation.

The 0.4.2 application was installed by its genuine installer. The current core
created the protected `Updates` directory through `update status --json`.
Manifest/worker/installer/status test files were populated as privileged
fixtures with the same SYSTEM/Administrators-only SDDL used by production
`create()`. The installed app and task protections came from the real installer.

### Measured checks

**53 checks completed: 49 expected denials and four positive controls.**

- **47 actual filesystem API checks** under the standard token:
  45 denials and two positive controls.
- **Six Windows `AccessCheck` evaluations** against the actual task security
  descriptor using that same token: four denials and two grants.

Actual `CreateFileW` requests for write, delete, DACL modification and owner
modification were denied with **Win32 error 5 (`ERROR_ACCESS_DENIED`)** on each
of these ten objects:

1. Installed `secblitz.exe`.
2. Installed application directory.
3. `C:\ProgramData\Secblitz`.
4. Its `Updates` directory.
5. Staged `update-manifest.json`.
6. Staged `update-worker.exe`.
7. Staged `update-installer.exe`.
8. Staged `update-status.json`.
9. A protected legacy root-level `update-status.json` fixture.
10. `C:\Windows\System32\Tasks\SecblitzUpdate`.

Creating a child file in the installed app directory, base state directory or
`Updates` was denied. Creating a hardlink from a harmless user-owned file into
the protected staging directory or the legacy root `update-worker.exe` location
was also denied. The same token could read the installed executable and create
a hardlink between two files in its own disposable writable folder, providing
positive controls for the token and filesystem API.

Two additional checks tested the opposite hardlink direction: linking a
protected installed EXE or protected staged installer into the user's own
writable folder. Both returned error 5 while an owned-file hardlink control
succeeded. These two checks ran after the corruption phase, against the genuine
0.4.1 disposable installation and its protected staged payload; they are not
misrepresented as additional 0.4.2 image-specific cases.

Evidence:

- `token-boundary-results/token-boundary.json` - all 53 rows and effective token.
- `external-link-verified/external-link-results.json` - the two reverse-direction
  hardlink denials with immediately captured native error codes.
- `review-summary.json` - machine-checked counts.

## 2. Scheduler policy: read/run is allowed; modification is not

The actual task security descriptor was:

```text
O:BAG:BAD:PAI(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;BU)
```

Windows `AccessCheck` with the real standard-user impersonation token granted
task read (`0x120089`) and execute (`0x20`) rights. It denied task data/XML write
(`0x2`), deletion, DACL changes and owner changes. The separate native open of
the task's backing file also denied all four modification rights.

**Interpretation:** the configured task permits a standard user to request the
fixed signed-update operation. That authorization does not let them substitute
a URL, executable, installer, arguments, principal or task XML. The installed
command remains fixed, and the protected code/staging boundaries remain in
force. Public task run authorization is therefore not, by itself, generic
SYSTEM code execution.

**Important limit:** these task grants/denials are authoritative DACL/token
authorization results, not successful standard-user Scheduler RPC invocations.
The alternate-user launch attempts failed at runner initialization or task
registration before the probe ran. No task-disable/replace RPC from those
attempts is counted as a passing test. Direct non-admin CLI process tests are
also not counted. Their existing unit tests are separate evidence.

## 3. Native pinning, links, locks and journal exceptions - PASS

The existing **seven elevated updater tests** were executed serially as SYSTEM,
session 0. All passed, including:

- Protected, reopenable `Updates` directory and the shared base `engine.lock`.
- Clean child environment/native known-folder resolution with lock contention.
- Verified-payload pin preventing write, replacement and deletion while held.
- Directory data-access pin preventing prefix rename.
- Hardlink/reparse-point rejection.
- Legitimate staged replacement under ancestor pins while respecting a payload
  pin.
- Duplicate-worker lock exclusion.

Three native engine tests also passed:

```text
updater_reserved_entries_coexist_with_exact_journal_roundtrip
updater_exceptions_reject_wrong_types_unknown_names_and_corrupt_wal
updater_reserved_files_reject_hardlinks
```

Thus accepting the exact `Updates` directory and five legacy filenames did not
make arbitrary filenames, wrong object types, corrupt journals or hardlinked
reserved files acceptable in these tests.

Source review corroborated the runtime behavior: handle inspection rejects
reparse objects and multi-link files; strict roots/payloads require trusted
owners and do not grant untrusted access; payload handles deny write/delete
sharing through verification and execution; directory pins retain no-delete
sharing. The worker rechecks signed bytes, time/version constraints and payload
integrity before spawning the fixed installer. Its image must match the trusted
currently installed image. The engine permits only the explicit reserved
updater namespace while production platform validation retains the ACL/owner
boundary.

These are deterministic native race-boundary checks, not an exhaustive
multi-process stress/fuzz campaign or a dedicated directory-junction matrix.

Evidence: `install-results/system-pinning-layout.out`,
`engine-reserved-entries.out`, and `system-tests.json`.

## 4. Real worker tampering/replay rejection - PASS

All fixtures used inert byte changes and genuine executables. Protected staging
was populated by the review administrator to simulate bad downloaded/staged
content; this does **not** imply a standard user could populate it - the preceding
token tests showed the opposite.

| Case | Actual worker / installed build | Result | Observed duration |
| --- | --- | --- | ---: |
| One bit flipped in the manifest signature | Current 0.4.2 / current 0.4.2 | Exit 1; persisted `failed`; installed hash unchanged | 1,046 ms |
| Otherwise structured 0.4.3 manifest signed by an unrelated ephemeral TEST key | Current 0.4.2 / current 0.4.2 | Exit 1; persisted `failed`; installed hash unchanged | 951 ms |
| Genuine old worker replay with valid production manifest | Genuine 0.4.1 worker / installed 0.4.2 | Exit 1; persisted `failed`; current image unchanged | 559 ms |
| Valid production-signed newer manifest, one installer byte corrupted without changing size | Genuine 0.4.1 worker / installed 0.4.1, target 0.4.2 | Exit 1; persisted `failed`; old image unchanged | 512 ms |

An additional `update install-staged` invocation from the **installed executable
path**, rather than the fixed protected worker path, returned exit 1 without
launching an installer.

The process-start observer captured **all four expected worker starts and zero
`update-installer.exe` starts** during the four tamper cases. Thus a missing
observer was not mistaken for “no execution.” The separate genuine setup and
uninstall operations used to establish fixtures were outside that staged-
installer observation target and are explicitly logged.

### Why the corrupted-payload case used the genuine older worker

The valid public manifest describes 0.4.2. A current 0.4.2 worker receiving that
valid same-version manifest would return `UpToDate` before testing payload
bytes. That would be a false hash-rejection test. Installing the genuine 0.4.1
package in disposable state made the valid 0.4.2 manifest genuinely newer, so
its worker reached the corrupted payload path. The current 0.4.2 invalid-key/
signature runtime checks and native payload-pin tests are reported separately.
No production signing key or invented production-signed future release was used.

The workers' persisted reason was the intentionally generic
`Staged installation failed`; the report does not claim the CLI exposed the
internal cryptographic exception. The fixtures, input verification, unchanged
image hashes and absence of installer launches establish the tested rejection.

Evidence: `tamper-resume-results/tamper-results.jsonl`,
`commands.jsonl`, `corrupt-payload.json`, the four saved status records and
`tamper-process-starts.jsonl`. `make-manifests.py` verifies the original public
signature/hash/size before constructing the invalid-trust envelopes.

## 5. Busy-session protection and live origin chain

The completed [0.4.2 live deployment gate](windows-v042-results.md) is reused
rather than repeated: actual published 0.4.1 → 0.4.2 update through SYSTEM,
`beacons.lol` compatibility then `secblitz.lol` current checking, protected new
state with legacy files retained, real guide scan/menu, `DeferredBusy` while
the same guide stayed alive, responsive arrow input and normal Esc exit.

This security review did not reconnect the VM to the Internet or tamper with
the production feed. Website/TLS/CDN behavior is not inferred from the offline
negative worker cases.

## Confirmed observations, severity and remaining coverage

### Informational: task execute rights are intentional authorization

Users' read/execute task rights were confirmed by Windows authorization checks.
Neither backing-file modification nor task write/delete/ownership rights were
granted. Treat a request to run the fixed signed checker differently from the
ability to replace its action. No elevation vulnerability was demonstrated.

### No new confirmed exploitable native finding

The tested untrusted token could not modify the installed image, stage an
alternate payload/worker/manifest, change protected status or link into the
protected state. The privileged worker rejected the invalid-trust/replay/
corruption fixtures without running the staged installer. This supports the
tested boundary; it does not prove every possible attack is absent.

### Explicitly uncovered or inherited evidence

1. **Actual standard-user task RPC / direct non-admin CLI process execution:**
   not completed in this runner. Credential-launched PowerShell exited with
   `0xC0000142`; limited S4U task registration returned `0x80070005`, including
   from a SYSTEM controller. Native same-thread impersonation and task DACL
   `AccessCheck` supplied the successful boundary evidence instead.
2. **Current 0.4.2 worker accepting a production-authenticated, strictly newer
   corrupted payload:** no such future signed manifest was available. The
   real corruption case used genuine 0.4.1 plus the valid 0.4.2 feed, as above.
3. **Cloudflare/Pages account takeover, origin routing policy, website/XSS,
   signing-key custody, CI/build compromise and release-author intent:** outside
   this native report and reserved for the other reviewers. Possession of the
   production signing key changes the trust model.
4. **TLS/proxy/redirect/network fault injection and manifest parser fuzzing:**
   not rerun here. Source checks, existing protocol tests and prior live results
   are distinct evidence, not replacements for those adversarial campaigns.
5. **Exhaustive race scheduling, every junction/reparse variant, initial
   namespace squatting before protected-state creation, registry ACL probing,
   Windows 10 and a full multi-user interactive matrix:** not directly covered.
   No availability guarantee is made against all such cases.
6. **Published 0.4.0 self-recovery:** still unsupported; its broken worker needs
   manual upgrade to a fixed release. That historical limitation is unchanged.

## Harness interruptions and cleanup

The evaluation VM hit its hourly shutdown limit during the first tamper run.
After reboot, inspection confirmed no active updater and the unchanged 0.4.2
image. The interrupted logs were retained and not counted as a complete result.
The negative phase was rerun with a new once marker and a separate evidence
directory; only the complete `tamper-resume-results` set is used above.

Final cleanup normally uninstalled the disposable old package, removed test
tasks/accounts/profiles, and restored original directories. Temporary SID
account-right entries were checked; all were absent (`STATUS_OBJECT_NAME_NOT_FOUND`).
No synthetic user or profile remained. Evidence and inert fixtures remain only
in the isolated review directories.

`cleanup-results/final-state.json` confirms:

- **All 18 controls unchanged.**
- **Original journal/file hashes unchanged.**
- No original app/data directory left moved aside.
- No installed app, shortcut, updater/test task, monitor service or app/worker/
  installer process remains.
- No temporary standard account or user profile remains.

The clone was shut down normally. **NIC1/NIC2 remain `none`**; networking was
never enabled for this review. Published/archived artifact hashes were checked
again after cleanup and still match the input table.

# Windows 0.4.2 - LIVE E2E PASS

**The deployment gate passed on 2026-10-03:** the genuine published 0.4.1
installation upgraded automatically to the published 0.4.2 release through its
owned SYSTEM scheduled task. Current-version checking, native audit, real guide
opening, busy-session deferral and cleanup all passed.

## Scope and exact artifacts

Only `Secblitz-W11-UI-Test` (`4b70288b-b64d-4796-a725-006da3162d0f`) was used.
The guest was Windows 11 Enterprise Evaluation, build 26200.9457. The original
user VM was untouched. No signing secret entered the guest; no release artifact,
production feed or application source was modified during this validation.

| Artifact | SHA-256 |
| --- | --- |
| Genuine published starting installer, 0.4.1 | `9713aff6c0f9d2029e8a0ee4b9c8c7013abd856a3f26717acb36cc6cab17bbcc` |
| Installed starting EXE, 0.4.1 | `f686dbf3f40289a5e2153178ccf96c9c8eee7a6fd230f6b636f5f35ae6b6f253` |
| Production downloaded installer, 0.4.2 | `28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4` |
| Installed final EXE, 0.4.2 | `78c666e456b3f72fbe503e4aad6213b1baec4f7172e3e8cacd17f9bf893b4672` |

The new installer is **3,813,017 bytes**. The starting installer came from the
unchanged `dist/archive/0.4.1/` copy of the published artifact, verified before
installation. **No rebuilt or version-spoofed fixture was used.** Native
`--version` checks confirmed `secblitz 0.4.1` before and `secblitz 0.4.2` after.

## Results and durations

| Gate | Observed result | Duration |
| --- | --- | ---: |
| Actual SYSTEM task: published 0.4.1 → published 0.4.2 | `Installed`, exact target hash/version | 15.166 s |
| Next SYSTEM task with current 0.4.2 | `UpToDate` in the new state directory | 1.437 s |
| Captured status JSON and stdout/stderr EOF | Exit 0, correct `up_to_date`, no stderr | 77.119 ms |
| Native `audit --json` | Exit 2, valid 18 results and 19 findings | 36.298 s |
| SYSTEM task while real guide remained open | `DeferredBusy`, same guide PID alive | 1.744 s |
| Captured busy status JSON and pipe EOF | Exit 0, correct `deferred_busy`, no stderr | 113.626 ms |
| Guide interaction after deferral | Down-arrow moved selection; Esc exited normally | Passed |
| Uninstall and original-state restoration | All preservation/residue checks passed | Passed |

Audit exit 2 represented findings, not a parser or operational failure. No fixes,
password generation, unsafe permission fixtures or Defender disabling were run.

## Live signed upgrade and origin transition

After installing the genuine 0.4.1 package with its default updater selection,
the test invoked **`Start-ScheduledTask SecblitzUpdate`**. Before this explicit
trigger, Scheduler reported the task had not yet run. The shipped task action
was not replaced by a diagnostic wrapper.

The observed chain ran entirely as **SYSTEM (`S-1-5-18`), session 0**:

```text
secblitz.exe update check
  C:\ProgramData\Secblitz\update-worker.exe update install-staged
    C:\ProgramData\Secblitz\update-installer.exe
      /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /TASKS= /SECBLITZUPDATE=1
```

The actual 0.4.1 binary retained its compiled **`https://beacons.lol`** origin.
Its normal signed-feed path downloaded and verified the 0.4.2 installer through
the compatible `/releases/stable.json` and `/downloads/` endpoints. No endpoint
override, redirect workaround, signature bypass or manual installer execution
was used for the upgrade.

The DNS cache was cleared immediately before each network check. After the old
check, captured DNS evidence contained **`beacons.lol`**; after the new 0.4.2
check, it contained **`secblitz.lol`**, matching the two immutable binaries'
pinned origins. This is DNS/process/artifact evidence, not a decrypted HTTP
packet trace. The updater's normal no-redirect and verification checks remained
active throughout.

The old worker persisted completion at the legacy path:

```text
C:\ProgramData\Secblitz\update-status.json
```

```json
{"checked_at":1791000524,"result":{"outcome":"installed","version":"0.4.2"}}
```

The next 0.4.2 task created/used:

```text
C:\ProgramData\Secblitz\Updates\update-status.json
```

It first recorded `up_to_date`, then `deferred_busy` during the real-guide test.
The legacy Installed record remained intact. Desktop selection and
`AutoUpdatesEnabled = 1` survived the upgrade, and the task security descriptor
matched its pre-upgrade value exactly. No monitor service was installed or
started by these default install/update operations.

## Journal compatibility, protected layout and real UI

Eight genuine, previously reverted transaction journals were copied from the
archived original state into the protected disposable state directory. Their
hashes were checked before and after upgrade, audit, guide use and uninstall.
No unrecognized JSON sentinel was introduced.

All five legacy updater files remained beside the real journals:

```text
update.lock
update-status.json
update-manifest.json
update-installer.exe
update-worker.exe
```

The new `Updates` child existed concurrently with those legacy files. Its
observed protected security descriptor was:

```text
O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)
```

SYSTEM and Administrators have full access; there are no user grants, and DACL
inheritance is protected. Base `engine.lock` sharing, exact reserved names,
wrong-type/unknown-name rejection and hardlink rejection had already passed the
0.4.2 native suite: **106 library + 52 CLI tests and seven elevated SYSTEM
tests**. Those negative tests were not rerun against the live feed.

The **actual installed 0.4.2 guide** ran in an interactive Windows terminal,
completed its scan and displayed the arrow menu. No `Unexpected journal entry`
error occurred despite the retained legacy state. While at that menu, the
normal SYSTEM updater task returned `DeferredBusy`; the same guide process
(PID 9112) remained alive. A Down-arrow input moved the visible selection, proving
continued responsiveness. **Esc exited normally** back to the command prompt.
The command shell was then closed with `exit`; nothing was force-terminated.

## Preservation and final state

The completed test installation was uninstalled normally. Independent final
evidence confirmed:

- **All 18 control baselines unchanged**, including Defender/UAC/firewall,
  registry preferences and service security descriptors.
- **Original journal/file hashes unchanged.**
- **All eight test journal copies unchanged.**
- Original application and data directories restored; neither left moved aside.
- No installed Secblitz EXE, desktop/start-menu shortcut, updater/test task,
  monitor service or Secblitz/worker/installer process remains.
- Original static lab IPv4 address and empty IPv4 DNS configuration restored.
- UI clone shut down normally; **NIC1/NIC2 `none`**, clipboard and drag/drop
  disabled. Temporary NAT is removed.

The original user VM was not queried or controlled, and no private key was
accessed or copied into the guest.

## Evidence locations

Persistent host root: **`target/windows-live-v042/`**.
Guest evidence retained under `C:\Windows\Temp\SecblitzLiveV042Final`.

- `upgrade-results/`: original install/version, SYSTEM process chain, status
  transitions, payload/final image hashes, origin DNS evidence and timing.
- `post-checks/`: current/busy status JSON, task XML/ACL, audit JSON/counts,
  protected directory descriptor and origin-transition DNS evidence.
- `cleanup-evidence/`: unchanged baselines/journals, retained legacy files,
  separate old/new status records, uninstall log and final-state assertions.
- `guide-menu-ready.png`, `guide-after-deferred.png`, `guide-responsive.png`,
  `guide-exit.png`: real interactive guide scan/menu, survival, response and exit.

Earlier blockers and offline acceptance history remain in
[the 0.4.x validation log](windows-v040-results.md).

## Limits and recovery note

This PASS covers the actual published **0.4.1 → 0.4.2** upgrade on the Windows 11
clone, both pinned origins, subsequent current/busy behavior and preservation.
It does not establish Windows 10 coverage or repeat the earlier non-admin/UAC
and all-language UI matrices. The first hourly trigger/ACL and broader installer
service lifecycle were covered in the preceding basic acceptance phase.

The published **0.4.0 worker remains unable to self-heal** its sanitized-
environment defect. Those installations still require a manual upgrade to a
fixed release; this successful genuine-0.4.1 run does not change that limitation.

# Windows 0.5.0: LIVE E2E PASS

## Live deployment gate: PASS

The **genuine published 0.4.3 installation upgraded to the final published 0.5.0
release through its actual owned SYSTEM scheduled task**. No rebuilt starting
binary or manual upgrade was used. The running LocalService monitor was resumed,
its automatic startup was preserved, and the protected release floor advanced.
Current checking, typed audit/readiness, real-guide deferral and cleanup passed.

Persistent evidence: **`target/windows-live-v050/`**.
Guest workspace: `C:\Windows\Temp\SecblitzLiveV050Final`.
Only the UI clone was used. No application source, published artifact, production
feed or website configuration was modified by validation.

### Exact live artifacts

| Artifact | SHA-256 |
| --- | --- |
| Genuine starting 0.4.3 installer | `871f61695f30a1566bb77627b0c4378c106879110b8c2d19affb8b92e0305605` |
| Installed starting 0.4.3 EXE | `7ec84de303d97a537b7d3324511ff63745f1911d2f29660e6ac8a8f436d5471d` |
| Published downloaded 0.5.0 installer | `c17a543fccbb0ec1c37c487aeb8da2e7bfd8a832e04996f6ead32e6dd2b77f3b` |
| Installed final 0.5.0 EXE | `036d8b69367cb7422ca5d6e821e73749f1c36ce35df437ac47b7d83ba829a6d9` |

The installer is **3,850,954 bytes**. Actual installed version probes confirmed
0.4.3 before and 0.5.0 after. Release files retained these hashes after cleanup.

### Eight recorded integration gates

| Gate | Result | Timing/evidence |
| --- | --- | --- |
| 1. SYSTEM automatic upgrade | Installed 0.5.0; exact image/version and package | **18.865 s** |
| 2. Resumed monitor | LocalService, Auto, Running; fresh complete 18/19 report | **9,195 bytes**, below 64 KiB |
| 3. Current scheduled check | UpToDate; valid status JSON and closed pipes | **1.336 s**; JSON/EOF **63.679 ms** |
| 4. Current floor proof | Authentic prior 0.4.3 floor advanced to signed 0.5.0 metadata | `floor-current-result.json` |
| 5. Native audit | **18 results, 19 findings**, readiness and typed firewall evidence; expected exit 2 | **29.407 s** |
| 6. Real-guide busy check | DeferredBusy; same guide PID alive and responsive | **1.005 s**; JSON/EOF **63.299 ms** |
| 7. Floor after busy check | Byte-identical 0.5.0 floor; valid status JSON | `floor-busy-result.json` |
| 8. Cleanup/preservation | Original controls/WAL restored; no installation/process residue | `cleanup-evidence/final-state.json` |

Bootstrap installation and screenshots support these gates rather than adding
native unit-test counts. The monitor's recorded 0.510 s is retrieval wait after
the fresh report was available, not scan duration. The 64 KiB bound applies to
report bytes, not process RSS.

### Updater and running-service behavior

The genuine 0.4.3 package was installed with explicit monitor opt-in alongside
desktop and automatic updates. The old monitor completed a read-only report and
was left running before `Start-ScheduledTask SecblitzUpdate`.

Process evidence recorded the check, protected worker and installer as SYSTEM
(`S-1-5-18`), session 0. The old and resumed monitor ran as LocalService
(`S-1-5-19`), with PID changing from 1080 to 5728 through owned-service maintenance:

```text
secblitz.exe update check
  Updates\update-worker.exe update install-staged
    Updates\update-installer.exe
      /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /TASKS= /SECBLITZUPDATE=1
```

Service startup remained Auto and state Running. Desktop selection,
`AutoUpdatesEnabled = 1` and the task DACL were preserved. The updater used the
compiled `https://secblitz.lol` origin and normal signed-feed verification with
no endpoint override, signature bypass or publication operation.

```json
{"checked_at":1791023797,"result":{"outcome":"worker_started","version":"0.5.0"}}
{"checked_at":1791023811,"result":{"outcome":"installed","version":"0.5.0"}}
```

### Authentic floor and monotonic advancement

The disposable protected state was seeded with the **authentic 0.4.3 floor saved
from the preceding successful live 0.4.3 run**, file SHA-256
`f91796311c0bc734cdff07e8e2a78f2fc175b0601e86e6072180e7acad64e2e5`.
This restores prior trusted local state; it does not claim that the current
public feed still served 0.4.3. No test-key manifest or replacement binary was used.

| Floor field | Before | After |
| --- | --- | --- |
| Version | 0.4.3 | **0.5.0** |
| Installer SHA-256 | `871f6169...e0305605` | `c17a543f...d2b77f3b` |
| Published at | 1791012306 | **1791022530** |
| Expires at | 1798788306 | **1798798530** |

All new fields matched the authenticated downloaded manifest. The floor in
`C:\ProgramData\Secblitz\Updates\release-floor.json` retained the protected
descriptor `O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)`. Owner, DACL protection and
grantees were checked. Its resulting SHA-256 was
`cc763ccf277c3d12593d5bee295f2e58c771b935ca452f82120cdb97acd1dc22`, unchanged
between current and busy checks. Normal status records remained valid JSON.
No live rollback/corruption or destructive atomic-write provocation was performed;
the preceding SYSTEM regression tests remain the fault-injection evidence.

### Post-update baseline, real guide and monitor

All three stored inbound preferences remained **NotConfigured**, with ActiveStore
**Block**. The audit classified each as compliant with typed
`effective: {kind: inbound, value: block}` and local authority. No automatic
repair candidate was present. All four readiness signals were included without
adding controls or findings.

The real guide completed its scan and displayed **Fix recommended first**,
zero recommendations, 18 protected and nine needing a choice. No Fix, Apply or
Undo action was selected. During the busy check, the same guide PID 4012 remained
alive; Down-arrow moved selection and Esc exited normally. No forced UI close
or hardening mutation occurred.

The resumed monitor report had **18 observations, 19 findings, incomplete=false**,
six correctly typed firewall entries and a root readiness object containing all
four signals. It reported Known writable system volume with values above 4 GiB,
Known AC/no-battery facts and a native reboot Boolean. Journal volume was
explicitly **Unknown** under LocalService, not false, zero or a green claim.

### Final restoration

The test app and monitor were uninstalled normally. All **18 original controls**,
original journal/file hashes and eight copied transaction journals were unchanged.
No new transaction WAL was created. Original directories were restored, with no
app, shortcut, updater/test task, monitor service or app/worker/installer process
remaining. Static IP/DNS settings were restored and the clone shut down with
**NIC1/NIC2 `none`**. The original user VM was untouched. No credentials or private
signing material were printed or included in evidence.

Evidence subdirectories: `upgrade-results/`, `post-checks/`, `cleanup-evidence/`.
Screenshots: `guide-menu.png`, `guide-responsive.png`, `guide-exit.png`. Windows
10 and the earlier full hardening matrix were not repeated; prior native 0.5.0
acceptance remains applicable.

## Prior native and real-UI acceptance

**The exact tested 0.5.0 artifacts were promoted before publication.**
Typed firewall evidence, read-only readiness, recommended-batch approval,
automatic post-apply/undo verification, failure handling, LocalService reporting
and installer lifecycle passed on the isolated UI clone. The subsequent genuine
live upgrade is now complete and recorded above.

## Copy-only pre-release follow-up: PASS

The owner corrected the redundant undo advice before 0.5.0 publication:

- Footer: `Your saved changes were reviewed.`
- Restored row: `Your earlier setting was restored.`
- All six language catalogs were updated; the manual-rescan instruction is gone.

The latest EXE was frozen, a new real Inno 0.5.0 installer compiled, and the full
native CLI suite rerun: **66 passed**, including all **13 localization checks
and 15 UI/advice checks**. The native library executable is byte-identical to
the previous accepted one (`f9e878...861d15`), and the frozen model, engine,
backend, guided flow, readiness, service, Cargo and installer sources remain
unchanged. The prior 138 library tests, nine SYSTEM cases, full lifecycle and
two-control apply/undo evidence therefore remain applicable; those mutation
fixtures were deliberately not repeated for this text-only change.

The rebuilt installer was exercised interactively. Desktop/automatic-update
defaults and checked finish-launch were retained. Finish launched the installed
0.5.0 guide, which completed its scan and showed valid device readiness,
**zero recommendations, 18 protected and nine needing a choice**. The installed
version and exact new EXE hash were checked. The guide exited normally with Esc;
no WAL was created and no control was changed. Normal uninstall restored the
original app/data directories and journal hashes, leaving no app, shortcut,
task, service or process. The first cleanup request correctly refused while the
unfocused guide was still open; the completed cleanup PASS supersedes that
guard message, with no forced process termination.

Current follow-up evidence is under **`target/windows-validation-v050-copy/`**,
including `provenance.json`, `final-results/`, installer/guide screenshots and
`promotion.json`. The superseded, never-published candidate is retained under
`superseded-unpublished-candidate/` in that evidence directory. All **21 archived
release files**, including `dist/archive/0.4.3/`, were verified unchanged.
The artifact table below is the authoritative updated 0.5.0 handoff.

## Artifacts

| File | SHA-256 |
| --- | --- |
| `dist/secblitz.exe` | `036d8b69367cb7422ca5d6e821e73749f1c36ce35df437ac47b7d83ba829a6d9` |
| `dist/secblitz-0.5.0-windows-x64-setup.exe` | `c17a543fccbb0ec1c37c487aeb8da2e7bfd8a832e04996f6ead32e6dd2b77f3b` |

Installer size: **3,850,954 bytes**. The installer was compiled in the guest from
the supplied frozen EXE and actual Inno source, then tested without rebuilding.
Guest, downloaded host and promoted hashes agree. `dist/SHA256SUMS` verifies.
The previous 0.4.3 EXE, installer and checksum file were preserved in
`dist/archive/0.4.3/`; that archive and every older archive remain unchanged.

Original functional evidence root: **`target/windows-validation-v050/`**. Full source/input hashes
are in `frozen-inputs.json`; promotion guards also checked that product source
and release bytes had not changed during validation.

## Isolation and completed checks

Only **Secblitz-W11-UI-Test**, UUID
`4b70288b-b64d-4796-a725-006da3162d0f`, was used. Pre-test snapshot:
`secblitz-pre-v050`, UUID `e9da6770-b293-4997-a70f-c3d45efb58fb`.
Guest: Windows 11 Enterprise Evaluation, build 26200.9457, inbox PowerShell 5.1.
The clone remained offline. The original user VM was never queried, controlled,
restarted or sent input.

| Check | Result |
| --- | --- |
| Full native library suite | **138 passed**, zero failed, 11 ignored |
| Full native CLI suite | **66 passed**, zero failed, two attended probes ignored |
| Localization checks within CLI suite | **All 13 passed** |
| Explicit `readiness::windows::tests::native_readonly_smoke` | Passed, 0.07 s |
| Native readiness ABI/fake IDispatch/CLSID cases | Passed within the library suite |
| Explicit elevated updater suite | **All nine passed as SYSTEM/session 0** |
| Production task XML fixtures | Three accepted, 21 mutations rejected |
| Independent Windows API and MTA COM comparison | Passed |
| Real UI no-op, approval, two-control apply/undo and failed operation | Passed |
| Full permitted Inno lifecycle and sanitized SYSTEM upgrade | Passed |
| LocalService monitor schema/typed metadata/readiness | Passed |
| Final controls and original WAL preservation | Passed |

The 11 library ignores were nine elevated updater tests, one real-readiness
smoke and the obsolete opt-in public 0.4.2 feed probe. The first ten were
explicitly exercised in their appropriate native context. The public feed probe
remained ignored. The old attended unit probes were not run as unit tests;
the actual application UI was exercised directly instead.

## Four review handoffs and runtime closure

The supplied code had four owner reviews. Their records describe their own
review-time states; this native execution does not rewrite those histories.

| Review | Recorded scope/fixes | Runtime evidence in this phase |
| --- | --- | --- |
| [Engine/contracts](review-v050-engine.md) | Effective readback before sealing, readiness-blocked owned no-ops, raw schema-1 originals | Full native suite, real two-control WAL/undo, no-op preservation; no raw-schema change |
| [Readiness security](readiness-security-review.md) | Unknown power, reparse rejection, high-integrity/pinned COM activation, ABI and lifetime | Native ABI and fake-IDispatch tests, real collector smoke, independent MTA COM/volume/power observations, LocalService report |
| [Platform](review-v050-platform.md) | Typed authority/effective evidence, strict current-store gates, wrapped errors, clean JSON | Actual inbox PowerShell observations, inherited Block baseline, native Public firewall write/readback/undo, valid typed monitor output |
| [Experience/integration](review-v050-experience.md) | Enter-only bounded approval, exactly-once verification before rendering, information counts and localization | Real menu/Back/Change selection/Space/Enter tests, automatic refresh and failure display, all 66 CLI and 13 localization tests |

Earlier GUID-comparison, constructor and translation integration blockers in
the review histories did not recur in the supplied built candidate. Native
execution passed rather than relying on the older cross-compilation claims.

## Firewall baseline and zero-write no-op

Independent `Get-NetFirewallProfile` captures showed all three profiles with:

```text
PersistentStore.DefaultInboundAction = NotConfigured
ActiveStore.DefaultInboundAction     = Block
```

The real JSON audit returned **18 control results and 19 findings**, exit 2 for
remaining findings. All three inbound outcomes were `compliant` with:

```json
{"effective":{"kind":"inbound","value":"block"},"authority":"local"}
```

No inbound repair was recommended. The baseline had **zero automatic-fix
candidates**. The real UI displayed `Recommended fixes: 0`, `Protected: 18`,
`Needs your choice: 9`. Its ten informational findings did not inflate that
attention count. Readiness appeared separately under Device check.

Selecting the first **Fix recommended** action displayed that no recommended
automatic fixes were available, returned to the menu and created **no WAL**.
All 18 controls and journal hashes remained unchanged. Genuine Allow eligibility
and malformed/missing evidence are covered by native unit/binding fixtures;
no third firewall setting was deliberately mutated in this UI phase.

## Independent readiness validation

The high-integrity application reported:

- System and journal volumes Known, writable, with **73,018,777,600 available
  bytes**, retaining values above 4 GiB.
- AC connected, no battery, and `battery_percent: null`.
- Windows Update reboot Known false.

A separate C# P/Invoke/PowerShell MTA probe used the real
`GetWindowsDirectoryW`, `GetVolumePathNameW`, caller-aware
`GetDiskFreeSpaceExW`, `GetVolumeInformationW` flags and `GetSystemPowerStatus`.
It queried the machine-registered, pinned WUA SystemInfo class's
`RebootRequired` property only. No update search, download or settings call was
made. The independent probe reported MTA, a 12-byte power structure, raw AC=1,
battery flag=128, percentage=255 and a native Boolean reboot=false. Its COM
property query took 54 ms in the recorded before-sample.

Both volume paths resolved to `C:\`; the before-sample available count was
73,019,224,064 bytes. The comparison allowed before/after variation plus a stated
256 MiB margin for concurrent guest allocations; it did not require an unstable
free-space count to remain byte-exact. Read-only flags, Boolean types, battery
sentinels, counts above 4 GiB and independent probe agreement all passed.

The real collector smoke returned in 0.07 s. The existing native/portable tests
cover wrong VARIANT types, canonical VARIANT_BOOL values, release ownership,
class parsing, timeout/unknown and single-worker behavior. A stuck OS COM call
was not deliberately induced in the live guest. The two-second limit is the
COM worker wait, not a universal deadline for every local volume call.

Evidence: `readonly-results/` and `native-results/readiness-readonly.out`.
The complete native audit took 29.564 s and performed no control or WAL mutation.

## Real recommended-batch apply and undo

The original app/data directories were archived for the UI fixture. Only these
two disposable machine preferences were temporarily changed by the harness:

1. Public firewall Enabled: true to **false**.
2. `ConsentPromptBehaviorAdmin`: 5 to **0**.

The clone was offline throughout. Defender, EnableLUA, all service DACLs and the
other controls were not weakened. All three stored inbound defaults stayed
**NotConfigured**. The original values were restored after the test.

The next real guide scan offered exactly **Public network firewall** and
**Administrator approval**:

- Fix recommended was the first root action.
- The review listed both candidates and defaulted to **Back**.
- Enter on Back made no change and created no WAL.
- Change selection opened unchecked boxes for the same candidate set.
- Both controls were explicitly selected and reviewed again.
- Space while Apply was selected did **not** submit or write.
- **One Enter** applied the two approved changes in one transaction.
- Without selecting Check my PC again, the automatic fresh assessment returned
  to **zero** recommendations and the original safe Public/UAC settings.
- Undo defaulted to No. Explicit Yes/Enter restored the fixture's false/0 values.
- Again without a manual check, the automatic fresh assessment returned to
  **two** recommendations.

Independent captures confirmed the other **16 controls unchanged at every
checkpoint**, including all inherited inbound preferences. The actual WAL
contained two Prepare, two Applied and two Restored records, finishing Reverted.
Its before-images remained raw schema-1 values:

```json
{"kind":"prepare","id":"firewall.public.enabled","before":false}
{"kind":"prepare","id":"uac.consent","before":{"present":true,"value":0}}
```

No typed effective/authority metadata was inserted into those originals.

### Failed-operation verification

After successful undo, a temporary native byte-range lock was held on the
disposable `engine.lock`. One approved Apply attempt failed at locking, and the
UI immediately attempted verification, which also correctly failed while that
lock was held. It displayed separate operation/check failures and withheld an
actionable fresh snapshot, rather than claiming success from stale evidence.

The lock was bounded to 120 seconds and explicitly released promptly. No new
WAL, mutation or retry occurred. The original reverted transaction remained
unchanged, and the guide exited normally. Native UI evidence demonstrates the
automatic post-check behavior; exact audit invocation counts, partial/no-op
paths and rendering-error behavior are additionally asserted by the passing
guided-session unit tests. No manual Check again was used between operation and
the observed automatic result.

Evidence: `ui-results/final-ui-transaction.jsonl`, phase captures and screenshots
`ui-preview-back-default.png`, `ui-change-selection.png`,
`ui-apply-awaiting-enter.png`, `ui-post-apply.png`, `ui-post-undo.png` and
`ui-failed-apply-postcheck.png`.

## Installer and LocalService monitor

The full permitted lifecycle passed, including hourly SYSTEM task/ACL/delayed
start, update choice retention, automatic-update markers, nine safe foreign-task
variants, monitor installation/scan/running upgrade/resume, quiet uninstall
rejection/retry and preservation of reports, journals and unrelated files.
Unsafe writable-task-ACL fixtures remained excluded. The existing nine SYSTEM
updater security regressions still passed.

The LocalService monitor report contained **18 observations, 19 findings,
`incomplete: false` and a readiness object**. All six firewall entries had the
correct typed evidence and local authority. System-volume space/read-only, power
and reboot observations were Known. Journal-volume readiness was explicitly
**Unknown** for LocalService because the protected journal path is not readable
in that context; it was not converted to zero space, false or a green claim.
See `monitor-validation.json` and the retained `monitor-report.json`.

The actual interactive 0.5.0 installer was also checked. Its task page showed
desktop and automatic updates checked, monitor unchecked. The finish page showed
launch checked; accepting Finish launched the working scan-first 0.5.0 guide.
It reached the expected zero-candidate baseline menu and exited normally.
Screenshots are `installer-tasks.png`, `installer-finish-launch.png` and
`installer-launched-guide.png`.

Installed offline update check/status returned structured failure as expected.
Networking was not enabled to test an unpublished 0.5.0 feed or to accept an
older production release during basic acceptance. The later genuine 0.4.3 to
0.5.0 live upgrade passed after publication, as recorded at the top of this report.

## Resolved copy observation and remaining limits

- The original candidate's redundant **“Run a new check after undo”** line is
  retained in `ui-post-undo.png` as historical evidence. The owner fixed it in
  the final copy-only candidate, validated above. No source was edited by the
  native validator, and no additional apply/undo fixture was needed for that fix.
- Read-only/zero-capacity storage failures and medium-integrity COM refusal were
  covered by reviewed code and unit fixtures, not by changing the VM's system
  volume or introducing another standard-user fixture in this phase.
- Only English installer/guide interaction was attended. All 13 automated
  localization checks passed. No new Windows 10 or full medium-user/UAC-broker
  claim is made.
- The current live result covers published 0.4.3 to 0.5.0 only. No future
  automatic feature, network WUA search/update operation or website control-plane
  change is claimed.

## Final restoration

After undo restored the deliberate false/0 fixtures, the harness independently
restored their original true/5 values and verified all 18 baseline controls.
It then completed the interactive installer check, uninstalled the test app and
restored the original app/data directories.

`ui-results/final-state.json` confirms:

- All **18 original controls equal baseline**.
- All **original journal/file hashes equal baseline**.
- Exactly one UI transaction, two applied controls and two restorations; the
  transaction is retained as Reverted evidence outside the original journal tree.
- No app, shortcut, task, monitor service or Secblitz process remains.
- Neither original directory is left moved aside.

The UI clone was shut down normally with **NIC1/NIC2 `none`**. It was restarted
only before the UI fixtures, while no mutation was outstanding, to avoid the
evaluation guest's hourly interruption risk. No forced UI close, network access,
private signing-key use or operation on the original user VM occurred.

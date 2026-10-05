# Secblitz v0.3.1 - native keyboard acceptance and real footage

## Actual v0.3.1 keyboard acceptance and footage - PASS (current)

The supplied immutable v0.3.1 binary was staged and verified before the normal cross-build path could be replaced by later updater work:

```text
115d4b9afc66a30ad9ab9ab61ae927d884388779fb1717dfaf6b0de4a61ad5b0
```

Only **Secblitz-W11-UI-Test** was operated on during this acceptance/capture phase. The original user VM received no new input, commands or changes. No product Rust source, installer, dist or website release configuration was edited/promoted.

### Native tests and actual keyboard behavior

- **83 library + 46 CLI tests passed**. Two attended keyboard/resize test functions remained ignored by the automated runner; they are not counted as automated passes. Actual keyboard acceptance below was performed independently in a real Windows terminal.
- Up/Down visibly moved the main highlight.
- Space visibly toggled a fix checkbox. Escape cancelled a checked selection without changes; empty selection also made no changes.
- Enter advanced to a separate confirmation with **No selected by default**.
- Space did not submit/change the confirmation, including when Yes was highlighted. Independent before-Enter captures remained identical to the unsafe fixture.
- **One Enter on Yes applied only Public firewall Enabled=False → True**. Unselected UAC consent stayed 0; all other controls remained unchanged.
- Escape cancelled undo confirmation without changing the applied state. A later single-Enter confirmed undo restored the exact fixture.
- Extra tools were displayed and dismissed with Escape; no password generator or extra action was executed. Escape from the main menu exited normally.
- Final cleanup restored all **18 controls exactly**, with real-time/tamper protection enabled. No network was enabled.

The fixture was limited to Public firewall disabled and UAC consent 0 in the offline clone. No WDigest fixture, exploit, credential reading, AV disabling or system-wide claim of protection was involved. Original journals were retained; the acceptance and recorded apply/undo transactions are all reverted.

### Usable real continuous footage

**Media handoff directory:** `/tmp/opencode/secblitz-v031-footage/`

**Use only:** `secblitz-v0.3.1-desktop-take03.webm`

| Property | Verified value |
| --- | --- |
| Capture | Continuous FFmpeg gdigrab of the actual Windows desktop |
| Codec / dimensions / rate | **VP9 / 1280×720 / 30 fps CFR** |
| Duration / decoded frames | **346.766 seconds / 10,403 frames** |
| Audio | None |
| Size | 21,346,057 bytes |
| SHA-256 | `4a17039c9cfc95572850b8b891575bfb74675bcd4874be1ee2c89d65308123d5` |

The file contains real scan progress, arrow/Space selection, separate confirmation, a real Fixed result, actual undo/restored-original output and an Extra tools glimpse. No screenshots were assembled into the video. Decoded video frames - not just live screenshots - were inspected to verify the meaningful states. `FINAL-contact-sheet.jpg`, `final-frames/`, `FINAL-media-metadata.json`, `SHA256SUMS`, and **`CAPTURE-NOTES.md`** accompany it.

The notes give validated source ranges for a 25–35 second Remotion cutdown (suggested 30 seconds): scan 6–35 s (time-compress), selection 127–137 s, confirmation 137–144 s, clean Fixed table 241–244 s, undo confirmation 247–253 s, restored-original table 332–336 s, extras 340–344 s. Windows Terminal search/scrollback was used only to frame the actual result tables; it is not an app search feature and should be trimmed from the hero navigation.

The raw recording remains visibly **v0.3.1**. It is not a later v0.4/updater demonstration. Keep **“Isolated Windows VM demo”** context, disclose shortened waits, and do not rewrite the actual restored row (which correctly says Needs your choice / earlier setting restored). No credentials, generated password, machine identifier or Technical details page appears in the captured flow.

### Capture QA and restoration

Native VirtualBox recording produced stale video frames despite correct live screenshots; QA rejected it. A host virtual-display frontend also failed to render reliably. Neither was substituted with a screenshot montage. The final take uses a checksum-verified portable FFmpeg 9.0.2 Windows build to capture the actual guest desktop. Rejected files are marked under `discarded/` and must not be used.

A noninteractive background observer recorded the real Public Enabled transitions **False → True → False**, with UAC consent staying 0. It then restored the original baseline **after recording stopped**. The observer task was removed. Independent final state: app count 0, service count 0, observer-task count 0, Defender real-time=true, tamper=true. All 18 captures match the original baseline; history exits 0 and all eight retained transactions are reverted.

Snapshot before this phase: `secblitz-pre-v031-media`, UUID `5510f03e-5b7c-4cdd-b111-eda7be8ae2a4`. UI-clone restarts needed during capture-tool troubleshooting occurred only after baseline restoration and with recording disabled. Its expired evaluation license remains an environment constraint. No original VM was restarted or modified.

Evidence/verification: `native-tests/`, `acceptance/`, `take03-evidence/`, `final-evidence/`, `verify_acceptance.py`, and `inspect_take03.py` under the handoff directory. The structural verifier passed cancellation, selected-only mutation, exact undo, recorded transitions, all-18 restoration and clean final state. No Remotion render was produced here; the separate media agent owns editing. Windows 10 and the original-nonadmin broker remain unvalidated.

---

## Earlier v0.3 diagnosis and pre-handoff plan (historical)

Date: 2026-10-02. **Ready for the coordinating agent's v0.3.1 binary handoff.** No v0.3.1 runtime acceptance or release promotion is claimed yet. Product Rust source, installers, distribution artifacts and website configuration were not changed by this diagnosis.

## User VM: strictly read-only observations

Target inspected: **Secblitz-W11-Test**. Its observed VM state was running at 1024×768; state-change timestamp `2026-10-02T18:36:14.166000000` UTC.

- Screenshot `/tmp/opencode/secblitz-v031-user-vm-readonly.png` shows the desktop, **not an active Secblitz console**.
- A read-only `Get-Process -Name secblitz` probe found **0 processes** at inspection time. A running image path/version or live console mode therefore could not be inspected. This does not assert what was running when the user experienced the problem, or what they launched later.
- The installed file **`C:\Program Files\Secblitz\secblitz.exe`** exists and its SHA-256 exactly matches the validated final **v0.3.0** release:

  ```text
  5762aa166c789523e663b3eeb98867dc9f8ef6f5fc3ea1aa636e09933801d8d1
  ```

- Windows file-version numeric parts returned zero; this executable lacks a usable PE VERSIONINFO resource. The resource source inspected contains only the manifest/icon, so numeric zero is **not evidence that the application is version 0.0.0**. Artifact identity is established by the known release hash.
- VirtualBox stdout retrieval returned `VERR_NOT_IMPLEMENTED`. To avoid writing diagnostic files into the user's guest, the small read-only probes returned explicit diagnostic values through process exit codes: count **0**, known-hash match code **30**. These are diagnostic encodings, not Secblitz application exit codes.

No keyboard/mouse input, application launch, process stop, installer, reboot, snapshot restore, configuration change or explicit file write was performed on the user VM. Only screenshots and noninteractive read-only process/file metadata/hash queries were used. Credential contents were not printed or written into evidence.

## Confirmed v0.3 input-model behavior

The inspected v0.3 guided implementation holds a `stdin.lock()` and reads **complete text lines** with `BufRead::read_line`. It trims whitespace and interprets literal menu numbers. It does not implement an arrow-key focus cursor or Space-toggle selection:

- Main menu: empty/whitespace input followed by Enter takes the Exit branch.
- Fix selection: empty input selects nothing.
- Confirmation: only literal `1` is Yes; empty input is No.

This is a confirmed interaction-model mismatch with the requested arrow/Space/Enter experience. It is **not evidence that the user's reported session deadlocked**, and it does not rule out a focus, selection or unfinished-scan issue during their earlier attempt.

## Reproduction on the separate owned UI clone

All input/testing below was confined to **Secblitz-W11-UI-Test**, UUID `4b70288b-b64d-4796-a725-006da3162d0f`. The user VM received no injected input.

Used the exact known v0.3 executable above, staged temporarily under `C:\Windows\Temp\SecblitzV031Diagnosis`. No installer ran. A real, **unmaximized** Windows Terminal/cmd console was opened at the 1024×768 desktop; streams were not redirected for the guide.

Observed sequence:

1. Waited for the initial check to finish and the numbered `>` main menu to appear.
2. Pressed Down and Space. **No menu highlight moved or checkbox toggled**; Space was ordinary line input.
3. Pressed Enter. The guide returned **exit 0**, and the batch printed `GUIDE_EXIT=0`.
4. Independently compared all 18 controls before/after: **identical**. Final app count and monitor count: **0 / 0**.

Evidence outside the repository:

- `/tmp/opencode/secblitz-v031-v030-menu.png`
- `/tmp/opencode/secblitz-v031-v030-arrow-space.png`
- `/tmp/opencode/secblitz-v031-v030-exit.png`
- `/tmp/opencode/secblitz-v031-diagnosis/` and `secblitz-v031-diagnosis.zip`

The repro was responsive to input and exited normally. It demonstrates a specific way the old interaction can appear not to work when used as an arrow/checkbox menu, without proving that this was the user's exact sequence.

## Console mode / freeze assessment

A read-only same-console helper sampled console flags immediately before and after the clone's v0.3 run. It called documented `GetConsoleMode` and `GetConsoleSelectionInfo`; it did not call SetConsoleMode, clear selection, or modify console registry settings.

| Field | Before / after |
| --- | --- |
| Input mode | `0x1F7` / `0x1F7` |
| Line input | true / true |
| Echo input | true / true |
| Virtual-terminal input | false / false |
| QuickEdit enabled | true / true |
| Selection flags | **0 / 0** |
| stdin/stdout/stderr redirected | false / false / false |

**Enabled QuickEdit is not the same as an active selection freeze.** No selection was reported at the sampled instants, and the reproduction accepted Enter and exited. These are clone pre/post measurements, not live measurements of the user's vanished console. Windows Terminal client-side selection/focus behavior and the user's earlier state remain unproven.

## v0.3.1 native acceptance plan after handoff

Do not run an in-progress artifact or claim arrow support based only on unit mocks. Once the final binary/hash is supplied:

1. Verify the exact artifact hash and CLI version on the UI clone; do not use absent PE metadata as the version gate.
2. Run the real guide without redirected streams in an unmaximized console, then at the 1024×768 maximized viewport. Wait for the menu to be displayed so scan/progress latency is distinguished from input failure.
3. Confirm Up/Down visibly change the focused menu item and Enter activates that item.
4. Confirm Space toggles only the focused fix checkbox, with all fixes initially unselected; Enter moves to review, not an immediate repair.
5. Verify empty selection, Escape/Back and default-No confirmation make no changes. Capture the actual hint text and selected state.
6. Verify a bounded selected subset and undo only after the coordinating agent authorizes its fixture; compare all 18 controls independently and restore the baseline.
7. Check the actual live input implementation's use of `stdin.lock()`/buffered reads around dialoguer. The old held lock is a point to review, **not a proven new deadlock**: behavior depends on which reader the new terminal backend uses. Native key response and cancellation/mode restoration must be observed.
8. Retain the existing terminal-required and captured-worker/no-hidden-pause gates. Do not treat redirected execution as a substitute for native arrow-key testing.

Original-nonadmin broker coverage remains a separate limitation; no kernel hooks, driver tricks, protocol changes or original-VM input are part of this test plan.

## Ready state

UI clone is running with its desktop available and an idle cmd window after the completed reproduction. No Secblitz process or monitor is running; no security fixture was created and all 18 controls are unchanged. Use only the UI-pinned helpers `/tmp/opencode/secblitz-ui-guest.py` and `/tmp/opencode/secblitz-ui-console.py` for future input/testing. The helper now supports explicit slow Up/Down scan-code input for the upcoming acceptance test.

The current results concern **v0.3 behavior and diagnosis only**. Await the main agent's final v0.3.1 executable/test handoff before testing the replacement interaction.

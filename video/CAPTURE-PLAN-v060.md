# Capture plan: Secblitz 0.6.0 hero video

Product version: 0.6.0 (unreleased at time of capture)
Target: 25-35 s final edit (nominal 28 s), 30 fps, 1280x720, no outro
Composition: SecblitzDemoV060 in sources-v060.json

---

## Terminal window setup

- Application: Windows Terminal (wt.exe) with Cascadia Code 11pt
- Window size: **120 columns x 32 rows** (satisfies the >= 62 col / >= 30 row requirement for the ASCII SECBLITZ block logo)
- Launch command: `wt.exe -p "Command Prompt" --size 120,32 -- secblitz.exe guide`
- Position: maximized or centered on a 1280x720 guest desktop
- Guest display: 1280x720 set via VBoxManage guestproperty before launch
- gdigrab target: `desktop` at 1280x720, 30 fps CFR, VP9 lossless (matches v031 capture method)
- FFmpeg command on guest (background):
  ```
  ffmpeg.exe -f gdigrab -framerate 30 -video_size 1280x720 -i desktop
    -vcodec libvpx-vp9 -lossless 1 -r 30
    C:\Windows\Temp\secblitz-v060-video\take01.webm
  ```

---

## Fixture plan

Two controls are deliberately weakened in the isolated VM before recording so that there are real findings to fix and a real "You're now protected from:" payoff. Both changes are reversible without a restart.

### Fixture 1: Public network firewall disabled

- Secblitz control: Public network firewall
- Baseline value: `Enabled = True` (verify with `(Get-NetFirewallProfile -Name Public).Enabled`)
- Weaken: `Set-NetFirewallProfile -Profile Public -Enabled False`
- Restore: `Set-NetFirewallProfile -Profile Public -Enabled True`
- No restart required.
- Evidence: the before/after JSON written by capture-v060.py confirms baseline and restoration.

### Fixture 2: Defender archive scanning disabled

- Secblitz control: Defender archive scanning
- Baseline value: `DisableArchiveScanning = False` (verify with `(Get-MpPreference).DisableArchiveScanning`)
- Weaken: `Set-MpPreference -DisableArchiveScanning $true`
- Restore: `Set-MpPreference -DisableArchiveScanning $false`
- No restart required.
- Evidence: same baseline JSON.

Both fixtures are recorded in sources-v060.json `fixtureDetails` at capture time. Secblitz reports exactly two items needing fixing: the status card shows "16 of 18 checks protected" on the home screen. The payoff shows both items verified fixed. Undo restores both fixture values, shown as "Needs your choice" in the final undo result screen.

---

## Storyboard (~28 s final, 30 fps)

Phase labels match sources-v060.json segment phases.

| Seg | Phase | Source action | Rate | Output |
|-----|-------|---------------|------|--------|
| home | check | Hold home screen: ASCII SECBLITZ logo, status card "16 of 18 checks protected", highlighted menu | 1x | ~2 s |
| scan | check | Live scan checklist ticking through all 18 checks; last tick completes | 5x | ~5 s |
| report | choose | Grouped report: two items visible with Risk / Protects you from / Why it matters lines | 1x | ~3 s |
| choose-fixes | choose | Navigate to "Fix recommended" or "Review and choose fixes"; recap lists both fixes with risk | 1x | ~3 s |
| apply | fix | Apply runs + automatic post-check runs | 3x | ~3 s |
| payoff | fix | "You're now protected from:" list, both items verified; hold for viewer to read | 1x | ~4 s |
| undo-nav | undo | Navigate back to home, then to Undo; undo prompt visible | 1x | ~3 s |
| undo-result | undo | Undo runs; result shows "Your earlier setting was restored" for both controls | 1x | ~5 s |

Total: approximately 28 s. End on the undo result screen (real footage). Apply a short 15-frame linear fade at the end of the last segment only (via crop/playbackRate adjustment). No brand close, no outro.

---

## Keystroke script (one continuous take)

Approximate key sequence for the capture driver. Exact timing is tuned to the actual binary during Phase B.

```
LAUNCH: wt.exe --size 120,32 -- secblitz.exe guide
WAIT:   "Your next step"           # home screen with SECBLITZ logo
HOLD:   3 s                        # let viewer read logo + status card
ENTER                              # activate highlighted menu item (Fix recommended)
  branch A: "Confirm selected fixes" visible (fix recap shown)
    HOLD 2 s                       # read the fix recap
    DOWN (highlight Yes)
    ENTER                          # confirm apply
  branch B: "There are no recommended" visible
    ESC (back to home)
    DOWN                           # move to "Review and choose fixes"
    ENTER                          # open review/report
    HOLD 2 s                       # read the grouped report
    SPACE                          # select fix 1
    DOWN                           # move to fix 2
    SPACE                          # select fix 2
    DOWN x2 (move to Apply)
    ENTER                          # confirm apply
WAIT:   apply progress completes   # auto post-check also runs
WAIT:   "You're now protected from:" visible
HOLD:   4 s                        # payoff hold
ESC                                # back to home
DOWN x3                            # navigate to Undo (item index may differ from v031)
ENTER                              # open Undo
WAIT:   undo prompt
DOWN                               # select Yes
ENTER                              # confirm undo
WAIT:   undo complete
HOLD:   3 s                        # read undo result
DOWN x4 (or EXIT menu item)
ENTER                              # exit
```

Note: exact DOWN counts for reaching Undo and Exit from the v060 home menu must be confirmed with the actual binary before the live take. The capture driver script (scripts/capture-v060.py, driver scripts/capture-v060-driver.ps1) has these as named constants that the operator updates after a dry run.

---

## Native test step before capture

Run these before starting the recording to confirm the binary is healthy on the guest:

1. `secblitz.exe --version` - confirm version string shows 0.6.0
2. `secblitz.exe audit --json` - confirm JSON audit output, 18 results, status shows 2 findings needing attention
3. `secblitz.exe guide` (brief interactive run) - confirm home screen ASCII logo renders in >= 32-row terminal

---

## Restore procedure

After recording (run regardless of whether recording succeeded):

```powershell
Set-NetFirewallProfile -Profile Public -Enabled True
Set-MpPreference -DisableArchiveScanning $false
```

Verify with:
```powershell
(Get-NetFirewallProfile -Name Public).Enabled      # must be True
(Get-MpPreference).DisableArchiveScanning           # must be False
```

The capture driver writes a fixture-verification JSON before and after. The before snapshot is taken before weakening; the after snapshot confirms restoration. These go into `target/video-v060/` for the manifest.

---

## Manifest population (after capture)

After a good take, populate `video/sources-v060.json`:

- `status`: `"ready"`
- `provenance.captureMethod`: `"Uninterrupted native Windows desktop recording with FFmpeg gdigrab, VP9 lossless, 1280x720, 30 fps CFR. One take."`
- `provenance.versionEvidence`: `"Visible v0.6.0 banner; frozen executable SHA-256 <hash>."` (from capture-v060.py output)
- `provenance.originalRecordingPath`: path in `target/video-v060/` where the recording was copied
- `provenance.fixtureDetails`: `"Public network firewall disabled and Defender archive scanning disabled in isolated VM. Both controls selected and fixed on camera. Undo restores both, shown as Needs your choice. Capture operator verified baseline restored."`
- `provenance.presentationNote`: `"No outro. Edit ends on real footage (undo result screen) with a short 15-frame fade."`
- `posterFrame`: choose a frame from the payoff segment showing the "You're now protected from:" screen
- `fixturesUsed`: `true`
- `sources[0]`: sha256 of the recorded file in `video/public/capture/`, ffprobe-verified dimensions and duration
- `segments`: populate after reviewing the recording. See storyboard above for the segment breakdown. Run `npm run verify-v060` after populating.

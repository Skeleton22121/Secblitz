# Remove Secblitz: keep or put back

Status: draft for owner review. Target release: 0.8.0.

## Goal

Removing Secblitz asks one question, wherever the person starts it:

- **Keep my PC as it is now.** Protection stays as it is. Secblitz, its
  background parts and all its data are removed. Removed apps stay removed,
  and can still be reinstalled from the Microsoft Store.
- **Put everything back the way it was.** Secblitz first undoes every change
  it made, newest first: settings, personal settings, removed apps (restored
  from their saved copies) and suggested-apps blocking. Then it removes itself.

Decisions already made by the owner: removed apps are restored in "put
back"; uninstalling from Windows Settings > Apps asks the same question.

Neither choice can undo Windows updates, virus scans, Bitwarden or anything
changed outside Secblitz. The sheet says so.

## What exists today (and the gaps)

- `Engine::revert` (src/engine.rs:1783) undoes only the newest unreverted
  batch, and a batch with a conflict or skip stays unreverted. Calling it in a
  loop would stall on the first conflict. **Gap: no "undo everything".**
- Personal (HKCU) settings have their own journal and undo
  (src/user_settings.rs), run as the person through the broker, one at a
  time. **Gap: no bulk undo.**
- Removed apps: `debloat::offline::restore_index` restores from a saved copy
  offline. Without a copy, the Store reinstall goes through the broker.
- "Block suggested apps" writes one machine policy value and seven per-user
  values and records none of the old values. **Gap: no undo at all.**
- The Inno uninstaller removes the monitor, update task, files and shortcuts.
  It leaves `%ProgramData%\Secblitz` (journals and app copies, often several
  GB), `%LOCALAPPDATA%\Secblitz`, `HKLM\Software\Secblitz`, `{app}\Monitor`,
  and a tray Run value turned on from the app. It never reverts anything.

## Building blocks to add

1. **`Engine::revert_all`.** Goes through every unreverted batch, newest
   first, applying the existing per-entry rules (unchanged, conflict, skipped,
   restored). A batch with a conflict stays unreverted, is reported, and
   processing continues with older batches. Safe because each entry is only
   restored if the setting still holds the value Secblitz wrote, so an older
   batch can't overwrite a newer one. Same lock, interlocks and write-ahead
   journal as `revert`. If interrupted, the existing recovery finishes or rolls
   back the entry in flight.
2. **`user_settings::undo_all`** for the current person, plus a broker request
   `UserSettingsUndoAll`.
3. **Undo for "Block suggested apps."** From 0.8.0 the old values (machine
   policy and per-user values, "absent" included) are recorded in the
   clean-up journal before the first write, and an undo puts them back. A
   block made by 0.7.0 has no record: "put back" leaves it and lists it as
   "made by an older version, left as it is". It never guesses old values.
4. **A hidden CLI subcommand `uninstall-revert`** (machine part, elevated) and
   `uninstall-revert --user` (personal part, as the person). JSON on stdout
   only, like `update health`. Both go through the existing argument
   allowlist. Used by the Inno uninstaller.
5. **A full cleanup step in the uninstaller**, for both choices: turn off web
   protection (NRPT rule, then the filter service; see the web protection
   spec), then the monitor, update task, tray Run value, `{app}\Monitor`,
   `%ProgramData%\Secblitz`, `HKLM\Software\Secblitz`, and the current
   person's `%LOCALAPPDATA%\Secblitz`. Paths are fixed. Each path is checked
   (owner, no reparse points) before deletion, like maintenance.ps1 does
   today.

## In the app

Settings gets a last group, **Remove Secblitz**, before About, with one
button. It opens a sheet:

> **Remove Secblitz**
> What should happen to the changes Secblitz made?
>
> ( ) **Keep my PC as it is now**
>     Your protection stays on. Apps you removed stay removed; you can
>     reinstall them from the Microsoft Store.
> ( ) **Put everything back the way it was**
>     Secblitz undoes its 12 changes and brings back 3 removed apps first.
>     This can take a few minutes.
>
> Windows updates, virus scans and apps you installed with Secblitz stay.
>
> [Cancel]  [Remove Secblitz]

Counts come from the journals. Lines with nothing to undo are hidden, and the
"put back" choice is hidden if there is nothing to put back.

**Put back** then shows a progress view, the same pattern as fixing:
settings, personal settings, removed apps, suggested-apps blocking, web
protection, one line each, ticked as it finishes. Removed apps without a saved
copy are reinstalled from the Store if the PC is online, otherwise listed as
not restored.

If something couldn't be put back, the result lists it in plain words (for
example "Firewall at home: you changed this yourself since, so it was left as
it is") with two buttons: **Remove Secblitz anyway** and **Keep Secblitz**.

**Removing:** the app starts `{app}\unins000.exe /VERYSILENT /SUPPRESSMSGBOXES
/NORESTART /SECBLITZDONE` and closes immediately, so no Secblitz file is in
use. `/SECBLITZDONE` tells the uninstaller the question was already answered
and the put-back already done.

A copy that isn't installed (portable exe) shows "Put everything back" only,
with no uninstall step, followed by "You can now delete secblitz.exe."

## From Windows Settings > Apps (Inno uninstaller)

- Interactive uninstall shows the same two choices on a custom page right
  after Inno's own "Are you sure" box (`usAppMutexCheck`) and before anything
  is removed. Cancel leaves everything untouched.
- **Put back:**
  1. Run `secblitz.exe uninstall-revert --user` hidden, for personal settings
     and the per-user part of suggested apps. Inno allows
     `ExecAsOriginalUser` only in Setup (the uninstaller raises), so this runs
     elevated: the same person's account when they approved the prompt
     themselves. The in-app path runs it as the person in every case.
  2. Run `secblitz.exe uninstall-revert` elevated, for settings, removed apps
     with a saved copy, and the machine part of suggested apps.

  The uninstaller's status text says "Putting your settings back". Store-only
  apps are not reinstalled here (no broker); the summary lists them.
- **Results:** failures show in one message box with the same plain lines,
  then removal continues. The person already chose to remove Secblitz.
- **Silent uninstall** (`/VERYSILENT` without `/SECBLITZDONE`, e.g. scripts or
  IT tools) keeps changes. No prompt, as today.

## Limits, stated in the UI

- Only the person removing Secblitz gets their personal settings put back.
  Other Windows accounts keep theirs.
- Store-only app reinstalls need internet.
- Settings changed since by the person or by Windows are left as they are and
  listed.
- "Keep" deletes the saved app copies to free the space, so those apps can
  then only come back from the Store. The sheet says so.

## Testing

- **Unit tests:**
  - `revert_all` with several batches, a conflict in the middle, a skipped
    entry, and an interrupted entry (recovery);
  - recording and undo for suggested-apps blocking, including absent values
    and a 0.7.0 block with no record;
  - the CLI allowlist for the new subcommand;
  - the sheet counts.
- **Installer tests:** extend installer/test-maintenance.ps1 and
  test-lifecycle.ps1 for full cleanup, path checks and the silent default.
- **VM, both entry points and both choices:**
  - apply fixes, change personal settings, remove apps (with and without
    copies), block suggested apps and turn on web protection;
  - then remove Secblitz;
  - check every value against a before-snapshot;
  - check that no service, task, Run value, NRPT rule, ProgramData folder or
    install folder is left;
  - check that no console window ever appears.

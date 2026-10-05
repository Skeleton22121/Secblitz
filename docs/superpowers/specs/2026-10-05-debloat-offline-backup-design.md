# Clean up apps: offline backup and restore

Date: 2026-10-05. Status: implemented and verified on the UI test VM.

## Goal

When Clean up apps removes an app, the app is removed completely from
Windows, but Secblitz keeps a backup of it (and, encrypted, each account's
data for it) so it can be brought back at any time, **offline**, from
Secblitz itself.

What the user asked for, verbatim intent:

- "always backup the app and files inside a state of secblitz so the apps
  removed are completely removed from the system but can be restored any
  time, offline from secblitz itself"
- Approach chosen: offline folder copy.
- App data: "App and data, but encrypted".
- UI: "no dev fluff" (plain words only; no hashes, manifests, paths, sizes
  in bytes, package names).

Success criteria:

1. A removed app is gone from `Get-AppxPackage -AllUsers`, the provisioned
   list and `C:\Program Files\WindowsApps`.
2. With the network disconnected, Restore brings the app back for every
   account that had it, Store-signed (`SignatureKind = Store`, not
   development mode, `Status = Ok`), and it launches.
3. Each account's app data comes back with it.
4. A modified backup is never run, and nothing in the backup can make the
   elevated process write outside the intended folders.
5. Apps removed before this feature keep today's Store reinstall.

## Feasibility (probed on the UI test VM, 2026-10-05)

With Microsoft.BingWeather 4.54.63045.0 (bundle + x64 main + scale-100
resource, five framework dependencies):

| Step | Result |
|---|---|
| Copy package folders with backup semantics (`robocopy /B`) | 30.8 MB, 461 files, `AppxSignature.p7x` and `AppxBlockMap.xml` included |
| `Remove-AppxPackage -AllUsers` | removed; folders gone from WindowsApps |
| `Add-AppxPackage -Register <backup>\AppxManifest.xml -DisableDevelopmentMode` | refused: "manifest is not in the package root" |
| `Add-AppxProvisionedPackage -FolderPath <backup>` | refused: needs packed `.appx`/`.msix` |
| Copy back to `WindowsApps\<PackageFullName>` without ACLs, register | `0x80070005` opening the package |
| Copy back, apply WindowsApps ACLs (SYSAPPID condition for **this** family), register the bundle manifest | bundle, main, resource all `Status = Ok`, `SignatureKind = Store`; app launches |
| Same with one byte flipped in a DLL | registers, but `Status = Modified, NeedsRemediation` and Windows refuses to launch it |

Notes:

- AppX cmdlets need a real user profile; they fail from a service or
  guest-control session ("Failed to determine the type of current user's
  profile"). Secblitz runs them from its elevated desktop process, as today.
- The technique is not documented by Microsoft. If a future Windows rejects
  it, restore falls back to the Store reinstall that exists today.

## Design

### Units

| Unit | Purpose | Depends on |
|---|---|---|
| `debloat::backup` (Rust) | Backup store layout, manifest, hashing, validation, retention | `platform::state_dir`, `sha2` |
| `debloat::vault` (Rust) | AES-256-GCM encryption of app data with a DPAPI-sealed key | Windows CNG (`BCrypt*`) and DPAPI (`CryptProtectData`) via `windows-sys` |
| `debloat/scripts/backup.ps1` | Describe the installed package set (identities, folders, dependencies, per-account data folders) as JSON; copy package folders with backup semantics | inbox Appx module |
| `debloat/scripts/restore.ps1` | Copy validated folders into WindowsApps, apply ACLs, register, provision for all users | inbox Appx module |
| GUI (`pages/debloat.rs`) | Plain-language copy, Restore / Delete backup, progress | the above |

Rust owns every security decision (what to copy, where, whether the backup
is intact). PowerShell only performs Appx operations on values Rust
validated, passed through environment variables as today.

### Store layout

```
<state_dir>\AppBackups\
  frameworks\<PackageFullName>\     shared framework copies, each with
                                    its own framework.json (file hashes)
  <PackageFamilyName>\
    backup.json              manifest (see below)
    packages\<PackageFullName>\...   main, resource and bundle folders
    data\<UserSid>.bin       encrypted app data, one per account
    key.bin                  DPAPI (machine) sealed AES key for this backup
```

Frameworks (VCLibs, UI.Xaml, .NET Native and similar) are shared by many
apps, so each is stored once and referenced by full name from every
manifest that needs it. A framework copy is deleted only when no manifest
references it any more.

`<state_dir>` is the existing protected Secblitz state directory (SYSTEM and
Administrators only, pinned and checked for reparse points by the existing
`platform` helpers). One backup per package family; a new removal of the
same family replaces it only after the new backup is complete.

### Manifest (`backup.json`)

- schema version, created time, catalog index and family name
- each package: full name, kind (bundle, main, resource, framework),
  publisher id, version, architecture, and every file as
  `{relative path, size, sha256}`
- each data blob: account SID, profile folder name, plaintext size, sha256
  of the ciphertext
- whether the app was provisioned (to restore for new accounts too)

Relative paths are validated on write and read: no `..`, no rooted or
drive-relative paths, no alternate data streams (`:`), no reparse points,
length limits, at most 50 000 files and 4 GB per backup.

### Remove flow (per app)

1. `backup.ps1 describe` returns the package set for the catalog family,
   dependency frameworks, provisioned state and per-account data folders.
   Rust checks every identity against the catalog entry (family name and
   publisher id) and the existing protected-package rules.
2. Free space check: needs the measured size plus 1 GB headroom. If short,
   this app is skipped with "Not enough free space to keep a copy of this
   app, so it was left installed."
3. Copy package folders into a staging folder
   (`AppBackups\.staging-<random>`) with backup semantics; copy any
   dependency framework not already in the shared store; hash every file
   and write the manifests.
4. For each account with a data folder: read it with backup semantics
   (no reparse point is followed), pack it into a simple length-prefixed
   archive, encrypt with AES-256-GCM (random 256-bit key per backup,
   random 96-bit nonce per 1 MB chunk, chunk index and family bound as
   associated data), write `data\<sid>.bin`.
5. Seal the key with `CryptProtectData` (`CRYPTPROTECT_LOCAL_MACHINE`,
   fixed entropy) into `key.bin`.
6. Re-read and verify the staged backup against its manifest; atomically
   rename staging to `<family>` (replacing an older backup).
7. Only now run the existing `remove.ps1`. If removal fails, the backup is
   kept (harmless) and the error shown as today.

### Restore flow

1. Load and validate the manifest; re-hash every file. Any mismatch:
   "This app's saved copy is damaged, so it can't be brought back from
   Secblitz." plus the existing Store option.
2. Check identities again against the catalog entry.
3. Missing frameworks first, then the app's packages: for each,
   `WindowsApps\<PackageFullName>` must not exist (if it exists and is
   registered, skip; otherwise refuse). Copy with restore semantics.
4. ACLs: take the security descriptor of a live, registered Microsoft
   package folder and file in WindowsApps as the template, replace its
   `WIN://SYSAPPID` family with this package's family, apply to the
   restored folders and files. The template is checked to contain the
   expected owner (SYSTEM) and no grant of write to Users or Everyone.
5. `Add-AppxPackage -Register <bundle or main manifest>
   -DisableDevelopmentMode`, frameworks first.
6. If it was provisioned, or other accounts had it,
   `PackageManager.ProvisionPackageForAllUsersAsync(family)` so every
   account gets it at its next sign-in.
7. Data: decrypt each `data\<sid>.bin` and write it to that account's
   `LocalAppData\Packages\<family>`. The target path is built from the
   profile list (not environment variables); every component is opened
   without following reparse points and checked to be a plain directory;
   the app's data folder itself is created by Windows when the app is
   registered for that account (Secblitz never creates it); files are
   written without following links and owned by that account. Accounts
   that are signed out have no folder yet: their data waits in the backup
   (`pending.json`) and is put back the next time that account opens
   Secblitz. A planted junction or symlink
   anywhere on the path stops the data restore for that account with a
   plain message; the app itself stays restored.
8. Verify `Get-AppxPackage` shows the packages with `Status = Ok`, mark the
   journal entry restored. The copy has then done its job and is deleted
   (removing the app again makes a fresh one), unless it still holds data
   for an account that hasn't signed in yet; that copy goes once the last
   account has its data back. The same applies after a Store reinstall.

### Security notes

- Integrity: two independent checks. Secblitz's own hashes against a
  manifest in the protected folder, and Windows' package signature, which
  the probe showed refuses to run modified files.
- The backup store is writable only by SYSTEM and Administrators. An
  administrator can already install software, so the backup gives no new
  capability to anyone.
- Encryption protects app data if the backup files are copied off this PC.
  It does not protect against an administrator of this PC (the
  machine-scoped key is usable by any elevated process); BitLocker remains
  the protection for a stolen drive. Users' own DPAPI keys can't be used
  because accounts other than the signed-in one aren't logged on.
- Restore never takes a path, name or identity from the backup without
  re-validating it against the compiled catalog.

### UI (plain words only)

- Remove review sheet adds one line: "Secblitz keeps a copy, so you can
  bring these apps back any time, even without internet." With low space:
  the per-app skip message above.
- Removed apps rows: **Restore** button. With a backup it restores offline;
  without one it uses the Store as today. A second action, "Delete saved
  copy", exists only when a backup exists, so those rows show the "..."
  menu with both; others show the single button.
- Progress: "Bringing back Weather..." with the existing busy spinner.
- Results: "Weather is back." / "Weather is back. Some of its saved data
  couldn't be put back." / the damaged-copy message.
- No sizes in bytes, no file or package names, no technical terms. One
  summary line in the Removed apps tab: "Saved copies use about 120 MB."

### Errors and fallbacks

| Situation | Behaviour |
|---|---|
| Backup copy fails | App not removed; "Couldn't make a copy of this app, so it was left installed." |
| Not enough space | App not removed; message above |
| Backup damaged | Offer Store reinstall (today's path) |
| Windows rejects the restore | Undo partial copies, offer Store reinstall |
| Data restore blocked by a link | App restored, data skipped, plain note |

## Testing

Unit (host):

- manifest round trip, path validation (`..`, rooted, ADS, length, counts)
- identity checks against catalog entries
- archive pack/unpack, chunked AES-GCM framing (with a test key; CNG calls
  behind a trait)
- SDDL family substitution and template checks
- journal: backup present / restored / deleted states

VM (Secblitz-W11-UI-Test only), each verified on screen:

1. Remove Weather and two other catalog apps through the GUI; check they
   are gone and backups exist.
2. Disconnect the VM network; Restore each; confirm launch and
   `Status = Ok`.
3. Data round trip: create data in the app as Tester and as Administrator,
   remove, restore, data present for both.
4. Tamper: flip a byte in a backed-up file; Restore refuses (Secblitz
   hash check) before Windows is involved.
5. Junction: as a standard user, plant a junction at the data target;
   Restore restores the app, skips data, writes nothing through the link.
6. Framework gone: remove a framework the app needs after backup; Restore
   brings it back.
7. Low disk: fill the disk; removal is refused with the plain message.
8. No console windows appear during any of the above.

### VM results (Secblitz-W11-UI-Test, 2026-10-05, release builds)

| Check | Result |
|---|---|
| Remove Weather through the GUI | gone from Windows; copy of 3 packages + 5 frameworks, Tester's data encrypted (no plaintext in the copy) |
| Restore with the network off (twice, English and German) | 3 packages `Status = Ok`, `SignatureKind = Store`, not development mode; 372 data files back, owned by Tester; app launches; copy and unused framework copies deleted afterwards |
| Junction planted in the app's data folder | marker file restored byte for byte; junction not recreated; `hosts` and the `etc` folder unchanged |
| Dev Home (no Store listing, data for three accounts) | back and provisioned; every account's data restored at once (Windows keeps other accounts' data folders), so nothing waited in `pending.json` |
| One byte flipped in a copied DLL | Secblitz refuses the copy, says so, and gets the app from the Store instead; the installed DLL has the original byte; the damaged copy is removed |
| Delete saved copy | confirm sheet; copy and all five framework copies removed; row falls back to the Store |
| 600 MB free | "Kept: not enough free space to save a copy"; app left installed; nothing left in the store |
| Console windows during all of the above | none (Secblitz's PowerShell runs hidden) |

Not testable on this VM: a missing framework at restore time (every
framework Weather and the other catalog apps use is shared with apps that
stay installed, and Windows refuses to remove a framework in use), and data
waiting for an account that hasn't signed in (Windows kept every account's
data folder). Both paths are covered by host tests with a fake Windows.

## Out of scope

- Backing up apps that were removed before this feature.
- Restoring a backup on a different PC.
- Win32 programs (the catalog only lists Store packages).

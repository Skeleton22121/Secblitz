# Defender false positive (Trojan:Win32/Bearfoos.B!ml): findings and plan

Date: 2026-10-06. Scope: secblitz.exe 0.8.0 (MinGW cross build) and the Inno
Setup installer. Goal from the owner: Defender must not flag Secblitz as
malware or as an unwanted app, and no feature may be dropped.

Ground rules for every fix in this document: no obfuscation, no encryption,
no packing, no string splitting or renaming done only to dodge a scanner.
Only changes that make the program more honest and more ordinary are allowed
(less unusual launch patterns, native APIs instead of scripts, real file
details, a real publisher signature, and asking Microsoft for a review).

## 1. What Defender actually did

VM evidence (collected by the coordinator; this work never touched the VM):

- Defender settings on the VM: MAPSReporting=2, SubmitSamplesConsent=2
  (never send), PUAProtection=2 (audit), CloudBlockLevel=0. Signatures
  1.459.565.0, engine 1.1.26080.3.
- All three old detections were `Trojan:Win32/Bearfoos.B!ml` on **files as
  they were written to disk**: `C:\Users\Public\secblitz-080.exe` (a copied
  dev exe), the installed `C:\Program Files\Secblitz\secblitz.exe`, and
  `C:\Users\Public\head-release.exe`. Remediation then also listed what
  pointed at the quarantined file: the `SecblitzTray` Run value, the
  `SecblitzUpdate` task, the `SecblitzMonitor` service, the shortcuts and the
  uninstall key. Those were collateral, not the cause.
- The current release exe and the current setup scan clean with the same
  signatures (MpCmdRun custom scan).

Conclusion: this is a **static, file-based machine-learning verdict on the
PE file**, not a behavior detection. The `!ml` suffix means a model scored
the file; Bearfoos and Wacatac are generic buckets for "looks like a trojan",
not real malware families. Different builds of nearly the same code got
different verdicts, so each new build is a new roll of a probabilistic
classifier. Code changes can lower the score but can never guarantee a pass.
Only a trusted publisher signature plus Microsoft's own review make the
outcome stable.

Sources (web research, 2026-10-06):

- Microsoft, false positives and negatives:
  https://learn.microsoft.com/en-us/defender-endpoint/defender-endpoint-false-positives-negatives
- Microsoft submission guide: https://learn.microsoft.com/en-us/defender-xdr/submission-guide
- Rust binaries flagged by Defender ML: https://github.com/rust-lang/rust/issues/88297
- Go binaries flagged as Wacatac.B!ml:
  https://forum.golangbridge.org/t/trojan-script-wacatac-b-ml-alert-on-windows-for-any-golang-build/35777
- MITRE ATT&CK T1059.001 (PowerShell) lists `-EncodedCommand`,
  `-ExecutionPolicy Bypass` and hidden windows as abuse indicators; T1027.010
  covers command encoding.

## 2. Root causes, ranked

Ranked by how much each one plausibly moves a static PE classifier, from the
evidence in the file itself. Line numbers are from this branch.

### R1. No publisher signature on the exe, the installer or the uninstaller (highest)

- The shipped 0.7.0 exe has an empty Security Directory (no Authenticode),
  checked with `objdump -p dist/secblitz-0.7.0-windows-x64.exe`. Same for
  the 0.8.0 builds.
- The release script already knows how to sign:
  `scripts/build-release.ps1:69` (`Sign-ReleaseFile`), `:161` (signs the exe),
  `installer/setup.iss:52-53` (`SignTool`, `SignedUninstaller=yes`). Without a
  certificate it prints `UNSIGNED PREVIEW` (`scripts/build-release.ps1:187`).
- An unsigned file has no reputation and no identity, so the model judges it
  on its content alone. A valid signature from a known publisher is the
  strongest single counter-signal and lets Microsoft attach reputation to the
  publisher instead of to each new hash.

### R2. No file details (VERSIONINFO) in the exe (high) - fixed

- `assets/secblitz.rc` held only the manifest and the icon. The PE had
  resource types 3, 14 and 24 only: no company, product, description or
  version. Missing version info is a well known feature in static malware
  models (EMBER and similar) because throwaway malware rarely has it.
- Fixed in this branch (see section 3).

### R3. About 3000 lines of embedded PowerShell that talk about Defender (high)

The scripts are compiled in with `include_str!` and sit in plain text in the
exe: `src/platform/backend.ps1`, `src/platform/hardening.ps1`,
`src/actions/defender.ps1`, `src/diagnostics/{common,probes,browsers}.ps1`,
`src/patching/wua.ps1`, `src/debloat/scripts/*.ps1`,
`src/operations/probe.ps1`, `src/updater/health.ps1`. Test-only scripts
(`src/platform/backend.tests.ps1`, `backend.hardening.tests.ps1`, the
`#[cfg(test)]` source scans in `src/i18n.rs` and `src/explain/mod.rs`) are
**not** in the release binary (checked with `strings`).

Every suspicious-looking token has a legitimate reason:

| Token in the exe | Where | Why it exists |
|---|---|---|
| `Set-MpPreference` | `src/platform/backend.ps1:432`, `src/platform/hardening.ps1:585` | Turns Defender protections **on** (real-time, behavior, IOAV, archive, PUA, network protection, cloud level). Readback checks follow. |
| `DisableRealtimeMonitoring` and friends | `src/platform/backend.ps1:282` | Map from check id to the Defender preference name. The value written is `$false` to turn protection on, and undo restores the user's previous value. |
| `Add-MpPreference -AttackSurfaceReductionRules_*` | `src/platform/hardening.ps1:591` | Turns on attack surface reduction rules. |
| `Add-MpPreference -ExclusionPath/Extension/Process` | `src/platform/hardening.ps1:1024-1026` | Only the **undo** of "remove risky Defender exclusions": it puts back an exclusion the user had, and only one that was on the reviewed risky list. |
| `ExclusionPath` (read) | `src/platform/hardening.ps1:1002`, `src/platform/backend.ps1:554`, `src/diagnostics/probes.ps1:258` | Counting and listing exclusions to warn about them. Read only. |
| `iex`, `downloadstring`, `encodedcommand`, `mshta`, `certutil`, `bitsadmin`, `regsvr32` | `src/diagnostics/probes.ps1:602` | Detection regexes: the startup-items check flags Run keys that look like malware launchers. This is a security feature looking **for** those patterns. |
| `FromBase64String` | `src/platform.rs:66` (`ps_text`), `src/patching/script.rs:74`, `src/debloat/scripts/register.ps1:9` | Injection safety: data (Wi-Fi names, update metadata, app lists) travels as base64 so no value can ever become script text. Typographic quotes make plain quoting unsafe (comment at `src/platform.rs:57-61`). |
| `[ScriptBlock]::Create([Console]::In.ReadToEnd())` | one bootstrap per launcher (section R4) | Reads the compiled script from a private pipe, because Windows limits a command line to 32K characters. |

To a static model, this vocabulary looks like a Defender-tampering dropper,
whatever the code actually does with it. Removing the words is not possible
without removing features, so the honest options are: move the work to native
APIs (fewer script lines), or move the scripts out of the exe into separate
signed files (section 4, P5 and P6).

### R4. PowerShell launch pattern (medium for the static verdict, high for behavior) - partly fixed

Before this branch every launcher used the textbook loader shape: hidden
console, `-EncodedCommand <base64>`, and two of them `-ExecutionPolicy
Bypass`:

- `src/platform/windows.rs` (platform and hardening controls, Defender support
  actions): `-ExecutionPolicy Bypass -EncodedCommand`, `CREATE_NO_WINDOW`.
- `src/debloat/windows.rs` (app removal): same.
- `src/operations/windows.rs`, `src/patching/windows.rs`,
  `src/diagnostics/windows.rs`: `-EncodedCommand`, hidden, suspended start then
  job assignment.
- `src/updater/windows.rs` (update health): the **whole** `health.ps1` as
  base64 on the command line.

The encoding hid nothing: the bootstrap is a compiled constant and the real
script comes over stdin. Fixed in this branch (section 3). What remains is
required: `CREATE_NO_WINDOW` (the owner's rule that the GUI never shows a
terminal) and the stdin bootstrap.

### R5. MinGW toolchain output and leaked build paths (medium, unproven)

- The shipped exe is a MinGW cross build: it imports `msvcrt.dll`, has no
  MSVC Rich header and carries GCC strings. A large share of commodity
  malware is MinGW cross-compiled, so this layout is over-represented in
  training data. `scripts/build-release.ps1:17` already targets
  `x86_64-pc-windows-msvc`, but releases have been built with the GNU cross
  toolchain.
- The release exe contains about 840 absolute build paths such as
  `/home/slay/projects/...` and the cargo registry path (from panic
  locations). Odd for a Windows program and a small privacy leak (the
  developer's user name).

### R6. API mix that resembles stealers or ransomware (low to medium, all needed)

Imports found in the release exe, each with its feature:

- `CryptUnprotectData`, `CryptProtectData`, `BCryptEncrypt/Decrypt`: the
  offline app backup vault (`src/debloat/wincrypto.rs:120`, `:215`). DPAPI
  plus AES next to file walking is also what browser stealers and ransomware
  import.
- `AdjustTokenPrivileges` with `SeBackupPrivilege`/`SeRestorePrivilege`:
  saving app data (`src/debloat/winfs.rs:47-66`).
- `WTSQueryUserToken`, `DuplicateTokenEx`, `CreateProcessAsUserW`: starting
  the tray in the user's session after an update
  (`src/updater/tray_session.rs:19-28`, `:214`, `:249`).
- `CreateToolhelp32Snapshot`/`Process32FirstW`: finding running Secblitz
  processes (`src/broker.rs:332`, `src/updater/windows.rs:1094`,
  `src/operations/windows.rs:429`).
- `CreateServiceW`: the monitor and web protection services
  (`src/service/windows.rs:710`, `src/filter/scm.rs:660`).
- `RegSetValueExW` on the DNS client policy key: the web protection rule
  (`src/filter/routing.rs:22`). Rewriting DNS is what "DNS changer" trojans do.
- `LoadLibraryExW` + `GetProcAddress`: optional or undocumented system calls
  (`src/filter/routing.rs:154-162` for `DnsFlushResolverCache`,
  `src/filter/adapters.rs:152-156`, `src/diagnostics/windows.rs:631-637`).
- `ShellExecuteExW` with `runas`: self elevation (`src/launcher.rs:197`,
  `src/platform/windows.rs:123`).
- `GetAsyncKeyState`: comes from the GUI toolkit (winit), not our code.

None of these can go without dropping a feature. They matter because they add
up in the same unsigned, unknown file. A signature (R1) is what offsets them.

### R7. Install footprint (context, not the trigger)

Per-machine Run key (`installer/setup.iss:188`), an hourly SYSTEM scheduled
task (`installer/maintenance.ps1:152`), two services, and an NRPT rule that
sends all DNS to a local filter. Defender did not flag these; it removed them
only because they pointed at a quarantined exe. They are normal for security
software but stop looking normal when the exe is unsigned.

### R8. Installer (low for now)

`installer/setup.iss:381` runs the embedded `maintenance.ps1` with
`-ExecutionPolicy Bypass -File` in a hidden window, and `:647` runs `cmd.exe`
to capture output. The setup currently scans clean. Its script text sits in
Inno's compressed data, so its static weight is small; the runtime command
line is the visible part.

## 3. What this branch changed

Commits (all behavior-preserving, no feature removed):

1. `Add Windows file details to the app and Setup`
   - `assets/secblitz.rc`: a VERSIONINFO block (CompanyName, FileDescription,
     ProductName, FileVersion/ProductVersion 0.8.0, OriginalFilename,
     LegalCopyright, Comments with the website).
   - `src/main.rs` test `windows_file_details_match_the_package_version`:
     fails when Cargo.toml, the .rc or the manifest disagree, so a version bump
     cannot forget them. **On every version bump, update `assets/secblitz.rc`
     (4 places) and `assets/secblitz.manifest`.**
   - `installer/setup.iss`: AppPublisherURL, AppSupportURL, AppUpdatesURL,
     AppCopyright and explicit VersionInfo* for Setup.
   - Visible side effect: Task Manager, the notification area settings and
     Properties now say "Secblitz" with version 0.8.0 instead of a bare file
     name.
2. `Start PowerShell with a plain -Command, not -EncodedCommand`
   - All six launchers pass the same fixed bootstrap as readable text with
     `-Command`. PowerShell runs `-EncodedCommand` and `-Command` through the
     same path; only the decoding step differs. The bootstrap has no double
     quote, so it stays one argument in every quoting scheme we use.
   - Update health (`src/updater/windows.rs`) no longer puts its whole script
     on the command line; it uses the stdin bootstrap like everything else
     (`read_only_output_fed` writes it to a pipe on its own thread so a stalled
     pipe cannot hold up the 30 second deadline).
   - The hand-written base64 encoder in `src/platform/windows.rs` (only used
     for this) is gone.
3. `Run PowerShell with RemoteSigned instead of Bypass`
   - `src/platform/windows.rs` and `src/debloat/windows.rs`. Scripts arrive on
     stdin, so execution policy only applies to the inbox module files they
     import from System32. RemoteSigned always allows local files, so the
     result is the same. The other four launchers never passed a policy.

### String counts in the release exe, before and after

`strings -a -n 3` on `target/x86_64-pc-windows-gnu/release/secblitz.exe`
(MinGW, LTO). "Copies" counts exact-case occurrences; LTO merges identical
literals, which is why six launchers showed only two `EncodedCommand` copies.
UTF-16 strings were checked too and none of these tokens appear there.

| Token | Before | After |
|---|---|---|
| `EncodedCommand` (exact) | 2 | 0 |
| `encodedcommand`, any case (detection regex) | 3 | 1 |
| `Bypass` (exact) | 1 | 0 |
| `bypass`, any case (rest is English prose) | 6 | 5 |
| `powershell`, any case | 152 | 152 |
| `Set-MpPreference` | 5 | 5 |
| `Add-MpPreference` | 4 | 4 |
| `Remove-MpPreference` | 4 | 4 |
| `ExclusionPath` | 6 | 6 |
| `DisableRealtimeMonitoring` | 2 | 2 |
| `iex` as a word, any case (detection regex) | 1 | 1 |
| `downloadstring`, any case (detection regex) | 1 | 1 |
| `FromBase64String` | 3 | 3 |
| `[ScriptBlock]::Create` | 4 | 5 (update health now uses the shared stdin pattern) |
| base64 alphabet table | 3 | 2 |
| VERSIONINFO resource | absent | present |
| Authenticode signature | absent | absent (owner decision) |

The counts in the original brief (`IEX` x6, `Bypass` x6, `powershell` x97)
came from case-insensitive greps of an earlier build; the table above uses
one consistent method on both builds.

Honest assessment: these changes remove the two most famous launcher
indicators and add the missing file details, but the Defender vocabulary in
R3 is untouched because it is the product. The stable fix is R1 (signing) and
Microsoft review, below.

## 4. Plan, ranked by impact versus risk

| # | Step | Impact | Risk | Who |
|---|---|---|---|---|
| P1 | Sign the exe, the installer and the uninstaller | Very high | Low | Owner decision (cost, identity) |
| P2 | Submit each release to Microsoft as a software developer | High (per build) | None | Owner decision |
| P3 | Build releases with the MSVC target | Medium | Medium | Owner decision, VM check |
| P4 | Strip absolute build paths with `--remap-path-prefix` | Low to medium | Low | Next small change |
| P5 | Port read-only PowerShell probes to native Rust | Medium, grows per port | Medium per port | Planned, one probe at a time |
| P6 | Ship scripts as separate signed .ps1 files (after P1) | High | High | Planned, after P1 |
| P7 | Installer: drop Bypass, then move maintenance into the exe | Low | Medium | Planned |
| P8 | Pre-release Defender scan gate | Catches regressions | None | Release checklist |

### P1. Code signing (owner decision, nothing bought)

The pipeline is ready: `scripts/build-release.ps1 -CertificateThumbprint ...
-RequirePublisherSignature` signs the exe and passes the same signtool command
to Inno, which also signs the uninstaller (`SignedUninstaller=yes`). Options as
researched on 2026-10-06; **check prices and eligibility on the vendor sites
before buying**:

- **Azure Trusted Signing** (Microsoft now also calls it Artifact Signing):
  about USD 9.99 per month (Basic, 5,000 signatures) or 99.99 (Premium).
  No hardware token; works from CI. Eligibility has been limited to
  organizations with a few years of verifiable history in the US, Canada, EU
  and UK, and to individuals in the US and Canada only. Microsoft's own
  service, and it is the option Microsoft recommends for SmartScreen and
  Defender reputation. Signing from Linux works with `jsign` (it supports
  this service directly). https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options
- **Certum open source code signing** (Poland, individuals worldwide): around
  EUR 49 per year with the cloud key (SimplySign), more with a physical card.
  Needs an open source project. https://shop.certum.eu/open-source-code-signing-on-simplysign.html
- **SignPath Foundation**: free signing for qualifying open source projects,
  built from a public repository in their pipeline. https://signpath.org/
- **Classic OV certificate** (DigiCert, Sectigo, GlobalSign): roughly USD
  150 to 300+ per year. Since June 2023 the key must live on a hardware token
  or cloud HSM. **EV no longer buys instant SmartScreen trust** (Microsoft
  changed this in 2024), so EV is not worth the premium for this purpose.
- Linux signing tools: `osslsigncode` (PFX or PKCS#11) and `jsign`.

No option gives instant reputation; reputation builds as signed releases are
downloaded without trouble. Signing is still what makes the ML verdict
attach to a known publisher instead of a brand new unknown file.

### P2. Microsoft false positive review (owner decision, nothing submitted)

- Portal: https://www.microsoft.com/en-us/wdsi/filesubmission, choose
  **Software developer**, attach the exe and the setup, name the detection
  (`Trojan:Win32/Bearfoos.B!ml`), and explain what the program does (open
  source hardening tool, website, that it turns Defender protections on).
- A cleared verdict covers that file hash only. Submit every public release
  (exe and setup) before announcing it. Typical wait is a few days; there is
  no published deadline for individual developers.
- Submitting uploads the binary to Microsoft. The VM has
  SubmitSamplesConsent=2 (never send), so nothing has been sent so far.

### P3. Release with the MSVC toolchain (owner decision)

`scripts/build-release.ps1` targets `x86_64-pc-windows-msvc` with a static CRT
and checks the PE (ASLR, DEP, no dynamic CRT). Using it, on Windows or with an
MSVC cross setup, gives a conventional Microsoft-linker PE. Not proven to
help, but cheap to try: build both, scan both with the same signatures on the
VM, compare. Every feature needs the usual VM pass on the MSVC build.

### P4. Remove absolute build paths

Add `--remap-path-prefix=<repo>=secblitz` and
`--remap-path-prefix=<CARGO_HOME>=cargo` to the release RUSTFLAGS
(`target/build-tools/cross-env.sh` for the cross build, `RUSTFLAGS` in
`scripts/build-release.ps1:107` for the MSVC build). Panic messages keep
relative paths. Not done here because both RUSTFLAGS owners live in the build
environment, and the MSVC script cannot be run from Linux to verify it.

### P5. Port read-only PowerShell to native Rust, one probe at a time

Each port deletes script text from the exe and removes a hidden PowerShell
start at run time. Best first candidates (read only, so the lowest risk):

1. `src/updater/health.ps1`: registry ACL checks (`RegGetKeySecurity`) and
   the Task Scheduler COM API (`ITaskService`). The `windows-sys` crate
   already has `Win32_System_Com` and `Win32_Security`.
2. Defender preference and status reads (`Get-MpPreference`,
   `Get-MpComputerStatus`): the WMI class `MSFT_MpPreference` in
   `root\Microsoft\Windows\Defender` via COM, or the documented registry
   values. Writes should stay with `Set-MpPreference`, which is the supported
   path and respects Tamper Protection.
3. The startup-items check in `src/diagnostics/probes.ps1` (Run keys and
   Startup folders): plain registry and file reads.

Each port needs a parity test against the PowerShell result on the VM before
the script part is deleted. This is several days of careful work, not a quick
patch, so it is planned rather than started here.

### P6. Ship the scripts as separate signed files (only after P1)

With a code signing certificate, the .ps1 files can be Authenticode-signed,
installed next to the exe in `C:\Program Files\Secblitz\scripts` (admin-only
ACL), and run with `-File` under `-ExecutionPolicy AllSigned`. That is the
most ordinary pattern there is (Microsoft's own tools do it) and takes the
whole R3 vocabulary out of the exe. It needs: a new integrity design to match
today's pinning (the exe must verify each script's signature and path before
use), handling for the portable exe, updater changes so scripts and exe move
together, and installer changes. High effort and high risk; do it only after
P1 and P2 show whether they are enough on their own.

### P7. Installer

First, `-ExecutionPolicy Bypass` to `RemoteSigned` in `installer/setup.iss:381`
(the script is written to `{tmp}` by Setup itself and has no internet mark).
Later, move the maintenance actions into `secblitz.exe` subcommands (the
uninstaller already calls `uninstall-revert` and `uninstall-cleanup`), so
Setup starts no PowerShell at all. Not changed here because the installer
cannot be tested without the VM and it currently scans clean.

### P8. Release gate

Before publishing any build: copy the exe and the setup to the VM, update
signatures, run `MpCmdRun.exe -Scan -ScanType 3 -File <path>` on each, and run
a full scan after installing. Record signature and engine versions with the
result. Because the verdict is probabilistic, a clean scan of one build says
nothing about the next one.

### Not allowed, and not done

No obfuscation, no encryption or compression of the scripts, no splitting or
renaming of tokens such as `Set-MpPreference`, no packers, no anti-analysis.
Those are detection evasion, they would make a security tool look exactly
like malware to a human analyst, and they would hurt the Microsoft review in
P2.

## 5. Owner decisions

1. Which signing route (Azure Trusted Signing if eligible, Certum open
   source, SignPath Foundation, or a classic OV certificate), and the budget.
2. Whether to submit each release to Microsoft (this uploads the binaries),
   starting with 0.8.0.
3. Whether to switch release builds to the MSVC target (P3).
4. Whether to start the native ports in P5, and in which order.
5. Whether P6 is wanted once signing exists.

## 6. VM checks for this branch

The changes keep behavior identical, but every PowerShell start changed its
command line, so each path needs one real run on the VM:

1. Defender scans: update signatures, then `MpCmdRun -Scan -ScanType 3 -File`
   on the new exe and on a setup built from this branch; then install and run
   a full scan. Record the signature version.
2. Properties of `secblitz.exe` and of the setup: Details tab shows Secblitz,
   version 0.8.0, copyright. Task Manager shows "Secblitz". Apps and features
   shows the publisher link.
3. Checkup (diagnostics) runs every probe, including the startup items,
   Defender, Secure Boot and account probes, with no "unknown" that was not
   unknown before.
4. Fixes: apply and undo one registry control, one Defender preference
   (real-time or PUA), one attack surface rule, one risky-exclusion removal
   and its undo, and the PowerShell 2.0 feature removal (the DISM path).
5. Defender support actions: update signatures and quick scan.
6. App removal (debloat): inventory, remove one app, put it back.
7. Windows updates (patching): discover, download, install one update.
8. Maintenance probe and scan (operations).
9. `secblitz update health --json` as admin from the installed exe: reports
   the task as ready (or absent when updates are off).
10. Confirm no console window flashes for any of the above.

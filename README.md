# Secblitz

## A safer PC. Without headaches.

Check Windows protection, choose the supported fixes you want, and keep a local record for review and undo. Secblitz helps repair specific unsafe settings without pretending every PC needs the same configuration.

**Scan. Choose. Fix. Undo.**

**Live release: [secblitz.lol](https://secblitz.lol/) · 0.6.1 · Windows x64**

- [Download the installer](https://secblitz.lol/downloads/secblitz-0.6.1-windows-x64-setup.exe)
- [Local executable](dist/secblitz.exe) and [verified release checksums](dist/SHA256SUMS)
- [Genuine published 0.4.3 to 0.5.0 live update results](docs/windows-v050-results.md)

Windows 11 Enterprise Evaluation was tested. **Windows 10 remains untested.** The binaries are not Authenticode-signed; the automatic-update manifest is independently authenticated with a pinned Ed25519 public key. These are different guarantees.

## 0.6.1

**0.6.1 is published** ([release record](docs/website-deployment.md#published-061-release)). It is the 0.6.0 engine with a redesigned terminal interface: a block-letter logo on roomy terminals, a status card with an "N of M checks protected" bar, a live scan checklist, and plain-language impact lines ("Risk:", "Protects you from:", "Why it matters:") for every check. After a verified fix, a "You're now protected from:" list appears only for fixes the fresh post-check confirms; restart-required fixes are listed separately. See [report design](docs/report-design.md).

## 0.6.0

**0.6.0 is published.** The public downloads and signed feed were verified after deployment. [Release publication](docs/website-deployment.md#published-060-release) records final hashes and installer acceptance. [Development scope and acceptance](docs/ROADMAP.md#060-development-candidate) distinguish implemented code, native evidence and unfinished work.

The redesigned terminal interface has five main choices:

```text
Fix recommended
Review and choose fixes
Check again
Advanced
Exit
```

- One in-place screen, compact protection/review/unknown badges, restrained colors and a short explanation for the focused action.
- **Advanced** groups Undo, maintenance, diagnostic profiles, selected Windows quality updates, extra tools and technical details.
- Space selects fixes. Enter opens a named recap with restart and Undo facts. **Apply / Change selection / Back** defaults to Back; changing selection retains the checkboxes.
- Apply and the independent post-check update the same screen. Long recaps are pageable; undisplayed content cannot authorize changes. Back returns to the immediate parent.
- Optional activity animation, differential redraw, resizing, `NO_COLOR`, high contrast and `--no-animation`; all six languages remain supported.

New library/CLI work includes copy-on-write recovery journals, durable risk-aware maintenance, 23 read-only diagnostic probe paths, exact-plan quality updates, and staged/delegated Secblitz delivery. These are not evidence that every new native repair or release path has passed end-to-end acceptance. Automated application upgrades, verified backup restores, broad effective-access analysis, Home/Pro/Windows 10 acceptance and external security assessment remain unfinished. Binaries remain publisher-unsigned by the owner's choice.

```powershell
.\secblitz.exe guide
.\secblitz.exe diagnostics run --profile everyday
.\secblitz.exe diagnostics run --profile development --context printers,nas,vpn --json
.\secblitz.exe operations capabilities --json
.\secblitz.exe operations guide
```

Protected maintenance requires an explicit administrator terminal. Original-user browser inventory uses `diagnostics run --original-user` from the owner's normal non-elevated desktop terminal. Diagnostic profiles provide advice, not permission to change additional settings.

## Published 0.5.0: start with a check

Setup defaults to **desktop shortcut on, Open Secblitz on, automatic updates on, optional monitor off**. You can change the installer choices. Open Secblitz normally and approve the administrator prompt for protected-state access.

The app starts with a settings check, not automatic repairs or a virus scan:

1. Use **Up/Down** and **Enter** to choose **Fix recommended first**, or choose individual fixes.
2. Review the exact proposed batch. **Back is the default**; **Change selection** opens unchecked boxes. Use Space to toggle and Enter to review again.
3. Highlight **Apply these fixes** and press **Enter once** to approve only that batch. Space does not approve; extra tools and installs are not included.
4. The guide automatically performs **one fresh check after every attempted apply or undo**, including failures. A failed check is shown separately, never replaced by stale success.
5. **Esc** goes back/cancels; at the root it exits. Back, empty selection and declined approval make no changes.

Reports show **Protection / Status / What happens next**, with informational findings under **More information**, outside issue counts. Windows-inherited firewall Block is recognized as protected without rewriting `NotConfigured`. A separate read-only **Device check** reports system/journal space and read-only flags, power, and Windows Update restart state. Unknown is not interpreted as false or healthy.

Undo restores the newest recorded batch first. Independent completed batches retain their original values, so undoing the latest does not undo earlier work. Undo can be skipped or conflicted if state or authority changed. It is not a complete PC restore and does not reverse updates, scans, software installation or external Settings changes.

## Automatic updates and optional tools

The installer-owned `SecblitzUpdate` SYSTEM task checks for Secblitz releases on an hourly schedule, starting one hour after registration. This is **not a guarantee of installation within an hour**: power-off, sleep, network failure and an open guide can delay it. Missed starts are not configured for immediate catch-up. Setup does not itself run a feed check.

The updater verifies signed metadata and exact installer size/hash, remembers the highest authenticated release it has observed, and atomically persists protected update state. It defers while the installed app is busy and never force-closes your guide or restarts Windows. This updater updates **Secblitz**, not Windows or every installed app.

Extra tools are separate explicit choices:

- Update Defender protection or request a quick scan. Review Windows Security afterward; a returned command is not a clean bill of health.
- Install/start the optional read-only monitor. It writes local reports and never heals settings.
- Generate a **24-character password** in a private interactive terminal. It is not copied, saved to a vault or assigned to an account.
- Install Bitwarden after approval from the original non-elevated desktop account. Vault setup, imports, extensions and MFA remain your choices.
- Open fixed Windows Settings pages through the original-user handoff. Opening a page is not a completed repair. The full standard-user/UAC broker path remains unvalidated end to end.

Checks, history and monitor reports stay local. Requested updates/downloads contact external services; Secblitz has no telemetry/report-upload feature. The [security review summary](docs/SECURITY-REVIEW.md) separates completed fixes and tested rejections from known boundaries, including unsigned initial-download trust.

## Short commands

From the executable's folder:

```powershell
.\secblitz.exe                 # Guided check; no automatic fixes
.\secblitz.exe audit
.\secblitz.exe audit --details --no-animation
.\secblitz.exe history
.\secblitz.exe revert
```

Advanced `apply` deliberately processes the full eligible catalog without the guide's subset selection. Use `guide` to choose fixes. Audit/history also require elevation. For reports and updater automation, use an already elevated terminal:

```powershell
.\secblitz.exe audit --json > .\audit.json
.\secblitz.exe update status --json
.\secblitz.exe update check --json
```

`update check` must run from the trusted installed executable. `WorkerStarted` is a handoff, not installation success; inspect later status. Noninteractive/JSON updater requests never open UAC and all updater commands are non-pausing.

App languages: **English, Spanish, French, German, Portuguese, Italian**. Choose with `--lang en|es|fr|de|pt|it`. `--details` exposes technical evidence; JSON retains stable raw fields/statuses. Guide normal exit 0 is not a security verdict. Audit/apply/revert use 2 for review-required results, 1 for operational failure and 0 otherwise. Updater failure/error is 1; nonfailure outcomes, including busy deferral, are 0.

## What is verified

The genuine published **0.4.3 installation upgraded to published 0.5.0** through its owned SYSTEM task, with no rebuilt starting binary or verification bypass:

| Live gate | Result |
| --- | --- |
| Published upgrade | `Installed`, exact version/hash, **18.865 s** |
| Next current-version check | `UpToDate`, **1.336 s** |
| Task while real guide open | `DeferredBusy`, **1.005 s**; same guide responded to Down-arrow and Esc |
| Resumed LocalService monitor | Fresh **9,195-byte** report: 18 observations, 19 findings, separate typed readiness |
| Audit after upgrade | **18 results / 19 findings**, plus four readiness signals; inherited firewall Block compliant, zero repair candidates |
| Protected release floor | Advanced to signed **0.5.0** hash/timestamps; unchanged after busy check |
| Preservation | All 18 baselines, original journals and eight genuine journal copies unchanged |

Native functional acceptance passed **138 library + 66 CLI tests**, a separately invoked real-readiness smoke, and **nine elevated updater cases as SYSTEM**. Actual UI tests covered approved two-control apply/undo, default Back, Change selection and automatic post-verification after success/failure. The final copy-only follow-up reran the 66 CLI tests; unchanged library/core evidence was retained. The live upgrade did not repeat destructive fault fixtures. Only Windows 11 Enterprise Evaluation build 26200.9457 in the isolated UI clone was used; the user's VM was untouched. [Current evidence](docs/windows-v050-results.md) gives exact attribution.

0.5.0 uses `https://secblitz.lol`. Older clients with compiled `https://beacons.lol` retain direct feed/download compatibility; the old root and new www host redirect safely to the primary site. Mail configuration was not changed. **Published 0.4.0 installations need a manual upgrade** because their old worker cannot self-repair its environment defect.

The homepage video is an actual **0.3.1** VM recording presented through the Remotion-based media work. It is not a live scan or a recording of the new updater. It has no captions or playback controls; do not infer current UI behavior solely from the footage.

## Scope and next work

The catalog remains **18 repair controls and 19 advisory findings**, not 37 vulnerabilities. Four typed readiness signals are separate, not new repairs. Confirmed read-only system/journal volumes or zero available journal bytes block **new repairs**, not Undo; unknown, power and restart information do not invent a blocker. BITS/wuauserv repair stays bounded to fixed service identities. This is not a complete PC fixer, replacement antivirus, DISM/SFC engine or cleanup tool.

Windows 10 general support ended October 14, 2025. ESU and LTSC/IoT exceptions require exact edition/entitlement checks; Secblitz does not verify them. Full original-standard-user broker testing, wider Home/Pro/hardware coverage and Authenticode release validation remain open.

- [Feature guide](docs/FEATURES.md): exact catalog, keyboard flow, optional tools and updating.
- [Security model](docs/security-model.md) and [update contract](docs/update-contract.md): trust, recovery, scheduling and protocol limits.
- [Engineering roadmap](docs/ROADMAP.md): researched priorities and future features, not implemented promises.
- [Security review summary](docs/SECURITY-REVIEW.md): deployed fixes, adversarial evidence, unsigned bootstrap and other known boundaries.
- [Control research](docs/hardening-research.md), [privilege paths](docs/privilege-escalation.md), [permissions design](docs/permissions-design.md).
- [Live 0.5.0 results](docs/windows-v050-results.md); [0.4.3 security acceptance](docs/windows-v043-results.md) keeps earlier security fixes separately attributed.

## Developer appendix

Native MSVC builds require Windows x64, Rust stable, Visual Studio C++ Build Tools and Windows SDK. Packaging also uses rustfmt, Clippy, dumpbin and Inno Setup; runtime does not require these tools. From a VS Developer PowerShell:

```powershell
cargo test --locked --all-targets --target x86_64-pc-windows-msvc
cargo build --locked --release --target x86_64-pc-windows-msvc
.\scripts\build-release.ps1 -IsccPath 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
```

The script derives filenames from Cargo metadata, currently **0.6.0** for development builds. The previously accepted GNU distribution remains `dist/secblitz.exe`, `dist/secblitz-0.5.0-windows-x64-setup.exe` (**3,850,954 bytes**) and `dist/SHA256SUMS`. Use that checksum file and the matching acceptance record for the published release. No real publisher certificate was available; the unsigned status is explicit. `-RequirePublisherSignature` requires an authorized certificate, not a self-signed trust workaround. Native execution of GNU artifacts is not native-MSVC provenance.

On Linux, `cargo test --locked --all-targets` checks portable logic and terminal PTY behavior, not real Windows controls. `scripts/build-release.ps1` also runs the diagnostic, servicing and quality-update PowerShell boundary fixtures. A local successful build is not production-release acceptance or deployment.

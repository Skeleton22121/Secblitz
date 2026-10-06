# Changelog

All notable changes to Secblitz are written here in plain words. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

New work goes under **Unreleased**. `scripts/bump-version.py` moves that section into a
dated version section when a release is prepared (see `docs/RELEASING.md`).

## [Unreleased]

### Added
- **Search on Protection and Clean up apps.** Type a word, or press Ctrl+F, to show only the matching settings or apps. It forgives small typing mistakes and accents, and the choices you already made stay in place while you search.
- **Help and feedback in Settings.** You can report a problem or suggest a feature from Settings. Each one opens a short form on GitHub. Security problems have their own private email address: support@secblitz.lol.
- **Privacy policy.** A plain privacy page says exactly when Secblitz uses the internet and what it keeps on your PC. The installer shows a short summary, and Settings links to the full page.
- Web protection credits the block lists it uses, from AdGuard, EasyList and HaGeZi.

### Changed
- **Checks before it starts.** Fixes, app clean-up, repairs, updates and per-account settings now check what they need before showing any progress. If something is missing, you see why and what to do instead of a progress bar that fails at the end.
- **"Run as administrator" asks the usual way.** Opening Secblitz with "Run as administrator" now reopens it normally and asks for permission once, so every feature works.
- Fix details show only what the row above does not already say.
- The Web protection page no longer repeats its on or off state in a second section.
- Secblitz is open source under the MIT license. The website and README link to the source code.
- Items to fix now say what turning them on protects you from, and their explanations use calmer words.
- Clean up apps no longer ticks any app for you. You choose what to remove.
- When Windows Update is busy, fixes say so and offer to try again, instead of asking you to restart.
- **Smoother, faster screens.** Pages slide in gently, buttons no longer get colored outlines, and the app redraws less, so it feels quicker. Checking again shows the full checking screen with live progress, and when a check, fix or removal finishes, the bar completes before the result fades in.

### Fixed
- **Secblitz no longer hides the Administrator account you are signed in with.** Hiding the built-in Administrator account is only offered when you use a different account, so you can always sign back in.
- The list of fixes in progress sits in the middle of the window, follows the fix that is running, and its scroll bar no longer covers the names.
- The score circle turns green when everything it counts is protected, even when a few steps are still up to you.

### Removed
- Three notes that only reported something another check already fixes: Remote Desktop waiting for connections, file sharing waiting for connections, and accounts without a password.

## [0.8.1] - 2026-10-06

### Fixed
- **Automatic updates from 0.7.0 finish properly.** Updating from 0.7.0 installed the new version but stopped before setting up web protection, restarting background checks and keeping automatic updates, then reported a failure. Setup now completes every step during an automatic update. If your PC already got 0.8.0 this way, this update finishes the job.

## [0.8.0] - 2026-10-06

### Added
- **Web protection.** Secblitz can block ads, trackers and known dangerous websites for the whole PC. It runs as a small background service, keeps today's blocked counts, and offers "Try again" if it stops working.
- **Remove Secblitz.** Settings now has a clear way to remove the app. You choose whether to keep the changes Secblitz made or put everything back the way it was, and the uninstaller cleans up after itself.
- **Put everything back.** One step undoes every change in a single pass, including the apps you removed. Store apps are downloaded again when needed, and the progress bar keeps moving.
- **Three more fixes you can choose, with undo.** Secblitz can now turn off automatic sign-in, stop Remote Desktop connections and switch off the very old file-sharing feature (SMB1) for you, instead of only telling you where to click. Each one is your choice, shows what will change, and can be undone from History. Remote Desktop is never turned off while you are connected remotely, and the old file sharing is left alone while something is using it.
- **Core system protection, fixed for you.** Secblitz can now turn on Memory integrity and, once that runs, the extra kernel stack protection. It only offers this when your PC supports it, nothing manages it, nothing is locked, and none of your drivers look like they would stop working. You choose it, restart once, and can undo it. It is never offered inside a virtual machine. If it is not offered, Protection says why in plain words and names any driver it was unsure about. After the restart, Secblitz tells you if it is not running and offers the undo.
- **Four more fixes you can approve.** Risky background program locations, firewall allowances for programs in Downloads or Desktop, redirected trusted websites in the hosts file, and risky start-up programs now appear under "Needs your attention". Each lists exactly what will change, switches things off instead of deleting them, and can be undone exactly. If anything changed again since, Secblitz leaves it alone and says so.
- **Restart now.** When Windows has been waiting for a restart, the Tools page offers "Restart now" with a confirmation. Programs with unsaved work still ask you first.
- **Old accounts, shared folders and browser warnings.** Secblitz can switch off old accounts nobody has used for a long time (never deleting them), limit folders shared with everyone on your network, and remove a setting that turned off your browser's warnings about dangerous sites. Protection names the accounts and folders it would change, and undo puts each one back exactly.
- **Remove found threats from Tools.** When Windows Security has found something harmful, the Tools page offers to remove it after a confirmation, and then says whether everything was removed, only part, or nothing could be removed.
- **Recovery tools turned back on for you.** When the Windows recovery tools are off but Windows still has their files, Protection offers to turn them back on with Windows' own tool, and the "Recovery tools" health tip has a "Review fix" button. Nothing else about your drives or start-up is touched, and undo turns them off again. If the files are gone, Protection says so in plain words.
- **Health tips point to the fix.** When Protection offers a fix for a health tip, the tip says so and has a "Review fix" button that opens the same review. Nothing changes until you agree. If the fix is not offered, "See why" shows the reason, and if Protection has not checked yet, "Check now" runs a check.
- **Windows file details.** The app and its Setup now show their name, version and copyright in Properties, Task Manager and security prompts.
- A build pipeline on GitHub that builds the app from source, publishes checksums and can attach a proof of where each file was built.

### Changed
- **Opens without waiting.** If Secblitz checked your PC less than an hour ago, opening it again shows that check instead of checking everything again. "Check again" always runs a new check, and Secblitz checks again by itself after Windows restarts or after anything was changed from Secblitz.
- **Clearer messages when something goes wrong.** Every error now says in plain words what happened and what you can do about it, such as signing in with an administrator account, restarting, or checking your internet. "More details" explains the cause instead of showing technical text.
- "Block suggested apps" can now be undone, because Secblitz records what was there before.
- Secblitz now starts PowerShell with a plain command and a normal safety policy, instead of an encoded command and "Bypass". This avoids looking like malware to security software.
- The installer asks whether to keep or put back your changes when you remove Secblitz, and always removes its own service and folders.

### Fixed
- A damaged web protection folder no longer stops the app from starting.
- Web protection counts ad networks as ads, and passes on unusual but valid website names instead of dropping them.
- The uninstaller no longer leaves an empty program folder behind.

## [0.7.0] - 2026-10-05

### Added
- **A new app window.** Secblitz is now a normal Windows app with a home screen, a Protection page, Tools, History and Settings. It replaces the text-based screens.
- **Clean up apps.** Remove the apps Windows came with that you never use. Secblitz keeps a saved copy of each one, so you can bring it back at any time, even without internet.
- **More than 50 checks.** New checks cover Windows support dates, Secure Boot, Defender settings, sign-in options, Wi-Fi, privacy and more. Every check explains in plain words what it is, what happens if it is off, and what changes if you turn it on.
- **Tools.** Defender scans and protection updates, Windows repair, security updates, PC health tips, a password maker and an optional Bitwarden install.
- **Background checks** from the notification area, and settings you can change per user.
- Light and dark mode, and six languages: English, Spanish, French, German, Portuguese and Italian.
- A new shield-and-bolt icon.

### Fixed
- Four checks that could never run on a real PC now work.
- Repair Windows and update installs work when the app is opened from File Explorer.
- A PowerShell injection weakness and two weak spots around administrator access were closed.

## [0.6.1] - 2026-10-04

### Changed
- A redesigned terminal screen and report. Nothing else changed from 0.6.0.

## [0.6.0] - 2026-10-03

### Added
- A broader PC assessment covering Defender, device security, accounts, remote access, updates, recovery, storage and networking.
- Windows repair tools (DISM and SFC) and Windows quality update installs, each approved by you first.
- Compatibility profiles for everyday use, gaming, development and higher security.
- Safer update delivery with a gradual rollout.

### Fixed
- The update health check now recognizes the system account name Windows really reports.

## [0.5.0] - 2026-10-03

### Added
- A recommended fix plan you can review, then approve in one step.
- An automatic re-check after every fix or undo, so you see the real result.
- Four read-only device readiness checks.
- Updates now come from secblitz.lol and are checked against a pinned signature.

### Fixed
- A firewall setting that Windows already blocks is no longer wrongly offered as a fix.
- Items that are only information no longer inflate the "needs attention" count.

## [0.4.3] - 2026-10-03

### Fixed
- Older update files can no longer be replayed to roll a PC back. The newest version, hash and time are remembered safely.

## [0.4.2] - 2026-10-03

### Fixed
- Smaller reliability fixes to the installer and updater.

## [0.4.1] - 2026-10-03

### Fixed
- Smaller reliability fixes to the installer and updater.

## [0.4.0] - 2026-10-03

### Added
- Signed automatic updates, with a command to check for and install them.
- A portable single-file download next to the installer.

## [0.3.0] - 2026-10-02

### Added
- A Windows installer (Setup) with a proper uninstall entry.
- Screens you can use with the keyboard alone.

## [0.2.0] - 2026-10-02

### Added
- 18 security settings that can be checked, fixed and undone, including two Windows service permissions.
- Exact undo: the old value is saved before every change and restored on request.

## [0.1.0] - 2026-10-02

### Added
- The first release. It checks Windows security settings and explains what it finds.

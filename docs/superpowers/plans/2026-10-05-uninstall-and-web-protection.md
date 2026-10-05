# Remove Secblitz + Web protection Implementation Plan (0.8.0)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a person remove Secblitz from the app or from Windows Settings while choosing "Keep my PC as it is now" or "Put everything back the way it was", and add a system-wide DNS filter with three switches (ads, tracking and telemetry, dangerous websites).

**Architecture:** Uninstall is a set of "undo everything" building blocks (engine, personal settings, removed apps, suggested-apps blocking) driven by the GUI sheet or by hidden CLI subcommands the Inno uninstaller calls. Web protection is a new LocalService Windows service (`SecblitzFilter`) answering DNS on loopback, reached through one NRPT rule for `.` that a SYSTEM task keeps right; the app only writes an admin-only config file.

**Tech Stack:** Rust 2021, iced 0.14 GUI, windows-sys 0.59, windows-service 0.8, reqwest 0.12 blocking + rustls, Inno Setup 6, Windows PowerShell 5.1 (only for Appx and NRPT cmdlets), std threads + mpsc (no async runtime, no DNS crate).

**Specs:** `docs/superpowers/specs/2026-10-05-uninstall-design.md`, `docs/superpowers/specs/2026-10-05-web-protection-design.md`. Read both before starting any task.

## Global Constraints

- Build env first: `source target/build-tools/cross-env.sh`. Each parallel agent sets its own `CARGO_TARGET_DIR=target/wt-<task>` to avoid lock contention.
- Gate for every task: `cargo test --locked --all-targets` and `cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets -- -D warnings` both pass. Also `cargo build --locked --target x86_64-pc-windows-gnu` for tasks with `#[cfg(windows)]` code.
- `rustfmt --check --edition 2021 <file>` on every file you create, and on files that were rustfmt-clean before you touched them. Do not reformat files that were already unformatted (`src/gui/pages/fixes.rs`, `fixflow.rs`, `tools.rs`, `src/app/tools.rs`); keep your hunks formatted by hand there.
- Never touch any VM, never run `target/windows-validation-tools/*`. VM verification is done by the lead afterwards.
- The GUI must never open a console window: every child process uses `CREATE_NO_WINDOW` (0x0800_0000) and hidden PowerShell, as `src/debloat/windows.rs` does.
- New user-facing strings: plain words for people with no technical knowledge, no acronyms (no "DNS", "NRPT", "HKCU"), no em dashes, no jokes. Put every new English string in `i18n-pending/<task>.tsv` as `english\tes\tfr\tde\tpt\tit` (all six filled, placeholders like `{n}` identical), then the lead runs `scripts/merge-i18n-pending.py`. Do not edit `src/i18n.rs` directly.
- Nothing crosses the UAC boundary except fixed request codes and parsed enum-like choices: no paths, no command strings. New broker kinds and CLI subcommands go through the strict decoders/allowlists.
- Fixed paths only. Delete only after checking owner (SYSTEM or Administrators) and that no path component is a reparse point.
- `version = "0.8.0"` already in Cargo.toml. Do not change the version.
- Commit in your worktree with `git -c user.email=skeleton22121@gmail.com -c user.name=slay commit`, message ending with the line `Claude-Session: https://claude.ai/code/session_019sBc3UDNnKBXUfzPSA9bDy`.
- Web protection list URLs (compiled in, HTTPS only, redirects refused):
  - `adguard-dns` `https://adguardteam.github.io/HostlistsRegistry/assets/filter_1.txt` (daily)
  - `hagezi-windows` `https://adguardteam.github.io/HostlistsRegistry/assets/filter_63.txt` (daily)
  - `hagezi-tif` `https://adguardteam.github.io/HostlistsRegistry/assets/filter_44.txt` (daily)
  - `adguard-tracking` `https://filters.adtidy.org/extension/ublock/filters/3.txt` (weekly, classifier)
  - `easyprivacy` `https://easylist.to/easylist/easyprivacy.txt` (weekly, classifier)
  - `adguard-base` `https://filters.adtidy.org/extension/ublock/filters/2_without_easylist.txt` (weekly, classifier)
  - `adguard-mobile` `https://filters.adtidy.org/extension/ublock/filters/11.txt` (weekly, classifier)
  - `easylist` `https://easylist.to/easylist/easylist.txt` (weekly, classifier)
  All are served gzip-encoded (checked 2026-10-05; largest is `hagezi-tif`, 13 MB gzip, 52 MB unpacked).

## Review Focus

1. **Put back after the person changed something themselves.** A conflicting setting must be left alone and listed in plain words, and older batches must still be processed. Test: `revert_all_continues_past_conflict` (Task A1).
2. **Filter down or port 53 taken.** Windows must keep resolving through the next NRPT server, and the rule must be removed when the filter can't listen. Tests: `desired_rule_requires_fresh_listening_status` (Task B3) and `bind_failure_reports_port_in_use` (Task B2).
3. **Malformed or hostile DNS packets** (truncated, compression loops, qdcount 0 or 2, 64 KB TCP frames, non-loopback sources). Never panic, never answer non-loopback. Tests: `parse_rejects_*` (Task B1) and `udp_ignores_non_loopback` (Task B2).
4. **A list download that is huge, truncated, redirected or garbage.** The last good set stays in use. Tests: `download_rejects_oversize`, `bad_list_keeps_previous_set` (Task B2) and `parse_ignores_cosmetic_and_regex_rules` (Task B1).
5. **Uninstall when the exe or data is half gone** (portable copy, a missing ProgramData, a silent uninstall by IT tools). Silent uninstall keeps changes and never prompts; the portable copy offers put back only. Tests: installer `test-lifecycle.ps1` silent case (Task A5) and `portable_sheet_offers_put_back_only` (Task A6).

## File Structure

| Path | Responsibility | Task |
|---|---|---|
| `src/engine.rs` | `revert_all`, `undoable_changes`; shared per-transaction revert | A1 |
| `src/engine_revert_all_tests.rs` (new, `#[cfg(test)] #[path]` from engine.rs like the other test files) | tests for revert_all | A1 |
| `src/user_settings.rs` | `Setting::SuggestedApps`, `Setting::PERSONAL`, `undo_all`, `undoable` | A2 |
| `src/launcher.rs`, `src/broker.rs`, `src/gui/pages/personal.rs` | BlockSuggestedApps through `user_settings::apply`; personal page lists `PERSONAL` only | A2 |
| `src/debloat/suggested.rs` (new) | machine policy block/undo with recorded prior | A3 |
| `src/debloat/mod.rs`, `src/debloat/windows.rs`, `src/debloat/scripts/policy.ps1` (deleted) | `set_consumer_features_policy` → `suggested::block`; `restore_all` | A3 |
| `src/filter/mod.rs`, `dns.rs`, `matcher.rs`, `lists.rs`, `config.rs` (new, portable) | DNS packets, suffix-hash sets, list parsing/classifying, config/status files | B1 |
| `src/uninstall.rs` (new, binary-crate module next to `launcher`, because `i18n`/`Lang` live in the binary crate) + `src/main.rs` | plan counts, revert machine/user, summary lines; hidden CLI | A4 |
| `src/filter/server.rs` (new, portable), `src/filter/fetch.rs` (new), `src/filter/service.rs` (new, windows), `src/filter/adapters.rs` (new) | listeners, forwarding, downloads, the SecblitzFilter service | B2 |
| `src/filter/routing.rs` (new), `src/filter/control.rs` (new), `src/filter/scripts/nrpt.ps1` (new) | NRPT rule, reconcile, switch changes, remove everything | B3 |
| `installer/setup.iss`, `installer/maintenance.ps1`, `installer/test-*.ps1`, `installer/README.md` | choice form, put back, full cleanup, filter registration | A5 |
| `src/gui/pages/settings.rs`, `src/gui/pages/remove.rs` (new), `src/gui/pages/debloat.rs` | Remove Secblitz sheet; "Allow suggested apps again" | A6 |
| `src/gui/pages/web.rs` (new), `src/gui/mod.rs`, `src/gui/pages/home.rs`, `src/explain/` | Web protection page, Home suggestion card, explainers | B4 |

## Phases

- **Phase 1 (parallel, worktrees):** A1, A2, A3, B1. Disjoint files except `src/lib.rs` (B1 adds `pub mod filter;`).
- **Phase 2 (parallel, after the lead merges phase 1):** A4, B2, B3. `src/main.rs` is touched by A4 and B3 (the lead merges); `Cargo.toml`/`Cargo.lock` only by B2.
- **Phase 3 (parallel, after merge):** A5, A6, B4. `src/gui/mod.rs` touched by A6 and B4 (the lead merges).
- **Phase 4:** independent reviewers, lead's own review, i18n merge, full gate, VM verification.

---

### Task A1: `Engine::revert_all` and `undoable_changes`

**Files:**
- Modify: `src/engine.rs` (around `revert`, line ~1783)
- Create: `src/engine_revert_all_tests.rs` (wire with `#[cfg(test)] #[path = "engine_revert_all_tests.rs"] mod revert_all_tests;` next to the existing test-module declarations; reuse the fake backend/helpers that `engine_recovery_tests.rs` uses)

**Interfaces:**
- Produces:
  - `pub fn revert_all(&mut self, callback: impl FnMut(&str, &str)) -> Result<Report>`: every unreverted transaction, newest first. `report.transaction` is `None`; `report.results` holds every per-entry `Outcome` in processing order; `report.findings` = `self.findings()` plus one `journal_finding` per transaction still unreverted at the end.
  - `pub fn undoable_changes(&mut self) -> Result<usize>`: number of distinct control ids with at least one entry not in `State::Restored` inside an unreverted transaction. Read-only (takes the lock, no journal writes).

- [ ] **Step 1: Refactor without behavior change.** Move the body that processes one transaction (from `if !tx.reverting {` to the `Record::Reverted` append) into `fn revert_transaction(&mut self, tx: &mut Transaction, report: &mut Report, callback: &mut impl FnMut(&str, &str)) -> Result<()>`. `revert` calls it for the newest unreverted transaction. Run `cargo test --locked --all-targets`; all existing engine tests must still pass.
- [ ] **Step 2: Write failing tests** in `src/engine_revert_all_tests.rs`:
  - `revert_all_restores_every_batch_newest_first`: apply batch 1 (control X), batch 2 (control Y); `revert_all`; both values back to before; both transactions marked reverted; callback saw Y before X.
  - `revert_all_continues_past_conflict`: three batches; after applying, change batch 2's control to a third value through the fake backend; `revert_all` restores batches 3 and 1, reports batch 2's entry as `"conflict"`, batch 2 stays unreverted, and `report.findings` contains its journal finding.
  - `revert_all_older_batch_never_overwrites_newer`: batch 1 sets X from A to B, batch 2 sets X from B to C. `revert_all` ends with X == A (batch 2 restores B, then batch 1 restores A).
  - `revert_all_reports_skipped`: an entry whose `restore_eligible` is false is `"skipped"`, others still restored.
  - `revert_all_recovers_interrupted_entry`: use the existing recovery test pattern (a `RestorePending` record without `Restored`), then `revert_all` finishes it.
  - `undoable_changes_counts_distinct_ids`: two batches touching X and one touching Y gives 2; after `revert_all` gives 0.
- [ ] **Step 3: Run** `cargo test --locked --all-targets revert_all` and see them fail (method missing).
- [ ] **Step 4: Implement.** `revert_all` takes the lock and interlocks exactly like `revert`, loads and `durable`s the transactions, then `for tx in transactions.iter_mut().rev().filter(|t| !t.reverted) { self.revert_transaction(tx, &mut report, &mut callback)?; }`. A write error with unknown outcome still returns `Err` immediately, as `revert` does.
- [ ] **Step 5: Run tests and clippy**, both pass.
- [ ] **Step 6: Commit** `Engine: undo every change in one pass (revert_all)`.

### Task A2: Suggested apps as a journaled personal setting

**Files:**
- Modify: `src/user_settings.rs`, `src/launcher.rs` (`block_suggested_apps`, `SUGGESTION_VALUES`, `CONTENT_DELIVERY`), `src/broker.rs` (tests only, plus the launcher's handling of `BlockSuggestedApps`), `src/gui/pages/personal.rs`

**Interfaces:**
- Produces:
  - `Setting::SuggestedApps` appended last to `Setting::ALL` (wire byte 5, id `"debloat.suggested_apps"`). Targets: the seven DWORDs under `Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager` (`SilentInstalledAppsEnabled`, `PreInstalledAppsEnabled`, `OemPreInstalledAppsEnabled`, `SubscribedContent-338388Enabled`, `SubscribedContent-338389Enabled`, `SubscribedContent-353694Enabled`, `SubscribedContent-353696Enabled`), safe value 0, absent is not safe.
  - `pub const PERSONAL: [Setting; 5]`: the five settings the personal page lists (everything except `SuggestedApps`).
  - `pub fn undoable(journal: &Path) -> Vec<Setting>`: settings with a journal entry, in `ALL` order.
  - `pub fn undo_all(reg: &mut dyn Registry, journal: &Path) -> Vec<(Setting, Outcome)>`: `undo` for each `undoable` setting, newest-first order is not needed (independent values).
  - The broker request `BlockSuggestedApps` (kind 6) keeps its wire code; the launcher now answers it with `user_settings::apply(&mut SystemRegistry, journal, Setting::SuggestedApps)` mapped through `Reply::from_result(HandleResult::Outcome(..))`. The GUI can undo it with the existing `Request::UserSetting(Setting::SuggestedApps, Op::Undo)`. No new broker kind is needed (deliberate simplification of the spec's `UserSettingsUndoAll`: the GUI sends `Undo` per setting whose `Query` answers `SafeByUs`).
- Consumes: nothing from other tasks.

- [ ] **Step 1: Failing tests** in the existing `user_settings` test module (it has a fake `Registry`):
  - `suggested_apps_apply_records_absent_and_values`: three values absent, four set to 1; `apply` writes all seven to 0; the journal holds seven priors (absent ones as `None`).
  - `suggested_apps_undo_restores_absent`: after the above, `undo` deletes the three and sets the four back to 1.
  - `suggested_apps_already_blocked_is_done_without_journal`: all seven already 0 → `Outcome::Done`, `undoable` is empty (nothing to undo, nothing guessed).
  - `undo_all_undoes_every_journaled_setting`: apply ShowExtensions and SuggestedApps; `undo_all` returns two `Done`; `undoable` is empty.
  - `personal_excludes_suggested_apps`: `PERSONAL` has 5 entries, no `SuggestedApps`, and `ALL[..5] == PERSONAL` (wire bytes unchanged).
  - In `src/broker.rs` tests: `Request::UserSetting(Setting::SuggestedApps, Op::Undo)` round-trips; `decode_with([13, 6, 0], ..)` is `None`.
- [ ] **Step 2: Run, see them fail.**
- [ ] **Step 3: Implement** the setting (status Safe iff all seven read 0; `Other` → Unknown), `PERSONAL`, `undoable`, `undo_all`. Replace the launcher's raw `RegSetValueExW` loop with the journaled `apply` (keep the managed-PC and journal-path handling the launcher already uses for `UserSetting`). Change `personal.rs` to iterate `PERSONAL` (cells sized `Setting::PERSONAL.len()`); its tests switch to `PERSONAL`.
- [ ] **Step 4: Tests and clippy pass** (Windows target too: the launcher is `#[cfg(windows)]`).
- [ ] **Step 5: Commit** `Suggested apps: record what was there before blocking, so it can be undone`.

### Task A3: Machine policy undo and restore all removed apps

**Files:**
- Create: `src/debloat/suggested.rs`
- Modify: `src/debloat/mod.rs` (`set_consumer_features_policy` → wrapper; add `restore_all`), `src/debloat/windows.rs` (drop `POLICY`), `src/debloat/tests.rs`
- Delete: `src/debloat/scripts/policy.ps1`

**Interfaces:**
- Produces (`secblitz::debloat::suggested`):
  - `pub trait PolicyStore { fn get(&self) -> Result<crate::user_settings::Value>; fn set(&mut self, value: u32) -> Result<()>; fn delete(&mut self) -> Result<()>; }`. `crate::user_settings::Value` is `Absent | Dword(u32) | Other`.
  - `#[cfg(windows)] pub struct MachinePolicy;` implementing it on `HKLM\SOFTWARE\Policies\Microsoft\Windows\CloudContent`, value `DisableWindowsConsumerFeatures` (create the key on set; deleting a missing value is Ok).
  - `pub fn journal_path() -> Result<PathBuf>`: `crate::platform::app_dir()?.join("suggested-policy.json")`.
  - `pub fn block(store: &mut dyn PolicyStore, journal: &Path) -> Result<()>`: if no journal file yet, read the prior (`Absent` → `{"prior":null}`, `Dword(v)` → `{"prior":v}`, `Other` → bail without writing); write the journal atomically (temp + rename) before writing 1; read back 1 or restore the prior and bail. If a journal already exists, keep the first prior and just ensure 1.
  - `pub enum Undo { Restored, NothingRecorded, ChangedSince }` and `pub fn undo(store: &mut dyn PolicyStore, journal: &Path) -> Result<Undo>`: no journal → `NothingRecorded`; current value is not `Dword(1)` → remove the journal, `ChangedSince`; else write the prior (delete for `null`), read back, remove the journal, `Restored`.
  - `pub fn recorded(journal: &Path) -> bool` and `pub fn legacy_block(store: &dyn PolicyStore, journal: &Path) -> bool` (= value is `Dword(1)` and nothing recorded: a 0.7.0 block or someone else's policy; left alone and listed).
  - `debloat::set_consumer_features_policy()` keeps its signature and calls `suggested::block(&mut MachinePolicy, &journal_path()?)` (Windows) / bails elsewhere.
  - `pub struct RestoreAll { pub restored: Vec<u16>, pub needs_store: Vec<u16>, pub failed: Vec<u16> }` and `pub fn restore_all(emit: &dyn Fn(u16, bool)) -> RestoreAll` in `debloat/mod.rs`: for each index from `journal::still_removed(&journal::load(), catalog().len())`, newest first: if `offline::has_copy(i)` call `offline::restore_index(i)`; `Back | BackWithoutSomeData | AlreadyThere` → do the same bookkeeping the GUI's `restored_ok` does today (read `src/gui/pages/debloat.rs` around line 577 and move that bookkeeping into a shared `pub fn finish_restore(index: u16)` in `debloat/mod.rs`, then make the GUI call it too); `Damaged | NoCopy` or `Err` → `needs_store` if the catalog entry has a `store_id`, else `failed`. Indices without a copy go to `needs_store`/`failed` the same way. `emit(index, ok)` after each.
- [ ] **Step 1: Failing tests** (`src/debloat/suggested.rs` `#[cfg(test)]` with an in-memory `PolicyStore`, journal in a `tempfile::tempdir()`):
  - `block_records_absent_then_undo_deletes`
  - `block_records_previous_zero_then_undo_writes_zero`
  - `block_twice_keeps_first_prior`
  - `block_refuses_non_dword`
  - `undo_without_record_is_nothing_recorded` and `legacy_block_detected_without_record`
  - `undo_after_someone_changed_it_leaves_it` (value 0 at undo time → `ChangedSince`, value stays 0)
  - In `src/debloat/tests.rs`: delete the test that `include_str!`s `policy.ps1` if any; add `restore_all_sorts_outcomes` using the journal path helpers with a fake (if `restore_all` is hard to fake, split the classification into `fn classify(outcome: Result<Restored>, has_store_id: bool) -> Bucket` and test that).
- [ ] **Step 2: Run, see failures. Step 3: implement** (`windows-sys` registry calls as in `user_settings::sys`). **Step 4: tests, clippy, Windows build pass. Step 5: Commit** `Clean up apps: undoable suggested-apps policy and restore-all`.

### Task B1: Filter core (portable)

**Files:**
- Create: `src/filter/mod.rs`, `src/filter/dns.rs`, `src/filter/matcher.rs`, `src/filter/lists.rs`, `src/filter/config.rs`
- Modify: `src/lib.rs` (`pub mod filter;`)

**Interfaces (Produces, all `secblitz::filter::…`):**
- `dns`:
  - `pub struct Question { pub name: String, pub qtype: u16, pub qclass: u16 }` (name lowercase ASCII, no trailing dot, labels joined by `.`; root is `""`).
  - `pub struct Query { pub id: u16, pub flags: u16, pub question: Question, pub question_end: usize }`
  - `pub fn parse_query(packet: &[u8]) -> Option<Query>`: `None` unless 12 ≤ len ≤ 4096, QR = 0, opcode = 0, qdcount = 1, ancount = 0, nscount = 0; name labels 1..=63 bytes, total ≤ 255, no compression pointers in the question, labels must be ASCII letters/digits/`-`/`_`.
  - `pub fn blocked_reply(query: &[u8], q: &Query) -> Vec<u8>`: copy header id, set QR, RD copied, RA set, rcode 0, qdcount 1, echo the question bytes `[12..question_end]`; `A` (1) adds one answer `0.0.0.0`, `AAAA` (28) adds `::`, other types no answer. Answers use the name pointer `0xC00C`, class IN, TTL 60.
  - `pub fn nxdomain_reply(query: &[u8], q: &Query) -> Vec<u8>` and `pub fn servfail_reply(query: &[u8], q: &Query) -> Vec<u8>` (no answers).
  - `pub fn with_id(packet: &[u8], id: u16) -> Vec<u8>`.
  - `pub fn reply_matches(reply: &[u8], id: u16, q: &Question) -> bool`: QR = 1, same id, qdcount = 1 and the parsed question (case-insensitive) equals `q`.
  - `pub fn truncated(reply: &[u8]) -> bool` (TC bit).
  - `pub const CANARY: &str = "use-application-dns.net";`
- `matcher`:
  - `pub fn hash(name: &str) -> u64` (FNV-1a 64 over the lowercase bytes).
  - `pub struct HashSet64(Vec<u64>)` with `from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self` (sort + dedup), `len()`, `contains(u64) -> bool` (binary search), `any_suffix(name: &str) -> bool` (checks every suffix with at least two labels: `a.b.c.com`, `b.c.com`, `c.com`).
  - `pub struct Category { pub block: HashSet64, pub allow: HashSet64 }` with `blocks(name) -> bool` = `block.any_suffix(name) && !allow.any_suffix(name)`.
  - `#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Kind { Ads, Tracking, Dangerous }`
  - `#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)] pub struct Switches { pub ads: bool, pub tracking: bool, pub dangerous: bool }`
  - `pub struct Filter { pub ads: Category, pub tracking: Category, pub dangerous: Category, pub never: HashSet64 }` with `pub fn decide(&self, name: &str, on: Switches) -> Option<Kind>`: `None` if `never.any_suffix(name)`; otherwise the first enabled match in order Dangerous, Tracking, Ads. `Filter::empty()`.
- `lists`:
  - `pub struct Source { pub id: &'static str, pub url: &'static str, pub max_bytes: u64, pub refresh_days: u64, pub role: Role }`, `pub enum Role { Dns, WindowsTracking, Threats, TrackingClassifier, AdClassifier }`, `pub const SOURCES: [Source; 8]` exactly the eight in Global Constraints (`max_bytes` 128 MiB for `hagezi-tif`, 32 MiB for `adguard-tracking` and `adguard-base`, 16 MiB otherwise; `refresh_days` 1 for Dns/WindowsTracking/Threats, 7 for classifiers).
  - `pub struct Parsed { pub block: Vec<String>, pub allow: Vec<String> }` and `pub fn parse_blocklist(text: &str) -> Parsed`: accepts only lines `||host^`, `||host^$important`, `@@||host^`, `@@||host^$important` where `host` is a valid hostname (`pub fn valid_hostname(&str) -> bool`: 2+ labels, each 1..=63 of `[a-z0-9-_]`, not starting/ending with `-`, total ≤ 253, after lowercasing). Everything else (comments `!`, `#`, cosmetic `##`, regex `/…/`, paths, wildcards `*`, other modifiers) is ignored.
  - `pub fn parse_classifier(text: &str) -> Vec<String>`: hosts from lines starting `||host^` regardless of modifiers (also `||host/` or `||host$`), never `@@` lines.
  - `pub const NEVER_BLOCK: &[&str]`: `windowsupdate.com`, `update.microsoft.com`, `windowsupdate.microsoft.com`, `delivery.mp.microsoft.com`, `do.dsp.mp.microsoft.com`, `emdl.ws.microsoft.com`, `sls.update.microsoft.com`, `activation.sls.microsoft.com`, `validation.sls.microsoft.com`, `licensing.mp.microsoft.com`, `msftconnecttest.com`, `msftncsi.com`, `wdcp.microsoft.com`, `wdcpalt.microsoft.com`, `definitionupdates.microsoft.com`, `go.microsoft.com`, `cp.wd.microsoft.com`, `smartscreen-prod.microsoft.com`, `smartscreen.microsoft.com`, `checkappexec.microsoft.com`, `urs.microsoft.com`, `displaycatalog.mp.microsoft.com`, `storeedgefd.dsx.mp.microsoft.com`, `purchase.mp.microsoft.com`, `cdn.winget.microsoft.com`, `winget.azureedge.net`, `login.live.com`, `login.microsoftonline.com`, `crl.microsoft.com`, `ocsp.msocsp.com`, `oneocsp.microsoft.com`, `time.windows.com`, `secblitz.lol`, `beacons.lol`.
  - `pub struct Inputs<'a> { pub dns: Option<&'a str>, pub windows: Option<&'a str>, pub threats: Option<&'a str>, pub tracking_classifiers: Vec<&'a str>, pub ad_classifiers: Vec<&'a str> }` and `pub fn build(inputs: &Inputs) -> Filter`:
    - `T` = hash set of `parse_classifier` over all tracking classifiers; `A` = same over ad classifiers.
    - `dns = parse_blocklist(inputs.dns)`. For each `d` in `dns.block`: `is_t = any suffix of d in T`, `is_a = any suffix of d in A`. Tracking gets `d` if `is_t`; Ads gets `d` if `is_a || !is_t`. If no tracking classifier text is available, both get every `d`.
    - Tracking additionally gets `parse_blocklist(windows).block`; each category's `allow` = the `allow` of the lists it was built from.
    - Dangerous = `parse_blocklist(threats)`.
    - `never` = `NEVER_BLOCK`.
  - `pub fn counts(filter: &Filter) -> [usize; 3]` (ads, tracking, dangerous block lengths) for status.
- `config`:
  - `#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)] pub struct Config { pub ads: bool, pub tracking: bool, pub dangerous: bool, #[serde(default)] pub paused_until: Option<u64> }` (unix seconds) with `fn any_on(&self) -> bool`, `fn active(&self, now: u64) -> Switches` (all false while `paused_until > now`), `fn paused(&self, now: u64) -> bool`.
  - `pub fn load_config(path: &Path) -> Config` (missing, over 4 KiB or invalid → `Config::default()`), `pub fn save_config(path: &Path, c: &Config) -> Result<()>` (temp + rename).
  - `#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)] pub struct Status { pub listening: bool, pub state: State, pub lists_updated: Option<u64>, pub day: u64, pub blocked: [u64; 3], pub domains: [u64; 3], pub last_error: Option<ErrorCode>, pub written_at: u64 }`; `pub enum State { #[default] Starting, Ready, NoLists }`; `pub enum ErrorCode { PortInUse, DownloadFailed, ListInvalid, NoUpstream }` (serde `rename_all = "kebab-case"` on both enums).
  - `pub fn load_status(path: &Path) -> Option<Status>` (cap 16 KiB), `pub fn save_status(path: &Path, s: &Status) -> Result<()>`.
  - `pub fn fresh(status: &Status, now: u64) -> bool` (`written_at` within 120 s).
  - Paths: `pub fn dir() -> Result<PathBuf>` = `%ProgramData%\Secblitz\Filter` (use the same ProgramData lookup `platform` uses); `config_path()` = `dir/config.json`; `data_dir()` = `dir/Data`; `status_path()` = `data_dir/status.json`; `lists_dir()` = `data_dir/lists`.
- [ ] **Step 1: Failing tests** (each module's `#[cfg(test)] mod tests`):
  - dns: `parse_accepts_a_query` (hand-built bytes for `www.Example.com A`, expect lowercase name, qtype 1); `parse_rejects_response`, `parse_rejects_two_questions`, `parse_rejects_compression_pointer`, `parse_rejects_truncated_name`, `parse_rejects_long_label`, `parse_rejects_oversize_packet`, `parse_rejects_empty`; `blocked_a_answers_zero_ip` (check answer RDATA `0,0,0,0`, TTL 60, ancount 1); `blocked_aaaa_answers_unspecified`; `blocked_https_is_empty_noerror`; `nxdomain_sets_rcode_3`; `reply_matches_checks_id_and_question`; fuzz-style `parse_never_panics` over 10,000 `rand` byte strings of length 0..600 (seeded `StdRng`).
  - matcher: `suffix_match_blocks_subdomains`, `two_label_minimum` (blocking `example.com` does not block `com`), `exception_beats_block`, `never_block_wins`, `decide_order_dangerous_first`, `switch_off_means_none`.
  - lists: `parse_accepts_plain_and_important`, `parse_reads_exceptions`, `parse_ignores_cosmetic_and_regex_rules` (`example.com##.ad`, `/ads?/`, `||ex*.com^`, `||example.com^$third-party`, `||example.com/path`), `hostname_validation`; `build_classifies_overlap_into_both` (dns list `doubleclick.net`, `adservice.example`, `hotjar.com`; tracking classifier has `doubleclick.net`, `hotjar.com`; ad classifier has `doubleclick.net`: ads = {doubleclick, adservice}, tracking = {doubleclick, hotjar}); `build_without_classifiers_puts_everything_in_both`; `windows_list_goes_to_tracking`; `sources_are_https_and_unique`.
  - config: `pause_turns_everything_off_until_time`, `oversize_config_is_default`, `status_round_trips`, `freshness_window`.
- [ ] **Step 2: Run, see failures. Step 3: Implement. Step 4: tests + clippy pass. Step 5: Commit** `Web protection: DNS packets, block lists and matching`.

### Task A4: Uninstall logic and hidden CLI

**Files:**
- Create: `src/uninstall.rs` as a binary-crate module (`mod uninstall;` in `src/main.rs`; it uses library items through `secblitz::…` and `crate::i18n::Lang`)
- Modify: `src/main.rs` (hidden subcommands, `execute`, the `human_cli_commands_are_gone` test and allowlist tests)

**Interfaces:**
- Consumes: `Engine::revert_all`, `Engine::undoable_changes` (A1); `user_settings::{undo_all, undoable, journal_path, SystemRegistry}` (A2); `debloat::{restore_all, RestoreAll}`, `debloat::suggested::{undo, Undo, legacy_block, recorded, journal_path, MachinePolicy}` (A3).
- Produces (`crate::uninstall`, binary crate):
  - `#[derive(Serialize, Default, Clone, Debug, PartialEq)] pub struct Plan { pub settings: usize, pub apps_with_copy: usize, pub apps_store_only: usize, pub suggested: bool }` and `pub fn plan() -> Result<Plan>` (Windows; elevated: opens the engine like the GUI does, counts `still_removed` split by `offline::has_copy`, `suggested::recorded`). Personal counts are not here: the GUI gets them through broker queries, the CLI `--user` part does its own.
  - `#[derive(Serialize, Clone, Debug, PartialEq)] pub enum Left { Setting { title: String, reason: LeftReason }, App { name: String }, AppNeedsStore { name: String }, Personal { id: &'static str }, SuggestedOlderVersion, SuggestedChangedSince }` and `pub enum LeftReason { ChangedSince, NotPossible }` (`conflict` → `ChangedSince`, `skipped`/other → `NotPossible`).
  - `#[derive(Serialize, Default, Debug)] pub struct Summary { pub restored: usize, pub left: Vec<Left> }`
  - `pub enum Step { Settings, Apps, Suggested }` and `pub fn revert_machine(progress: &dyn Fn(Step, bool)) -> Summary` (Windows): engine `revert_all`, then `debloat::restore_all`, then `suggested::undo` (`NothingRecorded` with `legacy_block` → `SuggestedOlderVersion`). Each step's errors become `Left` lines, never a panic or early return; `progress(step, done_ok)` after each.
  - `pub fn revert_user() -> Summary` (Windows): `user_settings::undo_all(&mut SystemRegistry, &journal_path()?)`; each non-`Done` outcome → `Left::Personal { id: setting.id() }`.
  - `pub fn left_line(left: &Left, lang: Lang) -> String`: plain translated line, e.g. `"{title}: you changed this yourself since, so it was left as it is"`, `"{name} could not be brought back. You can get it again from the Microsoft Store."`, `"Suggested apps were blocked by an older version of Secblitz, so they were left as they are"`. The titles come from the outcome's `title` (already the plain check name).
  - `pub fn cleanup_user() -> Result<()>` (Windows): delete `%LOCALAPPDATA%\Secblitz` after checking it is a plain directory, not a reparse point, owned by the current user.
- CLI (hidden, `Command::new(..).hide(true)`, never in `elevated_args`, never elevate):
  - `secblitz uninstall-revert` → must already be elevated (else exit 2, no UAC). Runs `revert_machine`, prints one `left_line` per leftover to stdout (UTF-8, `\n`), exit 0. With `--json` prints `Summary` as JSON.
  - `secblitz uninstall-revert --user` → must NOT be elevated-as-another-user; runs `revert_user`, prints lines, exit code = number of leftovers (0..=6).
  - `secblitz uninstall-cleanup --user` → `cleanup_user`, exit 0/1.
  - Lines are only for the uninstaller; no console window is created when launched with `CREATE_NO_WINDOW`/Inno `SW_HIDE`.
- [ ] **Step 1: Failing tests:** `left_reason_from_status` (`"conflict"` → ChangedSince, `"skipped"` → NotPossible), `left_lines_are_plain` (no `HKCU`, `DNS`, em dash; each variant renders in all six languages via `Lang` without falling back to a raw key — use the pending TSV rows), CLI tests in `main.rs`: `uninstall_commands_are_hidden` (not in `--help`), `uninstall_commands_never_elevate` (`elevated_args` panics/unreachable is not reached because `execute` handles them before; assert the parse succeeds and the command name is matched before elevation), update `human_cli_commands_are_gone` to allow the new hidden names.
- [ ] **Step 2–4:** implement, run tests/clippy/Windows build.
- [ ] **Step 5: Commit** `Remove Secblitz: put-back logic and uninstaller commands`.

### Task B2: SecblitzFilter service runtime

**Files:**
- Create: `src/filter/server.rs` (portable), `src/filter/fetch.rs` (portable), `src/filter/adapters.rs` (Windows impl + non-Windows stub), `src/filter/service.rs` (Windows)
- Modify: `src/filter/mod.rs`, `Cargo.toml` + `Cargo.lock` (reqwest feature `gzip`; windows-sys features `Win32_NetworkManagement_IpHelper`, `Win32_NetworkManagement_Ndis`, `Win32_Networking_WinSock`), `src/service.rs` only if a shared helper must become `pub(crate)` (prefer copying small helpers over editing the monitor)

**Interfaces:**
- Consumes: everything from B1.
- Produces:
  - `server`: `pub struct Shared { pub filter: RwLock<Arc<Filter>>, pub config: RwLock<Config>, pub upstream: RwLock<Vec<SocketAddr>>, pub stats: Stats }`; `pub struct Stats` with atomic counters `blocked: [AtomicU64; 3]` and `day: AtomicU64` (reset at a new day); `pub enum Action { Reply(Vec<u8>), Forward }`; `pub fn decide(packet: &[u8], shared: &Shared, now: u64) -> Option<(Query, Action)>` (`None` = drop; canary → NXDOMAIN reply; blocked → `blocked_reply` and count; else `Forward`); `pub fn forward(packet: &[u8], q: &Query, upstream: &[SocketAddr], timeout: Duration) -> Vec<u8>` (fresh `UdpSocket` bound to the unspecified address of the right family on port 0, random id from `rand::rngs::OsRng`, try each server once with `timeout` 2 s, accept only `reply_matches`, on `truncated` retry that server over TCP, restore the original id with `with_id`; all failed → `servfail_reply`); `pub fn serve_udp(socket: UdpSocket, shared: Arc<Shared>, stop: Arc<AtomicBool>)` (read timeout 500 ms to notice `stop`; ignore datagrams whose source is not loopback; hand each to a bounded worker pool of 16 threads via `mpsc::sync_channel(256)`, drop when full); `pub fn serve_tcp(listener: TcpListener, shared, stop)` (non-loopback peers closed at once; at most 32 open connections; 2-byte length frames up to 65535; 10 s idle timeout).
  - `pub fn bind(addrs: &[SocketAddr]) -> Result<(Vec<UdpSocket>, Vec<TcpListener>), BindError>` with `BindError::PortInUse`. Production addresses are `127.0.0.1:53` and `[::1]:53` (IPv6 bind failure when IPv6 is disabled is not fatal if IPv4 bound).
  - `fetch`: `pub fn due(source: &Source, last: Option<u64>, now: u64) -> bool`; `pub fn download(client: &reqwest::blocking::Client, source: &Source) -> Result<String>` (client built with `https_only(true)`, `redirect(Policy::none())`, `gzip(true)`, timeout 120 s, `no_proxy()` like the updater; non-200 → error; read through `take(max_bytes + 1)` and error if over; must be UTF-8); `pub fn store(lists_dir: &Path, id: &str, text: &str) -> Result<()>` (temp + rename) and `pub fn load_all(lists_dir: &Path) -> BTreeMap<&'static str, String>` (only ids in `SOURCES`); `pub fn rebuild(lists: &BTreeMap<&str, String>) -> Option<Filter>` (`None` when no blocking list is present, so state is `NoLists`); a rebuilt filter with zero domains for an enabled list that previously had some keeps the previous set (`ListInvalid`).
  - `adapters`: `pub fn upstream_servers() -> Vec<IpAddr>` (Windows: `GetAdaptersAddresses` with `GAA_FLAG_SKIP_ANYCAST | SKIP_MULTICAST`, `IfOperStatus == Up`, not loopback/tunnel-to-us; collect DNS server addresses; drop loopback, unspecified, IPv6 link-local and the `fec0:0:0:ffff::1..3` defaults; dedupe, keep order, at most 4) and `pub fn metered() -> bool` (`GetNetworkConnectivityHint`; `ConnectivityCost` Fixed/Variable or `OverDataLimit`/`Roaming` → true; errors → false). Non-Windows: empty / false.
  - `service` (Windows): `pub const NAME: &str = "SecblitzFilter";` `pub fn run() -> Result<()>` (service dispatcher); `pub fn install() -> Result<()>` (registers own-process, `LocalService`, start type Disabled, required privileges `SeChangeNotifyPrivilege` only, service SID unrestricted not needed, failure actions restart after 5 s, 5 s, 30 s, reset after 1 day, description "Blocks ads, trackers and dangerous websites for Secblitz"; same SD and binary checks as `service/windows.rs` `install`); `pub fn set_enabled(on: bool) -> Result<()>` (start type Auto + start and wait up to 10 s for Running, or stop and set Disabled); `pub fn state() -> Result<ServiceState>` (`NotInstalled | Stopped | Running | Other`); `pub fn delete() -> Result<()>` (stop, wait, delete; absent is Ok); `pub fn ensure_dirs() -> Result<()>` (elevated: create `Filter` with SD `O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;LS)(A;OICI;0x1200a9;;;BU)` and `Filter\Data` with `O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;LS)(A;OICI;0x1200a9;;;BU)`; validate owner and no reparse points like `inspect` does).
  - Service main loop: bind (on `PortInUse` write status `last_error = port-in-use`, `listening=false`, keep polling every 30 s); load lists and build on a background thread; every 2 s reload config; every 30 s refresh upstream (also right after a forward fails everywhere); every hour check `due` sources unless `metered()`, download, store, rebuild, swap; write status every 10 s and on change; respond to Stop within 3 s.
- [ ] **Step 1: Failing tests** (portable, Linux-runnable; bind to `127.0.0.1:0`):
  - `udp_blocks_and_forwards`: a fake upstream thread on `127.0.0.1:0` answering every query with a fixed A record; `Shared` with filter blocking `ads.example`; send `ads.example A` → `0.0.0.0`; send `ok.example A` → the fake's answer with the original id.
  - `forward_ignores_spoofed_reply`: fake upstream first sends a reply with a wrong id, then the right one → right one used.
  - `forward_retries_over_tcp_on_truncation`.
  - `forward_all_dead_returns_servfail` (upstream = closed port, timeout 200 ms in test).
  - `canary_gets_nxdomain`, `paused_forwards_everything`, `never_block_forwards`.
  - `udp_ignores_non_loopback`: unit-test the source check function `fn allowed_peer(addr: &SocketAddr) -> bool`.
  - `tcp_rejects_oversize_frame_and_closes`.
  - `bind_failure_reports_port_in_use`: bind `127.0.0.1:0`, then call `bind` with that exact address → `PortInUse`.
  - fetch: `download_rejects_oversize` and `download_rejects_redirect` against a tiny `TcpListener` HTTP server is not possible with `https_only`; instead test `read_capped(reader, max)` and `check_status(code)` helpers that `download` uses. `due_daily_and_weekly`. `bad_list_keeps_previous_set` (rebuild with an empty `adguard-dns` text when previous counts were non-zero keeps previous).
  - stats: `blocked_counts_reset_on_new_day`.
- [ ] **Step 2–4:** implement; tests, clippy, Windows build.
- [ ] **Step 5: Commit** `Web protection: the filter service`.

### Task B3: Routing, reconcile and switch control

**Files:**
- Create: `src/filter/control.rs` (portable decisions + Windows glue), `src/filter/routing.rs` (Windows), `src/filter/scripts/nrpt.ps1`
- Modify: `src/filter/mod.rs`, `src/debloat/windows.rs` (generalize the runner: `pub fn run_with_modules(script, modules: &[&str], env, timeout)`; `run` keeps its current module list), `src/main.rs` (hidden `filter` subcommands)

**Interfaces:**
- Consumes: B1 config/status, B2 `service::{set_enabled, state, delete, install, ensure_dirs, run}`, `adapters::upstream_servers`.
- Produces:
  - `pub fn rule_servers(network: &[IpAddr]) -> Vec<IpAddr>`: `127.0.0.1`, `::1`, then `network` (deduped, loopback removed, at most 4), then `9.9.9.9`, `149.112.112.112`.
  - `pub enum Desired { Rule(Vec<IpAddr>), NoRule }` and `pub fn desired(config: &Config, service: ServiceState, status: Option<&Status>, network: &[IpAddr], now: u64) -> Desired`: `Rule` only if `config.any_on()` and service `Running` and status is `fresh` and `listening`. Pause does not remove the rule (the service forwards everything while paused).
  - `routing` (Windows): `pub fn current_rule() -> Result<Option<Vec<IpAddr>>>`, `pub fn set_rule(servers: &[IpAddr]) -> Result<()>`, `pub fn remove_rule() -> Result<()>`, via `nrpt.ps1` run with modules `DnsClient` (+ Management/Utility). The script takes `$env:SECBLITZ_NRPT_MODE` in `Show|Set|Remove` and `$env:SECBLITZ_NRPT_SERVERS` (comma-separated; every item must parse with `[System.Net.IPAddress]::TryParse`, else throw). It only ever touches rules whose `DisplayName -eq 'Secblitz web protection'` and `Comment -eq 'Managed by Secblitz'`; `Set` removes ours then `Add-DnsClientNrptRule -Namespace '.' -NameServers $servers -DisplayName … -Comment …`; `Set` and `Remove` end with `Clear-DnsClientCache`. Output is one JSON line `{"servers":[...]}` or `{"servers":null}`.
  - `pub fn reconcile() -> Result<()>` (Windows, SYSTEM or elevated): compute `desired`; apply only if it differs from `current_rule`.
  - `pub fn apply_switches(new: Config) -> Result<()>` (elevated app): `ensure_dirs`; `save_config`; if `new.any_on()` → `service::set_enabled(true)`, wait up to 10 s for a fresh listening status, then `reconcile`; else `remove_rule` first, then `service::set_enabled(false)`.
  - `pub fn pause_for(duration: Duration) -> Result<()>` / `pub fn resume() -> Result<()>`: rewrite `paused_until` only.
  - `pub fn remove_everything() -> Result<()>`: `remove_rule`, `service::delete`, delete the `SecblitzFilterReconcile` task only if its action is our exe (or leave the task to maintenance.ps1; say which in a comment and keep it consistent with A5: **maintenance.ps1 owns task deletion**), delete `%ProgramData%\Secblitz\Filter` after the owner/reparse checks.
  - CLI (hidden, never elevate, require elevation or SYSTEM else exit 2): `secblitz filter run` (service dispatcher; no elevation check, SCM starts it), `secblitz filter reconcile`, `secblitz filter install` (B2 `install` + `ensure_dirs`), `secblitz filter uninstall` (`remove_everything`).
- [ ] **Step 1: Failing tests:** `rule_servers_order_and_dedupe`, `rule_servers_caps_network_at_four`, `desired_rule_requires_fresh_listening_status`, `desired_no_rule_when_all_off`, `desired_keeps_rule_while_paused`, `desired_no_rule_when_service_stopped`; a test that `nrpt.ps1` contains no `Set-DnsClient`, `Set-DnsClientServerAddress` or `DohServerAddress` (adapters are never touched) and filters on both DisplayName and Comment; CLI tests: the four `filter` subcommands are hidden and handled before elevation.
- [ ] **Step 2–4:** implement; tests, clippy, Windows build.
- [ ] **Step 5: Commit** `Web protection: routing rule, reconcile and switches`.

### Task A5: Installer and uninstaller

**Files:**
- Modify: `installer/setup.iss`, `installer/maintenance.ps1`, `installer/test-maintenance.ps1`, `installer/test-lifecycle.ps1`, `installer/README.md`, `installer/check-locales.py` (if it validates CustomMessages)

**Interfaces:**
- Consumes: CLI from A4 (`uninstall-revert`, `uninstall-revert --user`, `uninstall-cleanup --user`) and B3 (`filter install`, `filter uninstall`, `filter reconcile`).
- Produces:
  - maintenance.ps1 actions: `InstallFilter` (runs `secblitz.exe filter install` hidden, then registers task `SecblitzFilterReconcile` as SYSTEM: action `"{app}\secblitz.exe" filter reconcile`, triggers: at startup, hourly repetition, and an event trigger on `Microsoft-Windows-NetworkProfile/Operational` EventID 10000 via the CIM `MSFT_TaskEventTrigger` class; same owned-task checks as `SecblitzUpdate`), `ResumeFilter` (start the service again if Prepare found it running), `RemoveFilter` (runs `secblitz.exe filter uninstall`, deletes the reconcile task if owned), `Purge` (delete HKLM Run value `SecblitzTray` only if it points at `{app}\secblitz.exe`; `{app}\Monitor`; `%ProgramData%\Secblitz`; `HKLM\Software\Secblitz`; each path owner-checked and reparse-checked like the existing removal code). `Prepare` additionally records whether SecblitzFilter was running and stops it (exit code semantics stay compatible: reuse the existing resume flag mechanism or add a second one).
  - setup.iss:
    - `[Code]` in `CurStepChanged(ssPostInstall)`: `Maintain('InstallFilter')` always (service stays disabled unless the config has a switch on: on upgrade, `ResumeFilter` starts it again).
    - Uninstall flow in `InitializeUninstall` when `IsAdmin`: `CloseTray`; decide the choice: `/SECBLITZDONE` param → Keep (the app already put things back); `UninstallSilent` → Keep; else show a custom form (`CreateCustomForm`) with title "Remove Secblitz", the question, two radio buttons with the same texts as the app sheet, the footnote, and Cancel / Remove Secblitz buttons. Cancel → `Result := False` (nothing touched).
    - Put back: `ExecAsOriginalUser('{app}\secblitz.exe', 'uninstall-revert --user', …, SW_HIDE, ewWaitUntilTerminated, Code)`, then `ExecAndCaptureOutput` (Inno 6.3+, check the version the build uses in `scripts/build-release.ps1`; if older, write to a file in `{tmp}` with a fixed name instead) of `uninstall-revert` elevated; the uninstaller's status label says "Putting your settings back". Collected lines (and "{n} personal settings could not be put back" from the user exit code) are shown in one message box, then removal continues.
    - Both choices then: `Maintain('RemoveFilter')`, `ExecAsOriginalUser('{app}\secblitz.exe','uninstall-cleanup --user', …)`, `Maintain('RemoveMonitor')` (existing). In `CurUninstallStepChanged(usPostUninstall)`: `Maintain('Purge')`.
    - New `[CustomMessages]` for every language the installer ships (texts: "Remove Secblitz", "What should happen to the changes Secblitz made?", "Keep my PC as it is now", "Your protection stays on. Apps you removed stay removed; you can reinstall them from the Microsoft Store.", "Put everything back the way it was", "Secblitz undoes its changes and brings back the apps you removed first. This can take a few minutes.", "Windows updates, virus scans and apps you installed with Secblitz stay.", "Putting your settings back", "Some things could not be put back:"). Use the app's translations from the pending TSVs so wording matches.
- [ ] **Step 1: Failing tests:** extend `test-maintenance.ps1` for `Purge` path checks (reparse point and foreign owner are refused; owned paths removed) and the new ValidateSet; extend `test-lifecycle.ps1` with: silent uninstall without `/SECBLITZDONE` keeps a registry value Secblitz changed and removes ProgramData, Run value, service and task; `/SECBLITZDONE` never prompts. These scripts run on Windows only (CI); make sure they parse: `pwsh -NoProfile -Command "[System.Management.Automation.Language.Parser]::ParseFile(...)"` if `pwsh` exists on the host, otherwise note it.
- [ ] **Step 2–4:** implement; run `python3 installer/check-locales.py` and the Rust gate.
- [ ] **Step 5: Commit** `Installer: keep-or-put-back choice, full cleanup, web protection service`.

### Task A6: GUI, Remove Secblitz

**Files:**
- Create: `src/gui/pages/remove.rs` (state, messages, view of the sheet and progress)
- Modify: `src/gui/pages/settings.rs` (new group "Remove Secblitz" before About with one Danger button), `src/gui/mod.rs` (route the sheet through `modal()` for `Page::Settings`), `src/gui/pages/debloat.rs` ("Allow suggested apps again" link when `suggested::recorded` or the personal setting answers `SafeByUs`), `src/app/settings.rs` if install detection helpers are needed

**Interfaces:**
- Consumes: `uninstall::{plan, Plan, revert_machine, Step, Summary, Left, left_line}` (A4), broker `UserSetting(s, Op::Query|Op::Undo)` for `Setting::ALL` (A2), `filter::control::apply_switches(Config::default())` (B3) for the web protection line, `installed_exe()` (src/app/settings.rs:102).
- Produces: sheet states `Choose { plan, personal: usize, choice }`, `Working { steps: [StepState; 5] }` (settings, personal settings, removed apps, suggested apps, web protection), `Result { left: Vec<String> }`, `Leaving`.
  - Counts: `plan()` on a worker thread when the sheet opens; personal count = number of `Setting::ALL` answering `SafeByUs` to `Query`. Put-back text: "Secblitz undoes its {n} changes and brings back {apps} removed apps first. This can take a few minutes." with lines hidden when zero, and the whole choice hidden when nothing can be put back.
  - Put back order: personal settings (broker `Undo` per `SafeByUs` setting), then `revert_machine` on a worker thread, then Store reinstall through broker `ReinstallStoreApp(i)` for each `AppNeedsStore` when online, then web protection off.
  - Keep: skip straight to removing.
  - Removing: `Command::new(app_dir.join("unins000.exe")).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SECBLITZDONE"]).creation_flags(CREATE_NO_WINDOW).spawn()`, then exit the app (`iced::exit()`). Only when `installed_exe()` is `Some` and `unins000.exe` exists next to it, owner-checked.
  - Portable copy (`installed_exe()` is `None`): only "Put everything back", no remove step, final text "You can now delete secblitz.exe."
  - Failure screen: plain lines from `left_line`, buttons "Remove Secblitz anyway" and "Keep Secblitz".
- [ ] **Step 1: Failing tests** (the GUI pages have pure-state tests; follow `personal.rs` tests): `sheet_hides_put_back_when_nothing_to_undo`, `put_back_text_counts`, `portable_sheet_offers_put_back_only`, `failure_lists_left_lines_and_two_buttons`, `keep_skips_put_back`.
- [ ] **Step 2–4:** implement; tests, clippy, Windows build.
- [ ] **Step 5: Commit** `Settings: Remove Secblitz, keep or put back`.

### Task B4: GUI, Web protection page

**Files:**
- Create: `src/gui/pages/web.rs`, `src/explain/web.rs` (or add rows where explainers live; follow `src/explain/` layout)
- Modify: `src/gui/mod.rs` (`Page::Web` between `Debloat` and `Tools` in `Page::ALL`, `parse("web")`, label "Web protection", icon: add a globe-with-shield or reuse an existing fitting icon from `src/gui/icons.rs`), `src/gui/pages/mod.rs`, `src/gui/pages/home.rs` (suggestion card, not scored)

**Interfaces:**
- Consumes: `filter::config::{load_config, load_status, Config, Status, State, ErrorCode, fresh}`, `filter::control::{apply_switches, pause_for, resume}`, `filter::service::state`.
- Produces: page with three switches (texts from the spec), a status line ("On", "Off", "Paused until {time}", "Getting block lists ready", "Not working right now. Your internet still works, but nothing is being blocked."), Pause for 1 hour / Resume now, "Blocked today: {ads} ads, {trackers} trackers, {dangerous} dangerous websites", the YouTube limitation line, and on a portable copy "Web protection needs Secblitz to be installed." with switches disabled. Switch changes run `apply_switches` on a worker thread with the switch showing a busy state; status polled every 2 s only while the page is visible (subscription via `on_page`). Home card "Block ads, trackers and dangerous websites" (shown while all three are off; dismissible like other optional cards if the home page has that pattern) navigates to `Page::Web`; it never changes the score.
- [ ] **Step 1: Failing tests:** `page_order_has_web_after_debloat`, `status_line_for_each_state` (pure fn `status_line(config, status, service, now) -> Line`), `paused_shows_until_time`, `stale_status_shows_not_working`, `portable_disables_switches`, `home_card_not_scored`.
- [ ] **Step 2–4:** implement; tests, clippy, Windows build.
- [ ] **Step 5: Commit** `Web protection page and Home suggestion`.

### Phase 4 (lead)

- [ ] Merge all branches, resolve `src/main.rs`, `src/lib.rs`, `src/gui/mod.rs` overlaps.
- [ ] `python3 scripts/merge-i18n-pending.py`, fix rejected rows.
- [ ] Independent review workflow (correctness, security, plain language) on the full diff; fix confirmed findings.
- [ ] Full gate + release build.
- [ ] VM verification per both specs' Testing sections (lead only).

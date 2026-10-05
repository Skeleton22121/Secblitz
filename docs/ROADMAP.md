# Engineering roadmap after live 0.5.0

**Engineering decision memo, 2026-10-03, not public website copy.** Completed items below are implemented; backlog rows remain proposals. Secblitz complements Defender, servicing and backups, not a replacement antivirus or complete PC fixer. Preserve NAS, printer, VPN, developer, gaming and accessibility workflows through explicit context and bounded changes.

## 0.6.0 development candidate

**Publication update: 0.6.0 is live.** The final installer and executable hashes, signed feed and homepage were verified following deployment. See [release publication](website-deployment.md#published-060-release). Historical 0.5.0 artifacts remain archived. The implementation below advances the backlog; it does not close every row or constitute external certification. Candidate evidence below retains its original attribution; the final release additionally fixes the native-observed SYSTEM account-name health check.

| Area | Implemented in candidate source | Acceptance boundary |
| --- | --- | --- |
| Terminal experience | Five-item main menu, grouped Advanced, in-place status/progress, selected-fix recap, preserved selection, proper nested Back, semantic colors, optional animation, narrow layouts and six languages | Host PTY and real Windows ConPTY tests; final paging/resize/mode-restoration regressions are separately recorded |
| Preference recovery | Flushed copy-on-write snapshots; preserve damaged bytes before recovery; recognize only legal incomplete tails; committed before-images and exact state checks remain required | Every logical snapshot prefix tested; native real-file cuts and all engine tests passed. Ambiguous/interior corruption still blocks recovery |
| Maintenance engine | Fixed DISM check/scan/restore, SFC verify/repair and Defender quick-scan operations; exact digest approval, dependencies, expiring opt-in policy, window/power/network/idle gates, durable intent, supervision and verify-only resume | Native storage/process fixtures passed. Live DISM remediation has not passed; SFC completion is not clean-integrity proof. No unattended scheduler or automatic reboot |
| Broader assessment | 23 probe paths covering Defender/ASR/CFA, providers/management, cached updates, device security, accounts/remote access, software/browser inventory, recovery, storage and networking | 22 machine probes returned evidence in native Windows; original-user browser probe deliberately omitted when identity was not established. Unknown stays unknown |
| Compatibility | Everyday, gaming, development and higher-security advice profiles; declared printer/NAS/VPN/game/development needs; versioned rule references | Advice and compatibility notes, not automatic ASR/CFA profiles or verified application workflow tests |
| Windows quality updates | Fresh exact-ID/revision plans, bound bundle metadata/EULAs/source, selected download/install, cooperative cancellation, boot/process records and independent installed-revision verification | Unit/mock/native trust fixtures passed; eligible split-token session, real catalog/download/install and reboot recovery need end-to-end acceptance |
| Secblitz delivery | Exact-candidate root-authorized delegated key, deterministic signed rollout, persisted enrollment/floors, durable installation intent and post-install health, checked renewal tools | Protocol/tool tests and native interlock fixtures; production delegation/rollout not enabled, renewal job not installed, new live upgrade not tested. Not full TUF/root rotation |

### What remains genuinely unfinished

- Current application upgrades: no validated production backend. The research module is excluded from the application, advertises an empty catalog, and rejects execution. An obsolete VS Code target was rejected rather than offered as a security upgrade.
- Verified backup coverage and sample restores, off-device recovery proof, and repair workflows for storage/network issues.
- Effective AccessCheck/Authz analysis across service/task/executable chains. Current fixed-service ACE evidence is not that analysis.
- Tested mutating ASR/CFA/device-security profiles, browser-risk assessment beyond bounded inventory, account-security enrollment guidance, and workflow-specific post-change compatibility checks.
- A background scheduler for owner-approved remediation and reboot continuation. Current resume verifies interrupted work and never silently replays it.
- Windows 11 Home/Pro, supported Windows 10 variants, broader hardware and complete ordinary-user/over-the-shoulder account coverage.
- An external security assessment. Multiple agent implementation/review passes are peer review, not an outside assessor's report.
- Publisher authentication remains absent by explicit owner choice. No self-signed certificate was installed to imply Windows trust.

### Candidate evidence

- `target/windows-foundation-validation/`: initial native probes; two real Windows serialization/module-resolution bugs fixed.
- `target/windows-engine-native-validation/`: 75 native engine tests passed in 68.88 seconds; the earlier timeout was byte-at-a-time test fault-injection I/O, not an observed lock deadlock.
- `target/windows-v060-candidate-validation/`: frozen 0.6.0 candidate, 247 native library tests, 92 native CLI tests, 11 prior native fixtures, three additional patching fixtures, Windows PowerShell 5.1 boundary suites and real ConPTY captures. One split-token patching fixture lacked its prerequisite. Follow-up UI fixes require their own later evidence; do not apply old binary hashes to newer source.
- `target/windows-v060-ui-final/`: later native UI acceptance, 98 CLI tests passed, three explicit ConPTY regressions, real PageUp/PageDown, immediate-parent navigation, resize-before-approval and exact console-mode restoration. The library acceptance above was retained rather than rerun.
- Final small follow-up: bind differential-output cache to the resize epoch, including an A-to-tiny-to-A resize with no intermediate paint. The specific regression passed, Windows compilation and optimized rebuild passed. This last cache fix was not subjected to another VM/review cycle. Current local executable: `target/windows-release/x86_64-pc-windows-gnu/release/secblitz.exe`, SHA-256 `dbedb6a40cc88d19635d3df53b3bde55c257a28fe725b0c256b6c08d56b3867d`. Earlier frozen binary hashes describe their own tested inputs, not this final file.
- Only the isolated UI clone was used. All 18 controls and original journal bytes/ACLs/attributes were restored and verified; the original user VM was untouched.

Readiness or eligibility failures are recorded as blocked, never relabeled as successful repair. The remaining backlog below should be read alongside this candidate status and the historical published-release evidence.

## Completed first batch and security follow-up

The genuine published [0.4.3 to 0.5.0 live gate passed](windows-v050-results.md): Installed **18.865 s**, UpToDate **1.336 s**, DeferredBusy **1.005 s**, fresh resumed LocalService report **9,195 bytes**, all 18 baselines/WAL preserved. Native functional acceptance: **138 library + 66 CLI**, separate readiness smoke and nine SYSTEM cases. No claim that all proposed PC-maintenance features are finished follows.

| Previously identified gap | Implemented disposition | Boundary |
| --- | --- | --- |
| Persistent NotConfigured incorrectly recommended despite ActiveStore Block | Typed effective/authority evidence recognizes eligible inherited Block, with no rewrite/WAL; genuine Allow remains repairable | Missing/contradictory/nonlocal evidence cannot authorize a repair |
| Informational rows inflate attention counts | More information excluded from issue/protection totals; readiness separate | Unknown/attention remain visible; totals are not vulnerabilities |
| Reviewed one-action repair plan | Fix recommended previews exact current candidates; Apply/Change selection/Back defaults Back | Single Enter approval, fresh gates; no unapproved expansion or extra actions |
| Missing automatic post-verification | Exactly one fresh audit after every apply/Undo attempt, before rendering | Failure retained separately; failed verification withholds actionable snapshot |
| Basic device context | Four read-only typed readiness probes | Only confirmed read-only volumes/zero journal bytes block new repair, never Undo |
| Observed release replay and unlink-first persistence | Protected version/hash/timestamp floor and atomic temp/flush/MoveFileEx replacement shipped in 0.4.3 | Local trust/time/signing boundaries remain; not full TUF or installer rollback |
| Public source/test deployment | Allowlisted dist/pages, private docs/scripts, retired-path blocks and old-deployment removal | Reviewed exposed material had no secrets; not exhaustive leak proof |

The [old 0.4.2 audit](../target/windows-live-v042/post-checks/audit.json) is the historical reproduction, not current behavior. [0.5.0 evidence](windows-v050-results.md) shows zero inherited-firewall candidates, exact approved two-control apply/undo and post-failure verification. [Security review](SECURITY-REVIEW.md) records tested fixes and known boundaries. Authenticode remains an explicitly accepted unsigned-preview limitation because no real certificate was available; the opt-in production signature gate exists, but publisher certification is not completed.

## Next focused tranche

The initial recommended-plan/evidence/post-check tranche is delivered. Next prioritize **Windows integrity diagnosis**, **backup/recovery readiness**, and **trusted exact app upgrades**, each on a separate operation model with honest rollback limits. A planning window such as ten business days is for prioritization, not a promise to ship all three.

Keep torn-WAL recovery and broader release coverage ahead of widening repair scope. Malformed WAL still fails closed and can block Undo: preserve damaged bytes, never automatically truncate/delete or invent originals. Add Home/Pro, Windows 10 and real standard-user/over-the-shoulder broker cases; current Windows 11 Enterprise Evaluation evidence does not certify them.

## Prioritized engineering backlog

**Modes:** Auto = bounded assessment or already-approved execution; Approval = separate informed consent; Guidance = owner performs steps. **Effort S/M/L** is relative engineering/test scope, not a time estimate. Every new mutation needs fresh capability/management gates, independent verification and its own recovery semantics.

| Priority / effort | Work and mode | Independent verification | Rollback limits |
| --- | --- | --- | --- |
| **P0 / L** | Durable operation records and torn-WAL recovery; Guidance on ambiguous state | Crash-prefix, disk-full, restart and reopen tests; verify original ownership and exact state before any replay | Preserve bad WAL; no fabricated original or universal undo |
| **P0 / L** | Home/Pro/Windows 10/original-user gates; future real Authenticode certification [12] | Real token/UAC paths and publisher/timestamp chain with authorized certificate | Signature gate exists, certificate unavailable; no self-signed trust claim |
| **P1 / L** | Component repair: DISM CheckHealth/ScanHealth, conditional RestoreHealth, then SFC verify/scannow [1,2]; Auto diagnose, Approval repair | Parse native results/logs, rerun health check; test missing repair source, pending reboot and offline failure | Separate maintenance engine; component replacement cannot be reversed by Boolean WAL |
| **P1 / L** | WUA online diagnosis and selected quality updates [3]; Approval downloads/install/restart | Actual search result/error/freshness, installed update IDs and post-reboot state | No blanket feature/driver rollout; uninstall may be unavailable or undesirable |
| **P1 / M** | Exact trusted browser/app upgrades [4]; Approval per reviewed plan | Publisher/package/source/version, architecture/scope, pins, installed version and launch test | Preserve pins; migrations may preclude downgrade; no `upgrade --all` default |
| **P1 / L** | File History/Windows Backup coverage and restore-readiness [5]; Guidance then Approval | Successful job plus sample restore to a separate location; owner confirms recovery access | OneDrive presence is not backup proof; retention/sync may propagate damage; never collect recovery secrets |
| **P1 / M** | Non-disruptive network diagnostics; Auto read, Approval conditional DNS flush | Adapter/DNS/proxy/VPN context and required public/private-name reachability before/after | No blind Winsock/IP/firewall reset; flushing cache is not reversible and does not fix every fault |
| **P1 / M** | Disk reliability and NTFS online assessment beyond current space/flags [14]; Auto assess, Approval fixes | Counters/events/filesystem and symptom checks; backup first | Current readiness is not disk repair; unknown counters are not healthy |
| **P1 / L** | Effective service/task/executable-chain permission analysis [6-9]; bounded read-only first | Authz/AccessCheck with documented token/groups, task descriptor and executable/ancestor checks | More findings do not authorize broader repair; retain fixed identities and exact originals |
| **P1 / M** | Account MFA/Hello/passkey and recovery guidance [10]; Guidance in original-user context | Owner confirms successful sign-in and independent recovery path | No web-secret access, automatic enrollment/password reset or journaled credential rollback |
| **P1 / M** | Storage cleanup preview; Auto inventory, Approval allowlist | Per-item eligibility/age/ownership, estimated and actual space, cancellation tests | Never Documents or unknown app data; no DISM `/ResetBase`; deletion may be irreversible |
| **P2 / L** | Reboot continuation and optional tray alerts; Approval setup/restart | Resume owned operation ID, fresh evidence and accessible notifications after reboot [15] | No silent reboot or password storage; expired intent requires renewed review |
| **P2 / M** | Redacted local exports; Approval export | Secret/identifier fixtures, consent preview and escaped output | Copies persist outside app control; no automatic upload |
| **P2 / L** | TUF-style key rotation/thresholds, renewal automation, staged rollout/health checks [11] | Key-expiry/old-client migration, canary health and persistence fault tests [13] | Highest-seen floor already shipped; it is not full TUF or binary rollback |

## Sequencing and acceptance rules

Build new diagnostics before repairs. DISM/SFC, application installation, cleanup and Windows servicing belong to a **separate operation engine**, with preconditions, progress, restart state and truthful partial-completion records. Never market every action as reversible merely because preference undo exists.

Renew metadata before its **90-day** validity expires with unchanged release bytes and nondecreasing same-version timestamps. Test long-offline clients, missed hourly triggers and clocks; decide catch-up/metered/battery policy explicitly. The worker still waits for setup with locks held, without forced termination. Atomic metadata replacement and a protected floor are completed fixes, not automatic install recovery. Add health evidence before expanding that claim.

Use independent oracles: native APIs and workflow checks, not just the setter or UI saying success. Include managed policy, third-party security providers, NAS/printer/VPN needs, developer tools, games/anti-cheat, assistive input, offline devices and ordinary-user tokens. Restrict fixtures to the owned test environment, preserve baselines and never disable protections to manufacture a benchmark. WinPEAS remains blocked/unmeasured, not zero findings.

## Primary references

These sources inform proposals; their APIs/guidance do not certify this product or make these features implemented. Source findings above come from actual repository/evidence inspection. Reconfirm current edition/build requirements before implementation.

1. [Microsoft DISM image health](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/dism/dismcheckimagehealth-function?view=windows-11)
2. [Microsoft SFC](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/sfc)
3. [Microsoft Windows Update Agent search/download/install](https://learn.microsoft.com/en-us/windows/win32/wua_sdk/searching--downloading--and-installing-updates)
4. [Microsoft WinGet upgrade](https://learn.microsoft.com/en-us/windows/package-manager/winget/upgrade)
5. [CISA StopRansomware guide](https://www.cisa.gov/stopransomware/ransomware-guide)
6. [Microsoft AccessCheck](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-accesscheck)
7. [Microsoft AuthzAccessCheck](https://learn.microsoft.com/en-us/windows/win32/api/authz/nf-authz-authzaccesscheck)
8. [Microsoft service security/access rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights)
9. [Microsoft registered-task security descriptor](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nf-taskschd-iregisteredtask-getsecuritydescriptor)
10. [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html)
11. [The Update Framework specification](https://theupdateframework.github.io/specification/latest/)
12. [Microsoft SignTool](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool)
13. [Microsoft ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)
14. [Microsoft storage reliability counters](https://learn.microsoft.com/en-us/powershell/module/storage/get-storagereliabilitycounter)
15. [Microsoft REAgentC options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/reagentc-command-line-options?view=windows-11)

# Marketing copy review

## Final source verdict: PASS

Re-reviewed `website/index.html` on 2026-10-03 after the final copy landed. This review supersedes the earlier findings. All previous copy blockers are resolved. No remaining factual wording blocker was found.

Only `docs/marketing-copy-review.md` was changed by this review. Publication is pending; this verdict applies to current source, not the previously fetched production page.

## Ten editorial checks

1. **Caption removal: resolved.** Lines 59-64 contain no `figcaption` or `aria-describedby`. The accessible label is “See Secblitz in action”. Metadata no longer exposes VM/test-fixture wording. No visible replacement caption is required.

2. **Control and check counts: resolved.** The old “18 settings ... and 19 more” claim is gone. “Make the most of the protection built into Windows” accurately introduces the supported scope without needing catalog counts. Actual scope remains 18 setting controls and 19 advisory checks, corroborated by `windows-v042-results.md:34`.

3. **Selected fixes and Undo: pass.** The hero, steps and consent FAQ consistently describe approved fixes. The Undo FAQ says restoration covers saved settings, some changes may need review, and external changes, updates and installations are excluded. This does not promise full-machine rollback or conflict-free recovery. Evidence: `FEATURES.md:27,284-294`.

4. **Sign-in protection: resolved.** “Strengthen sign-in settings that could expose your account to others on your network” replaces the blanket plain-text-storage claim. It is a reasonable benefit description for the supported sign-in settings, not a promise to clear cached credentials or protect every credential store. Evidence: `FEATURES.md:162-164,174`.

5. **Firewall: resolved.** “Check your firewall protection at home, at work and on public networks” accurately describes the check. The heading “Keep unwanted connections out” is benefit-led wording in that context, not a claim that Secblitz removes all allow rules or guarantees total filtering. Evidence: `FEATURES.md:153-158,170`.

6. **Advisory next steps: resolved.** “Get clear next steps for drive encryption, updates and other protections that need your attention” no longer promises a Windows page for every check. Evidence: `FEATURES.md:204-228`.

7. **Extra tools and passwords: resolved.** “You choose which tools to use” removes the contradiction with enabled automatic monitoring. The monitor is correctly described as reporting without changing settings. “Generate a strong password to save in your password manager” describes the user's next step and does not claim automatic vault integration. Bitwarden installation requires approval. Evidence: `FEATURES.md:65-75,298-328`.

8. **Product scope: pass.** The antivirus FAQ explicitly says Secblitz does not replace antivirus; the quick scan is credited to Microsoft Defender. No SFC, disk-repair, backup implementation, total-fix or “100% secure” claim appears. The Defender, UAC and installer/service benefits fit the supported settings. Evidence: `FEATURES.md:147-178` and `security-model.md:7`.

9. **Updates and download details: pass.** Hourly checks, verification, installer opt-out and no forced app close/restart match the documented updater. `windows-v042-results.md:3-6,31-38,43-70,170-178` records the genuine published 0.4.1 to 0.4.2 PASS and both pinned origins. Unsigned-installer information and the matching 0.4.2 checksum are inside collapsed “Download details”. Native publisher signing and signed update metadata remain separate facts; the copy does not confuse them. The displayed 3.8 MB matches the documented 3,813,017-byte installer.

10. **Presentation and review scope: pass at source level.** Current HTML is English, has no em dashes, and retains the current layout structure. The video has muted looping inline playback and no native `controls` attribute. Earlier source inspection confirmed preference-aware autoplay, reduced-motion stills and responsive layout in `app.js` and `styles.css`; no new runtime test was run for this copy revision. The updated video/footer removal is reported by the media owner, not independently frame-verified in this editorial review. Asset URLs now include `?v=marketing`.

## Handoff

**PASS. No copy changes requested.** Previous blockers are resolved in the final source. Main can proceed with publication and its deployment verification.

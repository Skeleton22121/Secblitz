# Security

## Reporting a vulnerability

Report security problems privately, never in a public issue:

- Email **support@secblitz.lol**, or
- use GitHub's private reporting: **Security → Report a vulnerability**.

Include the Secblitz version, your Windows version and the steps to reproduce.
We aim to reply within a week and to ship a fix or give a clear answer before
anything is made public. Only the latest release receives fixes.

Reports about the updater, the elevated helper, undo, or anything that could
let another user or program change your settings are especially welcome.

## How Secblitz is built to be safe

- **Nothing changes without approval.** Checks are read-only. Fixes run only for
  the items the user approved, and optional settings are never pre-selected.
- **A fixed set of actions.** Secblitz can only change the settings built into
  it. It never runs commands, scripts or downloads chosen at run time. The
  elevated helper accepts fixed request codes only, never paths or commands.
- **Undo.** Original values are written to a local journal before each change,
  and History restores them newest first. It does not undo Windows updates, app
  installs or changes made outside Secblitz.
- **Signed updates.** An update must match the Ed25519 public key pinned in the
  app and the installer's exact size and hash. Older versions are refused.
- **No data collection.** No account, no ads, no telemetry. See the
  [privacy policy](https://secblitz.lol/privacy.html).

## Known limits

The executables are not Authenticode-signed yet, so trust in the first download
rests on HTTPS and the published checksum. Windows 10, Home and Pro editions and
many hardware setups are not yet tested.

More detail: [security model](docs/security-model.md),
[update contract](docs/update-contract.md),
[security review](docs/SECURITY-REVIEW.md). These were written for earlier
releases; this page is current.

# Releasing Secblitz

How a version goes from the main branch to users. The build runs in GitHub
Actions from source. Two things stay with the owner on purpose: signing the
update manifest (the key never leaves the owner's machine) and pressing
Publish.

```
bump PR -> merge -> tag -> CI draft -> (SignPath) -> verify -> sign manifest -> publish -> website
```

## Files that do this

| File | Job |
| --- | --- |
| `scripts/bump-version.py` | Moves the version in every file and the changelog |
| `scripts/release-notes.py` | Prints one version's changelog section (the release text) |
| `.github/workflows/ci.yml` | Tests, lints and tool tests on every push and pull request |
| `.github/workflows/bump-version.yml` | Runs the bump script and opens a pull request |
| `.github/workflows/tag-release.yml` | Tags a merged "Release X.Y.Z" commit and starts the build |
| `.github/workflows/release.yml` | Builds, optionally signs, attests and drafts the GitHub Release |
| `scripts/build-release.ps1` | The build itself (`-Stage Exe`, `-Stage Setup` or both) |
| `rust-toolchain.toml` | The one pinned compiler (1.93.0) |
| `CHANGELOG.md` | Release notes, in plain words |

`.github/workflows/windows.yml` is the older full check (RustSec audit,
installer lifecycle tests). It still runs on every push. Release builds use
`release.yml`.

## One-time repository setup

1. Settings, Actions, General: allow GitHub Actions to create pull requests
   (needed by `bump-version.yml`).
2. Settings, Branches: protect `main` and require the `CI` checks.
   Pull requests opened by the built-in token do not start CI by themselves.
   Close and reopen the bump pull request, or push an empty commit to it, to run
   the checks.
3. Settings, Rules: protect tags `v*` so only the owner and the workflow can
   create them.
4. Optional: replace `OWNER/REPO` in `docs` and `README.md` links once the repository exists.

## Steps

### 1. Write the notes

While you work, add short plain-words entries under `## [Unreleased]` in
`CHANGELOG.md` (Added, Changed, Fixed). No jargon, no em dashes. The release
page shows this text word for word.

### 2. Bump the version (pull request)

Either run the workflow: Actions, Bump version, Run workflow, enter the new
version. Or run it yourself:

```sh
python3 scripts/bump-version.py 0.8.1 --dry-run   # shows what would change, writes nothing
python3 scripts/bump-version.py 0.8.1
```

The script updates `Cargo.toml`, `Cargo.lock`, `assets/secblitz.rc`,
`assets/secblitz.manifest` and moves `Unreleased` into a dated `## [0.8.1]`
section. It refuses a version that is not greater than the current one, and an
empty `Unreleased` section (override with `--allow-empty`). `--dry-run` never
writes. `--site-only` (used at step 9, not now) moves the download links and version
text in `README.md` and `website/index.html` for the version `Cargo.toml`
already has. `--site` does that together with a bump.

Special case: while a version is being developed, its changelog heading reads
`## [0.8.0] - Unreleased` and `Cargo.toml` already says 0.8.0. To release it,
run `python3 scripts/bump-version.py 0.8.0`, which only adds the date.

The workflow commits "Release X.Y.Z" on branch `release/vX.Y.Z` and opens a
pull request. Read the changelog diff, then merge it. Any merge type works.
For squash or rebase the commit title must stay "Release X.Y.Z" (a trailing
"(#12)" is fine). A plain merge commit is recognised by its branch name
`release/vX.Y.Z`, so do not rename the branch.

### 3. Tag

`tag-release.yml` runs when the merge lands on `main`. It checks that the
commit title, `Cargo.toml` and the changelog agree, creates the tag `vX.Y.Z`
and starts `release.yml`. To tag by hand instead:

```sh
git tag -a v0.8.1 -m "Secblitz 0.8.1" && git push origin v0.8.1
```

### 4. CI builds a draft

`release.yml` does, in order:

1. Checks the tag equals the `Cargo.toml` version, the commit is on `main`, and
   the changelog has a dated section with entries.
2. Builds the exe on `windows-latest` with MSVC after running all tests
   (`build-release.ps1 -Stage Exe`).
3. Signs the exe (only if SignPath is on, see below).
4. Packs the Inno Setup from that exe (`-Stage Setup`).
5. Signs the setup (only if SignPath is on).
6. Writes `SHA256SUMS`, and creates build provenance attestations for the exe
   and the setup.
7. Creates a draft GitHub Release named "Secblitz X.Y.Z" with the changelog
   section as text and the setup, the portable exe and `SHA256SUMS` attached.

To rebuild an existing tag: Actions, Release, Run workflow, enter the tag. If a
draft for that tag already exists, delete it first.

### 5. SignPath (optional)

Off until you turn it on. See "Turning on SignPath" below.

### 6. Verify the draft

Download the files from the draft, then:

```sh
sha256sum --check SHA256SUMS
gh attestation verify secblitz-0.8.1-windows-x64-setup.exe --repo OWNER/REPO
gh attestation verify secblitz-0.8.1-windows-x64.exe --repo OWNER/REPO
```

The attestation proves the file was built by this repository's `release.yml`
from the tagged commit. On Windows, if signing was on, also check
`Get-AuthenticodeSignature` shows `Valid` for both files. Install the setup on
a test PC.

### 7. Sign the update manifest (offline, by the owner)

The update feed key is never in GitHub. On your own machine, with the
published setup bytes (after SignPath, if used) and the key outside the repository:

```sh
python3 scripts/sign-release.py --key /path/to/secret.pem \
  --public-key assets/update-public-key.hex --version 0.8.1 \
  --installer secblitz-0.8.1-windows-x64-setup.exe --output stable.json
```

Sign only the final bytes you will publish. If the setup changes later (for
example it is re-signed), sign again. Staged rollouts and renewals use
`scripts/release-authorize.py` and `scripts/release-renew.py` as before.

### 8. Publish the release

Edit the draft if needed, optionally attach `stable.json`, then press Publish.

### 9. Deploy the website

Follow `docs/website-deployment.md`. In short: put the final setup in `dist/`,
copy the portable exe to `dist/secblitz.exe` (the staging script reads that
name), put the signed `stable.json` in `website/releases/`, then

```sh
python3 scripts/bump-version.py 0.8.1 --site-only   # README and website download links
python3 scripts/stage-pages.py --expected-version 0.8.1
```

and deploy `dist/pages`. Update the size text ("8.2 MB") and the checksum on
the page by hand. Check the live feed and the download hashes afterward.

Do the `--site-only` step only now, after the files are really published,
because the website links must never point at a download that does not exist yet.

## Reproducible build

- One compiler: `rust-toolchain.toml` pins 1.93.0. CI installs exactly that.
- `--locked` on every cargo command, so `Cargo.lock` decides every dependency.
- `build-release.ps1` adds `--remap-path-prefix` for the workspace, cargo home
  and rustup home, `/Brepro` (fixed PE timestamp) and `/PDBALTPATH`, so no
  local path ends up in the exe.
- `SOURCE_DATE_EPOCH` is set from the commit time for tools that honour it.
- Every third-party action is pinned to a full commit SHA, with the version in a comment.
- Only the compiled crate downloads are cached in the release build, never
  compiled output.

Same source, same compiler and same runner image should give the same exe. The
runner image and Windows SDK can still change over time; the attestation, not a
byte comparison, is the proof of origin.

## Turning on SignPath

Secblitz plans to use the SignPath Foundation (free code signing for open
source). Check their current terms at https://signpath.org/ before applying.
What it needs, as far as we know:

- A public GitHub repository with an OSI-approved licence (Secblitz is MIT).
- Builds that run on GitHub Actions from that repository (this pipeline does).
- A project that is actively maintained, with a clear description, and a
  code signing policy page on the website or README, including the line that
  credits SignPath Foundation. The certificate is issued to the foundation.
- No software that could be treated as malware or a hacking tool. Secblitz
  changes security settings, so describe it plainly. The foundation decides.

Once approved:

1. In SignPath, create the project, an artifact configuration for the exe and
   one for the setup (a zip containing one `.exe`, signed with Authenticode),
   and a signing policy. Add the GitHub trusted build system and connect it to
   this repository.
2. Create an API token for a submitter user.
3. In GitHub, Settings, Secrets and variables, Actions:

| Kind | Name | Value |
| --- | --- | --- |
| Secret | `SIGNPATH_API_TOKEN` | The API token |
| Variable | `SIGNPATH_ORGANIZATION_ID` | Organization id |
| Variable | `SIGNPATH_PROJECT_SLUG` | Project slug |
| Variable | `SIGNPATH_SIGNING_POLICY_SLUG` | Signing policy slug (for example `release-signing`) |
| Variable | `SIGNPATH_EXE_ARTIFACT_CONFIGURATION_SLUG` | Artifact configuration for the exe |
| Variable | `SIGNPATH_SETUP_ARTIFACT_CONFIGURATION_SLUG` | Artifact configuration for the setup |
| Variable | `SIGNPATH_ENABLED` | `true` to turn the signing jobs on |

4. Run a release. The exe is signed first, the setup is built from the signed
   exe, then the setup is signed. A signing policy that needs approval waits
   for you in SignPath.

Set `SIGNPATH_ENABLED` to anything else, or delete it, to turn signing off again.

Known limit: Inno Setup puts an uninstaller inside the setup. SignPath signs
the outer setup, not that inner uninstaller. The uninstaller is unsigned until
the build is changed to sign it during compilation.

## Things CI will never do

- Hold the update-feed signing key or sign `stable.json`.
- Publish a release or deploy the website.
- Push to `main` (the bump workflow opens a pull request instead).

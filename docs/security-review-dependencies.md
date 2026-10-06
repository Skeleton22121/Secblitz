# Dependency and release-gate security review

Reviewed 2026-10-03 for Secblitz 0.4.2. Scope: the actual `Cargo.lock`,
`video/package-lock.json`, final local Windows artifacts, and release gates.

## Findings

**No known dependency vulnerabilities were reported by either audit.** There are
no confirmed high or moderate advisories to assign from these results. RustSec
reported one maintenance warning. A clean advisory scan is not proof that all
application, toolchain or supply-chain vulnerabilities are absent.

| Scope | Observed result | Owner and follow-up |
| --- | --- | --- |
| Rust lockfile, 213 dependencies | 0 vulnerabilities; 1 unmaintained warning | Native dependency owner: review `indicatif` migration below in a separate change. |
| Video lockfile, npm reports 302 total dependencies | 0 critical, high, moderate, low or informational vulnerabilities | Video pipeline owner: rerun the full lockfile audit before render-tool updates and release asset generation. |
| Publisher signing | Current EXE and installer have no embedded Authenticode signature | Release owner: obtain/select a trusted code-signing certificate and use the production gate. Current unsigned previews remain available. |

### Rust maintenance warning

- [`RUSTSEC-2025-0119`](https://rustsec.org/advisories/RUSTSEC-2025-0119.html):
  `number_prefix 0.4.0` is unmaintained.
- Confirmed path using `cargo tree --locked --offline -i number_prefix`:
  `secblitz 0.4.2 -> indicatif 0.17.11 -> number_prefix 0.4.0`.
- RustSec supplies no CVSS score, CVE alias or patched version for this warning.
  It recommends the maintained `unit-prefix` alternative. This is not a confirmed
  exploitable vulnerability and should not be presented as a high/moderate CVE.
- Follow-up: the native dependency owner should evaluate an `indicatif` version
  that removes this transitive crate, then validate progress rendering, Windows
  tests and Clippy. A direct `unit-prefix` dependency alone does not replace
  `indicatif`'s dependency. No lockfile or manifest was upgraded in this review.

### Video tooling boundary

`video/package.json` defines a private Remotion authoring/render pipeline using
`@remotion/cli`, `@remotion/fonts` and `remotion` 4.0.532, React/React DOM 19.3.0,
and TypeScript 5.9.3. These are not the website's browser-runtime dependencies
and are not linked into the native Windows executable.

They still matter to release integrity: `video/scripts/deliver.mjs` delivers
rendered MP4 and WebP assets into `website/assets`. Compromised render tooling
could alter those assets or execute with its operator/CI account's permissions.
A media change does not itself generate a valid binary-update signature.
However, rendering as the signing operator, with access to that same user's
private-key paths or signing authority, would cross that boundary. The pipeline
owner should keep render execution separate from the signing account and its
credentials. No private-key or Cloudflare credential was accessed in this review.

The npm audit included development dependencies and used the lockfile without
installing packages or running lifecycle scripts. It does not audit external
FFmpeg, downloaded browser binaries, Inno Setup, or the Rust compiler/SDK.

## Reproducible audit evidence

Observed tools: `cargo-audit 0.22.2`, Cargo 1.93.1, rustc 1.93.1,
Node.js v24.21.0 and npm 11.19.0. npm registry:
`https://registry.npmjs.org/`.

The official crates.io `cargo-audit` was installed with `cargo install --locked`
into the verified existing `target` directory, with a separate tool build target
and two compilation jobs. No root installation or project Cargo configuration
change was needed. Equivalent pinned installation and audit commands, from the
repository root:

```sh
cargo install cargo-audit --version 0.22.2 --locked \
  --root "$PWD/target/tools" --target-dir "$PWD/target/tools-build" --jobs 2
target/tools/bin/cargo-audit --version
target/tools/bin/cargo-audit audit --file Cargo.lock --json
cargo tree --locked --offline -i number_prefix
npm --prefix video audit --package-lock-only --ignore-scripts --json
sha256sum Cargo.lock video/package-lock.json
```

The actual npm audit ran from `video/` with the same audit options. Both audit
commands exited 0. RustSec's freshly fetched database reported:

- Database commit: `f8dee89e1b2f2f1eaf548312df7655fe5202a302`.
- Database last updated: `2026-10-02T22:27:46+02:00`.
- Advisory count: 1,288; lockfile dependency count: 213.
- Vulnerabilities: `found: false`, `count: 0`, `list: []`.
- No ignored advisories, severity threshold, architecture or OS filter.
- Only warning: `RUSTSEC-2025-0119`, `number_prefix 0.4.0`, unmaintained.

npm returned audit report version 2, `vulnerabilities: {}`, all severity counts
zero and `metadata.dependencies.total: 302`.

Lockfile SHA-256 values were identical before and after auditing:

| File | SHA-256 |
| --- | --- |
| `Cargo.lock` | `68a6b9a787c9126f2867a250c19f12e2a9c3d4b09cac3a5005b18f3a24724b1a` |
| `video/package-lock.json` | `635a47c3da3a110b2db67c7f6a9e45fa03bdb297d5c7b509f0fccfa8b918bee5` |

Future audit results can change as advisory databases change. These hashes,
versions and the RustSec database commit identify this review's snapshot.

## Final 0.4.2 PE inspection

GNU objdump 2.46 and independent PE-header parsing inspected the existing files
without executing or rebuilding them. Hashes match `dist/SHA256SUMS` and the
0.4.2 artifact identities recorded in Windows validation.
The existing Windows `--version` evidence is from that earlier validation; it
was not rerun on this Linux host.

| Artifact | SHA-256 |
| --- | --- |
| `dist/secblitz.exe` | `78c666e456b3f72fbe503e4aad6213b1baec4f7172e3e8cacd17f9bf893b4672` |
| `dist/secblitz-0.4.2-windows-x64-setup.exe` | `28c93869508923b2ea865267025dc6c0d9e92e5d343e5a78f175d32e5b1b83c4` |

- Application: x64 PE32+, `DllCharacteristics = 0x0160`, enabling
  `HIGH_ENTROPY_VA`, `DYNAMIC_BASE` (ASLR) and `NX_COMPAT` (DEP).
- Application base relocations are present, with size `0x5b14`.
- Application COFF symbol count and Debug Directory are zero. There are no
  `.debug*` sections. COFF characteristics `0x022e` mark symbols, line numbers
  and debugging information stripped. Runtime unwind sections remain as expected.
- Installer: x86 Inno loader, `DllCharacteristics = 0x8140`, including
  `DYNAMIC_BASE` and `NX_COMPAT`, with base relocations present. The x86 loader
  packages the x64 application; high-entropy ASLR is an application x64 check.
- Both Security Directories are zero: no embedded Authenticode signature.

Recheck with `objdump -p`, `objdump -h` and `objdump -f` on the named files, and
compare their SHA-256 values before applying these observations to a later build.
These artifact checks do not prove the current source/lockfile produced the
existing artifacts; any new build needs its own final-byte verification.

## Release gates implemented

`scripts/build-release.ps1` now accepts `-RequirePublisherSignature`:

```powershell
# Existing unsigned preview path remains supported.
./scripts/build-release.ps1 -DownloadInno

# Run on the authorized Windows signing host after selecting real credentials.
./scripts/build-release.ps1 -RequirePublisherSignature -CertificateThumbprint $PublisherThumbprint
```

Production signing requires credentials; the current unsigned preview is not
publisher-certified. The opt-in gate fails before building if no thumbprint is
provided. No certificate was selected or created, and no self-signing substitute
was introduced. A thumbprint selects a certificate; SHA-256 remains the file and
timestamp digest algorithm.

When a certificate is supplied, the script validates the final EXE and installer
using SignTool `verify /pa /all /tw`, Windows Authenticode
status, exact expected signer thumbprint and timestamp-certificate presence.
Nonzero verifier exit codes fail the build. The final x64 EXE must also have
ASLR, high-entropy ASLR, DEP and base relocations. Inno's existing signed
uninstaller configuration is retained.

Ordering remains: sign EXE, compile/sign installer, verify both final artifacts,
compute checksums, then optionally sign the Ed25519 update manifest over the
final installer bytes. Authenticode and the pinned-key update manifest are
different checks; the publisher gate does not replace feed signing.

`.github/workflows/windows.yml` now installs pinned official `cargo-audit 0.22.2`
in a separate Linux job and audits the real Rust lockfile against current
RustSec data. Audit/database failures block the dependent Windows job and its
artifact upload. The maintenance warning remains visible and is not suppressed;
it is not treated as a vulnerability failure. Existing locked tests and Clippy
with `-D warnings` remain mandatory. Checkout credentials are not persisted,
permissions remain `contents: read`, and the workflow uses no signing secrets.
Video npm tooling is not added to this native build job.

## Validation and remaining work

- Passed both live dependency audits and before/after lockfile hash checks.
- Passed PowerShell 7.4.13 parsing and local behavioral checks: real PE accepted;
  independently clearing each mitigation flag, removing relocations or truncating
  the PE rejected; missing production credentials rejected before build; native
  verifier failure propagated.
- Mocked signature checks accepted the expected timestamped signer and rejected
  unsigned status, wrong thumbprint, absent timestamp and missing signer. These
  checks do not substitute for Windows certificate-chain validation.
- Passed YAML parsing and checks for the blocking audit dependency, mandatory
  Rust tests/Clippy, and final signature/checksum/feed-signing order.
- Full hosted CI, Windows PowerShell 5.1 execution, MSVC rebuild, Inno lifecycle
  tests and real certificate/timestamp verification of the modified pipeline are
  **pending**. No nightly or hosted workflow run is claimed here.

## Binary packing

`Cargo.toml` already has `lto = true`, `codegen-units = 1` and `strip = true`
for optimized native releases. The inspected application is already stripped.
An optional packer such as UPX would be a size/compatibility decision, not a
security fix: packing does not prevent copying or reverse engineering and can
increase antivirus false positives. No packer was added. Any future byte-changing
packing step would need to precede Authenticode, checksums and manifest signing,
with the PE mitigation and Windows behavior checks rerun on its output.

//! Upstream assessment, observed 2026-10-03 UTC. NOT an approved catalog.
//!
//! Publisher stable feed:
//! https://update.code.visualstudio.com/api/update/win32-x64-user/stable/latest
//! Observed productVersion 1.140.0, commit
//! 07f806f999227108933c2e30515b26eecc1fda74, publisher timestamp 1790759204424,
//! SHA-256 b66449d7acf503f556b5a50593717b0092f48df1f62ce38a7dba5b93da0b425e.
//!
//! Independently fetched Microsoft WinGet installer manifest:
//! https://github.com/microsoft/winget-pkgs/blob/master/manifests/m/Microsoft/VisualStudioCode/1.140.0/Microsoft.VisualStudioCode.installer.yaml
//! It identifies 1.140.0, x64/user/Inno, release date 2026-09-30, and the same
//! publisher URL and SHA-256. The independently fetched locale.en-US manifest
//! identifies Microsoft Corporation and https://code.visualstudio.com/license.
//! The current license page was fetched and reviewed; the previous bundled
//! historical license is not used as a current license or production approval.
//!
//! Current runtime release inspected:
//! https://github.com/microsoft/winget-cli/releases/tag/v1.29.380
//! Published 2026-09-21; stable, not prerelease. The old 1.11-only assumption is
//! retired with the old executor. v1.29.380 Commands/ExportCommand.cpp retains
//! source-scoped JSON export and explicit source-agreement arguments; its release
//! notes additionally describe stored installer override/custom arguments in
//! export (never safe to replay blindly through import). Microsoft/PinningIndex.cpp
//! still uses LocalState/pinning.db and the v1 pin interface. These are source
//! observations, not proof that the prior native runner supports this runtime.
//!
//! Exact current installer source inspected:
//! https://github.com/microsoft/vscode/blob/07f806f999227108933c2e30515b26eecc1fda74/build/win32/code.iss
//! - PrepareToInstall calls StopTunnelServiceIfNeeded and StopTunnelOtherProcesses.
//! - Tunnel stopping explicitly invokes Stop-Process -Force (line 1457).
//! - Tunnel service handling may uninstall/reinstall the service (1485, 1879).
//! - KillContextMenuComSurrogate force-stops matching dllhost.exe processes (1724).
//! - Shell integration may remove/re-register an Appx package (1779, 1797).
//! - Normal versioned installs run inno_updater.exe --gc against prior payloads
//!   (1866-1871). Payload metadata is under VersionedResourcesFolder, unlike the
//!   legacy flat-layout verifier.
//!
//! Separate consent to tunnel stopping alone does not validate this full set of
//! effects. The old preflight only checked Code.exe, and the old local verifier
//! does not establish the active current versioned payload. Complete preflight,
//! verification, and supervision of the vendor helper/service operations have
//! not been established. No installer-version or signed-hash match is promoted
//! to current security. Production capabilities are unconditionally unsupported,
//! the selectable catalog is empty, and all live execution wrappers are removed.
//!
//! Re-enabling requires a separately reviewed implementation, fresh authenticated
//! publisher/manifest/license evidence, an absolute review expiry <=30 days, a
//! current stable recheck, explicit consent for every required disruptive effect,
//! and no-active-Code/tunnel checks. No approval permits terminating SecBlitz.
//! No VM or Windows installer was run for this correction.

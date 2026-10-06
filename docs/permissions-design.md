# Exact-state service permission repair

## Public contract and integration

The permissions module owns all native service-ACL operations. Its public API is:

```rust,ignore
pub fn controls() -> Vec<Control>;
pub fn validate_value(id: &str, value: &serde_json::Value) -> anyhow::Result<()>;
pub fn repair_target(id: &str, before: &serde_json::Value) -> anyhow::Result<serde_json::Value>;
pub fn observe(id: &str) -> anyhow::Result<Observation>;
pub fn write(id: &str, value: &serde_json::Value) -> anyhow::Result<()>;
pub fn audit() -> anyhow::Result<Vec<Finding>>;
pub fn with_permissions(delegate: Box<dyn Backend>) -> Box<dyn Backend>;
pub fn with_audit(delegate: Box<dyn Backend>) -> Box<dyn Backend>;
```

Only `permissions.service.bits` (BITS) and `permissions.service.wuauserv`
(wuauserv) are repairable. `controls()` advertises the string sentinel
`service-dacl-repair-v1` as the catalog target. **The sentinel is never a state or
a value to write**; `validate_value` and `repair_target` reject it. Observations
always contain an exact snapshot, including for structurally supported but
ineligible objects. A missing service, inaccessible descriptor or unsupported
descriptor structure returns an error instead of inventing a state.

`with_permissions` appends the two controls, routes the permissions namespace
through the fixed-name native boundary, and appends the five advisory findings.
All other controls, writes and machine identity are delegated. Unknown permission
IDs are rejected, not forwarded. `with_audit` remains an advisory-only adapter.
Main can opt in using:

```rust,ignore
let backend = permissions::with_permissions(platform::backend()?);
let mut engine = Engine::open(platform::state_dir()?, backend)?;
```

Native observation and writes call the separately implemented
`platform::permission_gate(id) -> Result<()>`. Observation still returns its exact
state with `eligible: false` if the gate, service identity checks, trusted owner
check or pure transform fails. Eligible observations have reason
`Eligible service permission repair`. Writes repeat the gate before opening with
WRITE_DAC and again immediately before final identity/state checks and mutation.
The adapter cannot bypass this gate even if write is called directly.

Pure catalog/validation/transform helpers work on non-Windows hosts. Native
observe, write and audit fail explicitly on unsupported platforms. The audit
adapter converts an audit-wide failure into an `unknown` finding.

## Snapshot encoding and bounds

State is a JSON **string**, `dacl-v1:<lowercase hex>`. The hex represents a
canonical self-relative security descriptor containing:

1. A 20-byte revision-1 header with fixed section ordering and offsets.
2. The complete owner SID.
3. The complete primary-group SID.
4. The exact DACL bytes, including ACE order and ACL padding, or a NULL offset.

The header carries SELF_RELATIVE, DACL_PRESENT, DACL_DEFAULTED,
DACL_AUTO_INHERITED and DACL_PROTECTED flags. Other descriptor flags are refused.
The SACL offset must be zero. Owner/group identity and the supported control bits
participate in every exact comparison, although writes never set owner or group.
Native buffers can order sections differently; normalization changes only the
container layout, not ACL content or principals. Canonical JSON input must
round-trip byte for byte: duplicate/overlapping section offsets, alternate hex
case, arbitrary suffixes, gaps, SDDL, paths and unused trailing bytes are refused.
Repeated ACEs for one SID are legitimate ACL data and are preserved individually.

Decoded descriptors are limited to **16 KiB**, hence state strings are at most
32,776 ASCII bytes. Native QueryServiceObjectSecurity uses its documented maximum
**8 KiB** buffer. All descriptor, ACL, ACE and SID ranges are bounds checked with
byte slices. SIDs have at most 15 subauthorities. Unknown ACE layouts can be
structurally retained for ineligible observations, but can never enter a repair
or restoration. Unknown descriptor layouts cannot be safely represented and
fail observation.

## Deterministic repair

For both IDs, `repair_target(id, before)` is the same versioned pure function:

- Require a trusted owner: SYSTEM (`S-1-5-18`), Builtin Administrators
  (`S-1-5-32-544`) or the well-known TrustedInstaller service SID.
- Refuse NULL/absent DACLs and **any** deny, inherited, flagged, object,
  callback/conditional or otherwise unsupported ACE. This first implementation
  deliberately does not attempt even canonical deny evaluation.
- For ordinary explicit ALLOW ACEs, inspect only Everyone (`S-1-1-0`),
  Authenticated Users (`S-1-5-11`) and Builtin Users (`S-1-5-32-545`).
- On these principals, remove SERVICE_CHANGE_CONFIG (`0x2`), DELETE (`0x10000`),
  WRITE_DAC (`0x40000`) and WRITE_OWNER (`0x80000`).
- If a changed ACE includes generic bits, map them to service rights first:
  GENERIC_READ = `0x2008d`, GENERIC_WRITE = `0x20002`, GENERIC_EXECUTE = `0x20170`,
  GENERIC_ALL = `0xf01ff`. Then remove the four dangerous rights. In particular,
  GENERIC_WRITE retains READ_CONTROL; GENERIC_ALL retains service start/query,
  stop, pause, interrogation and user-defined-control rights.
- Only a changed ACE's four mask bytes are replaced. Zero-mask ACEs are kept.
  Other principals, administrator/SYSTEM grants, SID bytes, ACE order, ACL
  padding, owner/group and control flags remain byte-identical. Generic-only
  safe ACEs are left byte-identical too.

An eligible compliant descriptor returns the identical state. The transform is
idempotent. It does not replace the descriptor with a canned Windows-version
default. Duplicate broad-principal ALLOW ACEs are each transformed in place.

## Native mutation and identity checks

Only local fixed-name services can be opened. Audit requests READ_CONTROL;
repair observation additionally requests SERVICE_QUERY_CONFIG. Writing requests
READ_CONTROL | WRITE_DAC | SERVICE_QUERY_CONFIG, never WRITE_OWNER,
SERVICE_CHANGE_CONFIG, start/stop or SACL access.

Before mutation, the service must have a trusted current owner and still be a
WIN32_SHARE_PROCESS service running as LocalSystem. QueryServiceConfig must name
the Windows system directory's `svchost.exe`, optionally quoted, with exactly
`-k netsvcs` or `-k netsvcs -p`; the standard `%SystemRoot%\system32\svchost.exe`
form is also accepted. Other legitimate OS/vendor variants are conservatively
ineligible. The expected executable is resolved with GetSystemDirectoryW, opened
with READ_CONTROL/attributes and OPEN_REPARSE_POINT, and pinned through the write
with read sharing only. It must be a nonempty regular non-reparse file owned by
SYSTEM, Administrators or TrustedInstaller. Configuration strings are decoded
only from bounded ranges inside the native query buffer.

These are identity/metadata checks, **not Authenticode verification or proof of
binary/DLL/ancestor-path integrity**. There is no process execution, service stop
or configuration mutation in the native permissions code; the fixed policy gate
is supplied by the platform's existing policy adapter.

After reading the current snapshot on the held service handle, write requires:

- Exact owner, group and descriptor-control equality with the requested state.
- Either `repair(current) == desired` (apply), or `repair(desired) == current`
  (restore). Thus a request cannot replace unrelated ACEs even if syntactically
  valid. A before image that intentionally restores dangerous rights is accepted
  only as the inverse of this fixed repair; its provenance is the engine's
  protected journal, not this low-level function.
- After repeating the policy and identity checks, exact equality with the
  previously read current state immediately before mutation.

`SetServiceObjectSecurity` is called with **DACL_SECURITY_INFORMATION only**.
The module does not change owner, group, SACL or request protection changes.
It rereads owner/group/DACL and requires exact desired-state equality afterward.
A failed call or readback mismatch returns an error; no blind second write or
automatic cleanup is attempted. The engine retains the prepared transaction for
recovery. Any Windows normalization that prevents exact readback is a visible
failure, not silently accepted approximate compliance.

## Engine contract: exact rollback without a schema change

The existing journal's string `before` variant can hold this state. No new schema
is necessary because the desired after image is **deterministically derived
from the durable before image**, with the version encoded by `dacl-v1` and the
catalog sentinel. The engine integration must:

1. Independently allowlist the two IDs and sentinel. Validate every persisted
   before image with `validate_value` and require `repair_target` to succeed.
2. Increase the bounded WAL line size from 4096 to e.g. **128 KiB**, retaining
   total-file and transaction limits. Test an actual large descriptor roundtrip.
3. For audit/compliance and new apply, derive the target from the observation;
   never compare the observed state with the sentinel.
4. Flush `Prepare { before }` before writing `repair_target(id, before)`.
5. For pending apply, final verification, restart recovery and revert, derive
   the target from that **recorded before**, not from a fresh current state.
   Revert only when current equals that exact derived target, or close a no-op
   when current already equals the recorded before. Otherwise report drift.
6. Repeat eligibility checks and retain recovery intent on failed writes.
   Existing WAL crash/recovery behavior applies to the string snapshots.

Collapsing state to `safe`, regenerating a target from current during rollback,
or treating the sentinel as a write value is forbidden. Safe-to-safe changes,
trusted-owner changes, group changes and protection changes must cause conflicts.

Win32 offers no service-DACL compare-and-swap. The native final reread narrows
but cannot eliminate the check/write race; the same limitation applies to the
registry backend. The engine lock coordinates Secblitz instances only. Owner or
configuration changes in the remaining race can produce a post-write mismatch;
they are never claimed to have been preserved atomically. The SACL is neither
read nor modified and is not a drift fingerprint. Holding a service handle binds
a single write to that object; across process restarts, deletion/recreation with
identical identity and descriptor is not distinguishable. A hostile administrator
or compromised protected journal remains outside the existing trust boundary.

## Advisory audit remains five services

`audit()` reviews BITS, wuauserv, WinDefend, Schedule and SecblitzMonitor. The
last three remain advisory-only. Each gets a finding even if another fails.
Missing services are `info`, read/parser failures are `unknown`, and dangerous
ALLOW candidates or unrestricted DACLs are `review`. Eligible repair is determined
by the separate fixed control observation, never by the advisory result.

This scan is not AccessCheck/effective-access evaluation. Denies and complex ACEs
make the assessment ambiguous. It covers only the three broad principals and
does not claim a machine is secure when no candidates are found. DELETE is a
destructive right, not independently proof of privilege escalation. The audit
does not assume SecblitzMonitor runs as SYSTEM; it is installed as LocalService.
Service registry/DLL permissions, arbitrary third-party services, other groups,
privileges, executable replacement and ancestor-directory access are outside
this repair's scope.

## Research and verification

Microsoft references used:

- [Service security and access rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights)
- [QueryServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-queryserviceobjectsecurity)
- [SetServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-setserviceobjectsecurity)
- [QueryServiceConfigW](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-queryserviceconfigw)

Portable tests cover deterministic roundtrip, all three principals, generic
mapping, safe-right and administrator preservation, zero-mask retention,
idempotence, inverse restore, owner/group/protection and safe-to-safe drift,
complex ACL rejection, NULL versus empty DACLs, malformed/truncated descriptors,
overlapping offsets, canonical encoding, size limits and exact ID allowlisting.
Windows cross-checks use `target/build-tools/cross-env.sh`; no guest changes
are performed. Runtime Windows mutation/readback and crash/revert validation
remain distinct from cross-compilation and pure tests.

# Independent permissions review - agent 1 of 4

Scope: `src/permissions.rs`, `src/permissions/{state,descriptor,windows}.rs`.
Engine integration was read for context; changes are limited to these files and
this report. No guest operations or Windows security-object mutations were run.

## Findings fixed

### P1: attributes-only svchost handle did not pin replacement

`identity()` opened the expected service host with `READ_CONTROL |
FILE_READ_ATTRIBUTES`, read sharing only, and `FILE_FLAG_OPEN_REPARSE_POINT`.
The sharing exclusion needed to prevent replacement is not established by a
metadata-only open. Retaining that handle was therefore insufficient evidence
that the checked file could not be renamed/replaced during the operation.

The open now also requests **FILE_READ_DATA** (`0x20081` total access), retaining
`FILE_SHARE_READ` only. Both identity handles remain alive through the native
write and exact readback. Failure to obtain this access fails eligibility/write.
This corrects the attributes-only pin claim in `permissions-design.md`; that
document is outside this agent's edit scope. Cross-compilation verifies the API
binding, not live Windows sharing behavior. No file bytes are read or executed.

### P2: unsupported access masks could become compliant targets

Ordinary ALLOW ACEs with only unrecognized/reserved rights previously passed
`repair()` unchanged. The advisory scanner also returned its ordinary
no-dangerous-candidate result. That treated semantics the implementation does not
evaluate as a supported safe state. Mixed dangerous/unsupported masks could also
be partially repaired and subsequently treated as compliant.

Repair now requires every ordinary ACE's mask, including unrelated principals,
to contain only the documented service-specific rights, standard service rights,
and four generic rights (`0xf00f01ff`). Other bits remain observable but make
repair/restore ineligible. Advisory scanning marks them complex, yielding
`unknown` when no dangerous candidate exists. `MAXIMUM_ALLOWED`,
`ACCESS_SYSTEM_SECURITY`, `SYNCHRONIZE` (unsupported by service objects), and
reserved bits are conservatively outside this transform's supported semantics.
Regression tests enumerate every unsupported bit, alone and mixed with
CHANGE_CONFIG, for broad and administrator principals.

### P2: advisory parser accepted malformed empty/unrestricted headers

The advisory parser ignored the security descriptor's reserved byte, ACL
reserved fields, and a nonzero DACL offset without DACL_PRESENT. Malformed input
could therefore be reported as an ordinary empty DACL or an unrestricted one.
It also lacked the pure snapshot parser's size bound.

The advisory parser now rejects those malformed headers and oversized input,
and treats unsupported descriptor-control semantics as complex. Dedicated
regressions cover these cases. This affected advisory classification; the
canonical writable snapshot parser already rejected these malformed inputs.

### Maintenance: owned Clippy warnings

Replaced manual divisibility checks with `is_multiple_of` in the owned parsers
and native string-pointer validation. Host and Windows-target strict Clippy
checks passed at review time.

## Security properties examined

- **Fixed targets:** only exact `permissions.service.bits` and
  `permissions.service.wuauserv` IDs select native repair objects. No serialized
  service name, executable path, SDDL, remote host, or arbitrary restore target
  is accepted. The catalog sentinel is not a state or a write value.
- **Exact transform:** remove CHANGE_CONFIG, DELETE, WRITE_DAC and WRITE_OWNER
  only from Everyone, Authenticated Users and Builtin Users explicit ordinary
  ALLOW ACEs. Changed generic grants are mapped with service-specific mappings;
  safe rights survive. Unrelated ACEs, zero-mask ACEs, duplicate ACEs, owner,
  group, supported control flags, order and ACL padding survive byte-for-byte.
- **No collapsed safe state:** every snapshot contains its own full owner/group
  and DACL bytes. Distinct safe-right masks and reordered ACEs are distinct
  states. Tests reject unrelated-principal substitutions and safe-to-safe drift.
- **Owner identity:** exact SYSTEM, Builtin Administrators or TrustedInstaller
  SIDs are required. New tests cover all three and reject same-RID/domain,
  different-authority, prefix and near-matching TrustedInstaller SIDs. Group
  identity is preserved exactly, not replaced with a default group.
- **Conservative ACL semantics:** deny, inherited, inherit-only, other flagged,
  object, callback, conditional and unknown ACEs cannot become repair or restore
  payloads. NULL and absent DACLs are ineligible; an actually empty DACL is
  distinct and can be unchanged/compliant for this narrowly defined control.
- **Serialized data:** bounded lowercase hex, revision/control validation,
  canonical offsets, section disjointness, exact SID lengths, ACL/ACE bounds and
  roundtrip equality prevent offsets, gaps, suffixes, truncation and overlapping
  sections from becoming write payloads. Unsupported ACE records are retained
  only as bounded opaque records for observation; validation is not a semantic
  validator for unknown ACE layouts. The transform rejects them before native
  writes, including zero-length-payload opaque records.
- **Native memory/API review:** service descriptors use initialized, u32-aligned
  8-KiB storage; configuration uses initialized, u64-aligned storage. String
  pointers are checked against their buffer and decoded within its bounds.
  Native SID pointers originate from successful GetSecurityInfo, are checked
  with IsValidSid and bounded GetLengthSid, and remain within the lifetime of
  the LocalFree-managed descriptor. Service handles are RAII-managed with
  CloseServiceHandle, file handles with File. No caller-provided pointer is
  passed into these APIs.
- **DACL-only mutation:** SetServiceObjectSecurity receives only
  DACL_SECURITY_INFORMATION (`4`). No OWNER/GROUP/SACL or protection-changing
  security-information flags are requested. Owner/group/control equality is
  required before mutation and in the complete post-write snapshot.
- **Gates and races:** write repeats the platform gate, host identity checks and
  exact current snapshot immediately before mutation. Exact readback mismatch is
  an error, not approximate success or an invitation to overwrite again.
- **Advisory wording:** dangerous ALLOWs are candidates, never AccessCheck results
  or proof of exploitability. A preceding deny still sets complex and retains a
  candidate rather than asserting usable rights. Unsupported semantics without a
  candidate yield unknown; a supported no-candidate result explicitly limits its
  scope to the three broad principals.

## Restore authorization boundary - important residual contract

The native transition rule is `repair(current) == desired` or
`repair(desired) == current`, with identical owner/group/control fields. This
prevents unrelated changes but **does not authenticate a historical before
image**. Repair is many-to-one: two distinct dangerous masks can produce the
same safe mask. A new regression explicitly demonstrates this fact alongside
rejection of unrelated/safe-state substitutions.

The privileged `write(id, value)` function must therefore remain behind the
engine's protected durable journal; it is not a safe untrusted restore API.
API and transition comments now state this requirement. The inspected engine
derives targets from recorded before-images, checks exact current state, and
validates fixed IDs and snapshots during WAL loading. Existing engine tests
cover replay, malformed journals, and exact-state conflicts. No public path/name
selected restoration was found in this module. A hostile administrator or
compromised protected journal is not made trustworthy by the inverse-transform
check. Authenticating arbitrary low-level callers would require a different
API/capability boundary, not a stronger byte parser.

## Residual support and verification limitations

1. **No service-DACL compare-and-swap:** another actor may change a DACL, owner,
   or service configuration after the final checks. Exact readback detects many
   resulting mismatches but does not provide atomic preservation. A held service
   handle binds one operation to that object; identical delete/recreate across
   restarts is not distinguishable by this snapshot format.
2. **Identity screening is limited:** fixed svchost path/arguments, LocalSystem,
   shared-process type, regular/non-reparse/nonempty metadata and a trusted file
   owner are not Authenticode, file-DACL, service-DLL, registry, ancestor-directory
   or effective-token integrity verification. Trusted ownership alone does not
   prove a file is not writable by another principal. Ancestor renames/reparse
   changes and service configuration races are not solved by the leaf handle.
3. **Strict compatibility:** legitimate variants of command line, account,
   service type, owner, descriptor flags or complex ACLs are ineligible. Owner
   or group defaulted flags, for example, are outside the snapshot flag subset.
   These cases require review, not a fabricated safe descriptor.
4. **No effective-access guarantee:** other principals, privileges, owner-based
   access, registry/DLL/file ACLs and arbitrary third-party services are outside
   the repair. The other three audited services remain advisory only.
5. **Windows normalization:** preserving flags, ACE order and padding is checked
   by exact readback. Native normalization or DACL-defaulted-bit changes could
   yield a visible post-mutation error. This review did not execute Windows
   mutation/readback, sharing-conflict, or crash/revert tests on a guest.

## Verification

Commands use `source /tmp/opencode/secblitz-cross-env.sh` first:

- `cargo test --lib` - **68 passed, 0 failed**, including five new permissions
  regression tests covering the findings and exact-state/owner boundaries.
- `cargo check --target x86_64-pc-windows-gnu --all-targets` - native Windows
  code and Windows-only test compilation; **passed**.
- `cargo clippy --lib --tests -- -D warnings` - host strict lint check; **passed**.
- `cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings`
  - Windows strict lint check; **passed**.

These are host-side checks only; they do not establish Windows runtime behavior.

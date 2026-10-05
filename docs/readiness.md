# Read-only readiness facts

`secblitz::readiness::collect() -> Readiness` collects independent facts for the
0.5 automatic feature. It makes no readiness decision and supplies no UI labels.
Types are re-exported from the shared `model` module, with no duplicate model.
All types derive `Serialize` and `Deserialize`; `Readiness` defaults to four
unknown probes. `Probe<T>` defaults to its unit variant `Unknown`.

```json
{"status":"known","value":{"available_bytes":1099511627899,"read_only":false}}
```

An unknown probe serializes as `{"status":"unknown"}`. A known reboot result of
false is `{"status":"known","value":false}`. Byte counts remain `u64`, including
values above 4 GiB. Consumers must not interpret unknown as false or zero.

## Sources and behavior

- **System volume:** `GetWindowsDirectoryW`, not an assumed C drive.
- **Journal volume:** native `FOLDERID_ProgramData` through
  `SHGetKnownFolderPath(KF_FLAG_DONT_VERIFY)`, with `Secblitz` appended in UTF-16.
  No environment expansion, directory creation or journal access is involved.
  `GetVolumePathNameW` resolves the containing volume for the future path even
  before the final directory exists, and honors existing mounted volumes.
- **Both volumes:** reject malformed, relative, UNC and device paths. Require a
  fixed local drive before resolution and for the resolved volume. Read only
  the caller/quota-aware available-byte output of `GetDiskFreeSpaceExW`, and the
  flags output of `GetVolumeInformationW`. Read-only reflects
  `FILE_READ_ONLY_VOLUME`; it is not an ACL/write-access test.
- **Power:** native `GetSystemPowerStatus` using the generated C-layout ABI.
  AC 0/1 maps to false/true; other values are unknown. Battery flag 255 is
  checked before 128. Flag 128 means absent and suppresses the percentage;
  reserved or contradictory absence/status masks are unknown. Percentages
  above 100, including 255, are unknown. A percentage is emitted only when
  battery presence is known true.
- **Windows Update reboot:** a local in-process `Microsoft.Update.SystemInfo`
  object, standard `IDispatch`, and only `RebootRequired` property-get. The
  registered machine CLSID is read with `RegGetValueW` and parsed with
  `CLSIDFromString`. `CLSIDFromProgID` is deliberately avoided because it may
  create a registration for an absent ProgID. No guessed class GUID is used.
  Only native `VT_BOOL` with canonical false/true values is accepted.

Each failed probe becomes `Unknown` without suppressing other fields. A power
API success can still contain unknown individual values. Non-Windows platforms
return the all-unknown default without native calls.

## COM lifetime and wait bound

One worker owns its apartment, dispatch reference, VARIANT and exception
strings. Successful `CoInitializeEx`, including `S_FALSE`, is balanced by
`CoUninitialize`. `RPC_E_CHANGED_MODE` does not claim ownership of an existing
apartment. Object and output cleanup occurs on the same worker before it sends
the result.

The caller waits at most two seconds for a newly started COM query. A concurrent
collector or a still-pending worker returns unknown immediately. Timeout does
not terminate the thread or release COM objects from another thread. At most
one stalled worker is retained process-wide, so repeated checks cannot grow
the number of hung COM workers. Late results are discarded and a subsequent
check starts a fresh query once the old worker has completed. Other native
Windows calls do not have a universal hard time bound; this is not a two-second
deadline for the entire collector.

## Read-only boundary

The implementation has no filesystem or registry mutation calls, no state-dir
creation, no journal writes, no PowerShell or child-process runner, no update
searcher, and no network/update-search request. Native COM activation may load
the installed Windows component. Probe output contains no paths, labels,
serial numbers, exception text or other identifying diagnostics.

These are advisory snapshots, not authorization to write. The native state-dir
security checks must separately veto reparse points and validate the actual
tree before later writes; this collector does not replace those checks.

## Verification

Portable tests cover failure isolation with injected probe functions, serde
round trips and counts above 4 GiB, unsupported platforms, UTF-16 path checks,
power sentinels and invalid masks. Portable fake volume API tests cover
native-call failure stages, remote-drive rejection and large counts.
Windows-only tests check native structure sizes. No test writes machine state or runs a real COM
query. Cross-compilation checks the Windows FFI; Windows-only tests require a
Windows test runner to execute.

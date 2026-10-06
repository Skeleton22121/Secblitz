//! Launcher <-> elevated GUI broker: a closed set of user-context actions.
//!
//! Protocol (spec 3.1): request `[kind, arg_lo, arg_hi]`, response `[status]`.
//! No strings cross the boundary. The launcher (standard user) serves the
//! pipe; the elevated GUI is the only client.

use crate::user_apps;
use crate::user_settings::{Op, Setting};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    OpenWindowsUpdate,
    OpenWindowsSecurity,
    OpenEncryption,
    OpenSignIn,
    InstallBitwarden,
    /// Turn off silent sponsored-app installs for the signed-in user (HKCU).
    BlockSuggestedApps,
    /// Reinstall a removed app; arg = index into `secblitz::debloat::catalog()`.
    ReinstallStoreApp(u16),
    /// Start a Store download for "put everything back" and answer at once.
    StartStoreApp(u16),
    /// How that download is doing: `Working`, `Done`, `Failed` or `Offline`.
    StoreAppStatus(u16),
    OpenTamperProtection,
    OpenProtectionHistory,
    /// Windows Security > Virus and threat protection > Protection history.
    OpenProtectionHistoryList,
    /// Settings > Network and internet.
    OpenNetwork,
    OpenAppBrowserControl,
    OpenOptionalFeatures,
    OpenAccounts,
    /// Windows Security > Device security > Core isolation.
    OpenCoreIsolation,
    /// Windows Security > Firewall and network protection.
    OpenFirewall,
    /// Windows Security > Device security.
    OpenDeviceSecurity,
    /// Settings > Accounts > Access work or school.
    OpenWorkAccounts,
    /// Settings > System > Recovery (Advanced startup).
    OpenRecovery,
    /// Settings > System > Remote Desktop.
    OpenRemoteDesktop,
    /// Settings > Privacy and security > Find my device.
    OpenFindMyDevice,
    /// The classic BitLocker page (editions without device encryption).
    OpenBitLocker,
    /// Settings > Network and internet > Wi-Fi.
    OpenWifi,
    /// Read, apply or undo one per-user (HKCU) setting; see `user_settings`.
    UserSetting(Setting, Op),
    /// Run `winget upgrade` once as the signed-in user and remember which
    /// allowlisted programs have a newer version.
    AppUpdatesScan,
    /// Ask about one allowlisted program (index into `user_apps::APPS`) from
    /// the last scan.
    AppUpdateQuery(u16),
    /// Upgrade one allowlisted program (index into `user_apps::APPS`).
    AppUpdate(u16),
    /// Is Bitwarden already installed for the signed-in user? Read-only.
    BitwardenStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Reply {
    Done,
    Failed,
    /// Restore fell back to opening the Store page for the user.
    OpenedStore,
    Unavailable,
    /// No internet connection (install could not download). Nothing was opened.
    Offline,
    /// A per-user setting is already in its safer state.
    Safe,
    /// ... and Secblitz made it so: undo is available.
    SafeByUs,
    /// A per-user setting is in its weaker state.
    NeedsAttention,
    /// Nothing to do on this PC (not installed, not applicable).
    NotApplicable,
    /// Could not be read, or the answer was not trustworthy.
    Unknown,
    /// A newer version of the program is available.
    UpdateAvailable,
    /// Still running (a Store download).
    Working,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Reply {
    /// The GUI's answer for a per-user setting request.
    pub fn from_result(result: crate::user_settings::HandleResult) -> Self {
        use crate::user_settings::{HandleResult, Outcome, Report};
        match result {
            HandleResult::Report(Report::Safe) => Reply::Safe,
            HandleResult::Report(Report::SafeByUs) => Reply::SafeByUs,
            HandleResult::Report(Report::Unsafe) => Reply::NeedsAttention,
            HandleResult::Report(Report::NotApplicable) => Reply::NotApplicable,
            HandleResult::Report(Report::Unknown) => Reply::Unknown,
            HandleResult::Outcome(Outcome::Done) => Reply::Done,
            HandleResult::Outcome(Outcome::Failed) => Reply::Failed,
            HandleResult::Outcome(Outcome::Blocked) => Reply::Unavailable,
        }
    }

    pub fn encode(self) -> u8 {
        match self {
            Reply::Done => 1,
            Reply::Failed => 2,
            Reply::OpenedStore => 3,
            Reply::Unavailable => 4,
            Reply::Offline => 5,
            Reply::Safe => 6,
            Reply::SafeByUs => 7,
            Reply::NeedsAttention => 8,
            Reply::NotApplicable => 9,
            Reply::Unknown => 10,
            Reply::UpdateAvailable => 11,
            Reply::Working => 12,
        }
    }
    pub fn decode(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Reply::Done,
            2 => Reply::Failed,
            3 => Reply::OpenedStore,
            4 => Reply::Unavailable,
            5 => Reply::Offline,
            6 => Reply::Safe,
            7 => Reply::SafeByUs,
            8 => Reply::NeedsAttention,
            9 => Reply::NotApplicable,
            10 => Reply::Unknown,
            11 => Reply::UpdateAvailable,
            12 => Reply::Working,
            _ => return None,
        })
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Request {
    pub fn encode(self) -> [u8; 3] {
        let (kind, arg) = match self {
            Request::OpenWindowsUpdate => (1, 0),
            Request::OpenWindowsSecurity => (2, 0),
            Request::OpenEncryption => (3, 0),
            Request::OpenSignIn => (4, 0),
            Request::InstallBitwarden => (5, 0),
            Request::BlockSuggestedApps => (6, 0),
            Request::ReinstallStoreApp(i) => (7, i),
            Request::OpenTamperProtection => (8, 0),
            Request::OpenProtectionHistory => (9, 0),
            Request::OpenAppBrowserControl => (10, 0),
            Request::OpenOptionalFeatures => (11, 0),
            Request::OpenAccounts => (12, 0),
            Request::OpenCoreIsolation => (20, 0),
            Request::OpenFirewall => (21, 0),
            Request::OpenDeviceSecurity => (22, 0),
            Request::OpenWorkAccounts => (23, 0),
            Request::OpenRecovery => (24, 0),
            Request::OpenRemoteDesktop => (25, 0),
            Request::OpenFindMyDevice => (26, 0),
            Request::OpenBitLocker => (27, 0),
            Request::OpenWifi => (28, 0),
            Request::OpenProtectionHistoryList => (29, 0),
            Request::OpenNetwork => (30, 0),
            // One byte for the setting, one for the operation.
            Request::UserSetting(setting, op) => (
                13,
                u16::from(setting.to_byte()) | (u16::from(op.to_byte()) << 8),
            ),
            Request::AppUpdatesScan => (14, 0),
            Request::AppUpdateQuery(i) => (15, i),
            Request::AppUpdate(i) => (16, i),
            Request::BitwardenStatus => (17, 0),
            Request::StartStoreApp(i) => (18, i),
            Request::StoreAppStatus(i) => (19, i),
        };
        let [lo, hi] = arg.to_le_bytes();
        [kind, lo, hi]
    }

    /// Strict decode against the compiled debloat catalog.
    pub fn decode(bytes: [u8; 3]) -> Option<Self> {
        Self::decode_with(bytes, secblitz::debloat::catalog().len())
    }

    /// Strict decode: unknown kinds, a non-zero argument on argument-less
    /// kinds and catalog indices out of range are all rejected.
    pub fn decode_with(bytes: [u8; 3], catalog_len: usize) -> Option<Self> {
        let [kind, lo, hi] = bytes;
        let arg = u16::from_le_bytes([lo, hi]);
        if !matches!(kind, 7 | 13 | 15 | 16 | 18 | 19) && arg != 0 {
            return None;
        }
        let apps = user_apps::APPS.len();
        Some(match kind {
            1 => Request::OpenWindowsUpdate,
            2 => Request::OpenWindowsSecurity,
            3 => Request::OpenEncryption,
            4 => Request::OpenSignIn,
            5 => Request::InstallBitwarden,
            6 => Request::BlockSuggestedApps,
            7 if usize::from(arg) < catalog_len => Request::ReinstallStoreApp(arg),
            8 => Request::OpenTamperProtection,
            9 => Request::OpenProtectionHistory,
            10 => Request::OpenAppBrowserControl,
            11 => Request::OpenOptionalFeatures,
            12 => Request::OpenAccounts,
            20 => Request::OpenCoreIsolation,
            21 => Request::OpenFirewall,
            22 => Request::OpenDeviceSecurity,
            23 => Request::OpenWorkAccounts,
            24 => Request::OpenRecovery,
            25 => Request::OpenRemoteDesktop,
            26 => Request::OpenFindMyDevice,
            27 => Request::OpenBitLocker,
            28 => Request::OpenWifi,
            29 => Request::OpenProtectionHistoryList,
            30 => Request::OpenNetwork,
            13 => Request::UserSetting(Setting::from_byte(lo)?, Op::from_byte(hi)?),
            14 => Request::AppUpdatesScan,
            15 if usize::from(arg) < apps => Request::AppUpdateQuery(arg),
            16 if usize::from(arg) < apps => Request::AppUpdate(arg),
            17 => Request::BitwardenStatus,
            18 if usize::from(arg) < catalog_len => Request::StartStoreApp(arg),
            19 if usize::from(arg) < catalog_len => Request::StoreAppStatus(arg),
            _ => return None,
        })
    }

    /// How long the GUI waits for the launcher's answer.
    pub fn timeout(self) -> Duration {
        match self {
            // winget can take minutes (download + install).
            Request::ReinstallStoreApp(_) | Request::InstallBitwarden | Request::AppUpdate(_) => {
                Duration::from_secs(15 * 60)
            }
            // WinGet may refresh its sources first.
            Request::AppUpdatesScan => Duration::from_secs(4 * 60),
            _ => Duration::from_secs(30),
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
const PREFIX: &str = r"\\.\pipe\secblitz-broker-";

/// The id is a 128-bit random value written as 32 lowercase hex digits.
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn new_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn pipe_name(id: &str) -> String {
    format!("{PREFIX}{id}")
}

/// Elevated-GUI side. Thread-safe: one request at a time.
pub struct Client {
    #[cfg(windows)]
    inner: std::sync::Mutex<imp::Pipe>,
    #[cfg(not(windows))]
    _private: (),
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("broker::Client")
    }
}

impl Client {
    /// Connect to `\\.\pipe\secblitz-broker-<id>`; `id` must be 32 lowercase hex.
    pub fn connect(id: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(valid_id(id), "invalid broker id");
        #[cfg(windows)]
        {
            Ok(Self {
                inner: std::sync::Mutex::new(imp::Pipe::connect(&pipe_name(id))?),
            })
        }
        #[cfg(not(windows))]
        {
            anyhow::bail!("The broker is only available on Windows")
        }
    }

    /// Blocking round trip. Call from a background task, never the UI thread.
    pub fn send(&self, request: Request) -> anyhow::Result<Reply> {
        #[cfg(windows)]
        {
            let mut pipe = self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("broker unavailable"))?;
            let byte = pipe.round_trip(request.encode(), request.timeout())?;
            Reply::decode(byte).ok_or_else(|| anyhow::anyhow!("invalid broker reply"))
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            anyhow::bail!("The broker is only available on Windows")
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::{ptr::null_mut, time::Duration};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_IO_PENDING, HANDLE, INVALID_HANDLE_VALUE,
            WAIT_OBJECT_0,
        },
        Storage::FileSystem::{
            CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Pipes::GetNamedPipeServerProcessId,
            Threading::{
                CreateEventW, GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW,
                WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
            },
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        },
    };

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    // SECURITY_SQOS_PRESENT with SECURITY_ANONYMOUS (0): the server never
    // gets to impersonate us.
    const SECURITY_SQOS_PRESENT: u32 = 0x0010_0000;

    pub struct Pipe {
        handle: HANDLE,
        event: HANDLE,
        /// A timeout or I/O error desynchronises the stream; stop using it.
        broken: bool,
    }
    // The raw handles are only used under the owning Mutex.
    unsafe impl Send for Pipe {}

    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
                CloseHandle(self.event);
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn parent_pid() -> Option<u32> {
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return None;
            }
            let me = GetCurrentProcessId();
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut found = None;
            let mut ok = Process32FirstW(snap, &mut entry);
            while ok != 0 {
                if entry.th32ProcessID == me {
                    found = Some(entry.th32ParentProcessID);
                    break;
                }
                ok = Process32NextW(snap, &mut entry);
            }
            CloseHandle(snap);
            found
        }
    }

    pub fn image_path(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    impl Pipe {
        pub fn connect(name: &str) -> anyhow::Result<Self> {
            let wide_name = wide(name);
            let handle = unsafe {
                CreateFileW(
                    wide_name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT,
                    null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error().into());
            }
            let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            if event.is_null() {
                unsafe { CloseHandle(handle) };
                return Err(std::io::Error::last_os_error().into());
            }
            let pipe = Pipe {
                handle,
                event,
                broken: false,
            };
            // The pipe name is secret and created with FIRST_PIPE_INSTANCE;
            // additionally require that the server is our launcher.
            let mut server = 0u32;
            anyhow::ensure!(
                unsafe { GetNamedPipeServerProcessId(pipe.handle, &mut server) } != 0,
                "broker server unknown"
            );
            let parent_ok = parent_pid() == Some(server);
            let same_image = match (image_path(server), std::env::current_exe().ok()) {
                (Some(theirs), Some(ours)) => theirs.eq_ignore_ascii_case(&ours.to_string_lossy()),
                _ => false,
            };
            anyhow::ensure!(parent_ok || same_image, "broker server is not the launcher");
            Ok(pipe)
        }

        fn io(&mut self, write: bool, buf: &mut [u8], timeout: Duration) -> anyhow::Result<()> {
            let mut done = 0usize;
            while done < buf.len() {
                let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
                overlapped.hEvent = self.event;
                let rest = &mut buf[done..];
                let started = unsafe {
                    if write {
                        WriteFile(
                            self.handle,
                            rest.as_ptr(),
                            rest.len() as u32,
                            null_mut(),
                            &mut overlapped,
                        )
                    } else {
                        ReadFile(
                            self.handle,
                            rest.as_mut_ptr(),
                            rest.len() as u32,
                            null_mut(),
                            &mut overlapped,
                        )
                    }
                };
                if started == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
                    return Err(std::io::Error::last_os_error().into());
                }
                let millis = timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32;
                if unsafe { WaitForSingleObject(self.event, millis) } != WAIT_OBJECT_0 {
                    unsafe {
                        CancelIoEx(self.handle, &overlapped);
                        let mut n = 0u32;
                        GetOverlappedResult(self.handle, &overlapped, &mut n, 1);
                    }
                    anyhow::bail!("The launcher did not answer in time");
                }
                let mut n = 0u32;
                if unsafe { GetOverlappedResult(self.handle, &overlapped, &mut n, 0) } == 0
                    || n == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                done += n as usize;
            }
            Ok(())
        }

        pub fn round_trip(
            &mut self,
            mut request: [u8; 3],
            timeout: Duration,
        ) -> anyhow::Result<u8> {
            anyhow::ensure!(!self.broken, "broker unavailable");
            let result = (|| {
                self.io(true, &mut request, Duration::from_secs(10))?;
                let mut reply = [0u8; 1];
                self.io(false, &mut reply, timeout)?;
                Ok(reply[0])
            })();
            if result.is_err() {
                self.broken = true;
            }
            result
        }
    }
}

#[cfg(windows)]
pub(crate) use imp::image_path;

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Request> {
        vec![
            Request::OpenWindowsUpdate,
            Request::OpenWindowsSecurity,
            Request::OpenEncryption,
            Request::OpenSignIn,
            Request::InstallBitwarden,
            Request::BlockSuggestedApps,
            Request::ReinstallStoreApp(0),
            Request::ReinstallStoreApp(41),
            Request::OpenTamperProtection,
            Request::OpenProtectionHistory,
            Request::OpenAppBrowserControl,
            Request::OpenOptionalFeatures,
            Request::OpenAccounts,
            Request::OpenCoreIsolation,
            Request::OpenFirewall,
            Request::OpenDeviceSecurity,
            Request::OpenWorkAccounts,
            Request::OpenRecovery,
            Request::OpenRemoteDesktop,
            Request::OpenFindMyDevice,
            Request::OpenBitLocker,
            Request::OpenWifi,
            Request::OpenProtectionHistoryList,
            Request::OpenNetwork,
            Request::AppUpdatesScan,
            Request::AppUpdateQuery(0),
            Request::AppUpdateQuery(user_apps::APPS.len() as u16 - 1),
            Request::AppUpdate(0),
            Request::AppUpdate(user_apps::APPS.len() as u16 - 1),
            Request::BitwardenStatus,
            Request::StartStoreApp(0),
            Request::StartStoreApp(41),
            Request::StoreAppStatus(0),
            Request::StoreAppStatus(41),
        ]
        .into_iter()
        .chain(Setting::ALL.iter().flat_map(|s| {
            [Op::Query, Op::Apply, Op::Undo]
                .into_iter()
                .map(|op| Request::UserSetting(*s, op))
        }))
        .collect()
    }

    #[test]
    fn requests_round_trip() {
        for request in all() {
            assert_eq!(Request::decode_with(request.encode(), 100), Some(request));
        }
    }

    #[test]
    fn decode_is_strict() {
        for kind in [0u8, 31, 32, 100, 255] {
            assert_eq!(Request::decode_with([kind, 0, 0], 100), None);
        }
        for kind in (1..=6u8).chain(8..=12).chain(20..=30).chain([17]) {
            assert_eq!(Request::decode_with([kind, 1, 0], 100), None);
            assert_eq!(Request::decode_with([kind, 0, 1], 100), None);
        }
        // User settings: unknown setting or operation bytes are rejected.
        assert!(Request::decode_with([13, 0, 0], 100).is_some());
        assert_eq!(
            Request::decode_with([13, Setting::ALL.len() as u8, 0], 100),
            None
        );
        assert_eq!(Request::decode_with([13, 0, 3], 100), None);
        // Wire byte 5 is the suggested-apps setting; the next one is unknown.
        assert_eq!(
            Request::decode_with([13, 5, 2], 100),
            Some(Request::UserSetting(Setting::SuggestedApps, Op::Undo))
        );
        assert_eq!(Request::decode_with([13, 6, 0], 100), None);
        assert_eq!(Request::decode_with([13, 255, 255], 100), None);
        assert!(Request::decode_with([13, 0, 0], 0).is_some());
        // Scan takes no argument.
        assert_eq!(Request::decode_with([14, 1, 0], 100), None);
        assert_eq!(Request::decode_with([14, 0, 1], 100), None);
        // App indices stay inside the allowlist.
        let n = user_apps::APPS.len() as u8;
        for kind in [15u8, 16] {
            assert!(Request::decode_with([kind, n - 1, 0], 100).is_some());
            assert_eq!(Request::decode_with([kind, n, 0], 100), None);
            assert_eq!(Request::decode_with([kind, 0, 1], 100), None);
            assert_eq!(Request::decode_with([kind, 255, 255], 100), None);
        }
        assert_eq!(Request::decode_with([7, 0, 0], 0), None);
        assert_eq!(Request::decode_with([7, 5, 0], 5), None);
        assert_eq!(
            Request::decode_with([7, 4, 0], 5),
            Some(Request::ReinstallStoreApp(4))
        );
        assert_eq!(Request::decode_with([7, 255, 255], 100), None);
        assert_eq!(Request::decode([7, 255, 255]), None);
        // Store downloads for "put everything back": catalog indices only.
        for kind in [18u8, 19] {
            assert!(Request::decode_with([kind, 4, 0], 5).is_some());
            assert_eq!(Request::decode_with([kind, 5, 0], 5), None);
            assert_eq!(Request::decode_with([kind, 255, 255], 100), None);
        }
    }

    #[test]
    fn replies_round_trip_and_reject_unknown() {
        for reply in [
            Reply::Done,
            Reply::Failed,
            Reply::OpenedStore,
            Reply::Unavailable,
            Reply::Offline,
            Reply::Safe,
            Reply::SafeByUs,
            Reply::NeedsAttention,
            Reply::NotApplicable,
            Reply::Unknown,
            Reply::UpdateAvailable,
            Reply::Working,
        ] {
            assert_eq!(Reply::decode(reply.encode()), Some(reply));
        }
        for byte in [0u8, 13, 14, 100, 255] {
            assert_eq!(Reply::decode(byte), None);
        }
    }

    #[test]
    fn setting_results_map_to_distinct_calm_replies() {
        use crate::user_settings::{HandleResult as H, Outcome, Report};
        assert_eq!(
            Reply::from_result(H::Report(Report::Unsafe)),
            Reply::NeedsAttention
        );
        assert_eq!(Reply::from_result(H::Report(Report::Safe)), Reply::Safe);
        assert_eq!(
            Reply::from_result(H::Report(Report::SafeByUs)),
            Reply::SafeByUs
        );
        assert_eq!(
            Reply::from_result(H::Report(Report::Unknown)),
            Reply::Unknown
        );
        assert_eq!(
            Reply::from_result(H::Outcome(Outcome::Blocked)),
            Reply::Unavailable
        );
        assert_eq!(Reply::from_result(H::Outcome(Outcome::Done)), Reply::Done);
    }

    #[test]
    fn ids_are_validated_and_random() {
        let id = new_id();
        assert!(valid_id(&id));
        assert_ne!(id, new_id());
        for bad in [
            "",
            "abc",
            &"A".repeat(32),
            &"g".repeat(32),
            &"a".repeat(31),
            &"a".repeat(33),
            "..\\..\\evil0000000000000000000000",
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
        assert!(Client::connect("../x").is_err());
        assert_eq!(pipe_name(&"a".repeat(32)).len(), PREFIX.len() + 32);
    }

    #[test]
    fn long_operations_get_long_timeouts() {
        assert!(Request::ReinstallStoreApp(0).timeout() >= Duration::from_secs(900));
        assert_eq!(Request::OpenSignIn.timeout(), Duration::from_secs(30));
        assert!(Request::AppUpdate(0).timeout() >= Duration::from_secs(900));
        assert!(Request::AppUpdatesScan.timeout() > Duration::from_secs(150));
        assert_eq!(
            Request::UserSetting(Setting::ShowExtensions, Op::Apply).timeout(),
            Duration::from_secs(30)
        );
    }
}

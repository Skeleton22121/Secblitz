//! Launcher <-> elevated GUI broker: a closed set of user-context actions.
//!
//! Protocol (spec 3.1): request `[kind, arg_lo, arg_hi]`, response `[status]`.
//! No strings cross the boundary. The launcher (standard user) serves the
//! pipe; the elevated GUI is the only client.

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Reply {
    Done,
    Failed,
    /// Restore fell back to opening the Store page for the user.
    OpenedStore,
    Unavailable,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Reply {
    pub fn encode(self) -> u8 {
        match self {
            Reply::Done => 1,
            Reply::Failed => 2,
            Reply::OpenedStore => 3,
            Reply::Unavailable => 4,
        }
    }
    pub fn decode(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Reply::Done,
            2 => Reply::Failed,
            3 => Reply::OpenedStore,
            4 => Reply::Unavailable,
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
        if kind != 7 && arg != 0 {
            return None;
        }
        Some(match kind {
            1 => Request::OpenWindowsUpdate,
            2 => Request::OpenWindowsSecurity,
            3 => Request::OpenEncryption,
            4 => Request::OpenSignIn,
            5 => Request::InstallBitwarden,
            6 => Request::BlockSuggestedApps,
            7 if usize::from(arg) < catalog_len => Request::ReinstallStoreApp(arg),
            _ => return None,
        })
    }

    /// How long the GUI waits for the launcher's answer.
    pub fn timeout(self) -> Duration {
        match self {
            // winget can take minutes (download + install).
            Request::ReinstallStoreApp(_) | Request::InstallBitwarden => {
                Duration::from_secs(15 * 60)
            }
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
        ]
    }

    #[test]
    fn requests_round_trip() {
        for request in all() {
            assert_eq!(Request::decode_with(request.encode(), 100), Some(request));
        }
    }

    #[test]
    fn decode_is_strict() {
        for kind in [0u8, 8, 9, 100, 255] {
            assert_eq!(Request::decode_with([kind, 0, 0], 100), None);
        }
        for kind in 1..=6u8 {
            assert_eq!(Request::decode_with([kind, 1, 0], 100), None);
            assert_eq!(Request::decode_with([kind, 0, 1], 100), None);
        }
        assert_eq!(Request::decode_with([7, 0, 0], 0), None);
        assert_eq!(Request::decode_with([7, 5, 0], 5), None);
        assert_eq!(
            Request::decode_with([7, 4, 0], 5),
            Some(Request::ReinstallStoreApp(4))
        );
        assert_eq!(Request::decode_with([7, 255, 255], 100), None);
        assert_eq!(Request::decode([7, 255, 255]), None);
    }

    #[test]
    fn replies_round_trip_and_reject_unknown() {
        for reply in [
            Reply::Done,
            Reply::Failed,
            Reply::OpenedStore,
            Reply::Unavailable,
        ] {
            assert_eq!(Reply::decode(reply.encode()), Some(reply));
        }
        for byte in [0u8, 5, 255] {
            assert_eq!(Reply::decode(byte), None);
        }
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
    }
}

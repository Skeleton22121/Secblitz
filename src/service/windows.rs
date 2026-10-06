use anyhow::{bail, ensure, Context, Result};
use std::{
    ffi::{c_void, OsStr, OsString},
    fs::File,
    io::{Seek, SeekFrom, Write},
    mem::{size_of, zeroed},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath},
};

use crate::platform::security::{descriptor, error, sid, wide, wide_str, Local};

const NAME: &str = "SecblitzMonitor";
const ACCOUNT: &str = r"NT AUTHORITY\LocalService";
const INTERVAL: Duration = Duration::from_secs(15 * 60);
const BUDGET: Duration = Duration::from_secs(5 * 60);
const REPORT_LIMIT: usize = 64 * 1024;
const RX: u32 = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
const RW: u32 = FILE_GENERIC_READ | FILE_GENERIC_WRITE;
const DIRECTORY_SD: &str = "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;LS)";
const APP_DIRECTORY_SD: &str =
    "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;LS)(A;OICI;0x1200a9;;;BU)";
const BINARY_SD: &str = "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;LS)(A;;0x1200a9;;;BU)";
// Status directory: LocalService may modify (the monitor writes `status.json`),
// Users may only list/read it (the unelevated tray). 0x1301bf = modify.
const STATUS_LS_RIGHTS: u32 = 0x1301bf;
const STATUS_DIRECTORY_SD: &str =
    "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;LS)(A;OICI;0x1200a9;;;BU)";
const REPORT_SD: &str = "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x12019f;;;LS)";
const SERVICE_SD: &str =
    "O:BAG:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;CCLCSWLOCRRC;;;LS)(A;;CCLCSWLOCRRC;;;BU)";

#[link(name = "ole32")]
extern "system" {
    fn CoTaskMemFree(p: *const c_void);
}
#[link(name = "advapi32")]
extern "system" {
    fn SetServiceObjectSecurity(
        service: *mut c_void,
        information: u32,
        descriptor: *const c_void,
    ) -> i32;
    fn ChangeServiceConfigW(
        service: *mut c_void,
        kind: u32,
        start: u32,
        error: u32,
        binary: *const u16,
        group: *const u16,
        tag: *mut u32,
        dependencies: *const u16,
        account: *const u16,
        password: *const u16,
        display: *const u16,
    ) -> i32;
    fn ChangeServiceConfig2W(service: *mut c_void, level: u32, info: *const c_void) -> i32;
}

fn attributes(sd: &Local) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    }
}
fn path_text(path: &Path) -> Result<&str> {
    let s = path.to_str().context("Non-Unicode Windows path")?;
    super::validate_path(s)?;
    Ok(s)
}
fn base() -> Result<PathBuf> {
    let mut raw = null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, null_mut(), &mut raw) };
    ensure!(
        hr >= 0 && !raw.is_null(),
        "Cannot resolve Program Files ({hr:#x})"
    );
    let result = (|| {
        // SAFETY: success returns a NUL-terminated UTF-16 string, freed only below.
        let text = unsafe { wide_str(raw) }.context("Invalid known-folder path")?;
        let p = PathBuf::from(String::from_utf16(text)?);
        path_text(&p)?;
        Ok(p)
    })();
    unsafe {
        CoTaskMemFree(raw.cast());
    }
    result
}
fn open(path: &Path, access: u32, creation: u32, sd: Option<&Local>) -> Result<File> {
    let path = wide(path)?;
    let sa = sd.map(attributes);
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ,
            sa.as_ref().map_or(null(), |s| s),
            creation,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(error());
    }
    Ok(unsafe { File::from_raw_handle(h) })
}
fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    // SAFETY: plain C struct for which all-zero bytes are a valid initial value.
    let mut i = unsafe { zeroed() };
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut i) } != 0,
        "File information: {}",
        error()
    );
    Ok(i)
}

// Parent directories may grant creation of unrelated children (the default C:\
// ACL does). They must not grant deletion/replacement of existing children or
// alteration of the parent. Every ancestor is pinned without delete sharing.
// Inherit-only ACEs cannot affect an ancestor; new objects use protected DACLs.
fn inspect(
    file: &File,
    directory: bool,
    protected: bool,
    report: bool,
    shared: bool,
) -> Result<()> {
    let i = info(file)?;
    ensure!(
        i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "Reparse point rejected"
    );
    ensure!(
        (i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) == directory,
        "Wrong object type"
    );
    ensure!(
        directory || i.nNumberOfLinks == 1,
        "Hard-linked file rejected"
    );
    unsafe {
        let mut owner = null_mut();
        let mut acl = null_mut();
        let mut sd = null_mut();
        let rc = GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut sd,
        );
        ensure!(rc == 0, "Cannot inspect file ACL ({rc})");
        let _sd = Local(sd);
        inspect_descriptor(sd, directory, protected, report, shared)
    }
}

fn inspect_descriptor(
    sd: *mut c_void,
    directory: bool,
    protected: bool,
    report: bool,
    shared: bool,
) -> Result<()> {
    unsafe {
        let mut owner = null_mut();
        let mut acl = null_mut();
        let mut defaulted = 0;
        let mut present = 0;
        ensure!(
            GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted) != 0,
            "Cannot inspect owner"
        );
        ensure!(
            GetSecurityDescriptorDacl(sd, &mut present, &mut acl, &mut defaulted) != 0
                && present != 0,
            "Cannot inspect DACL"
        );
        let system = sid("S-1-5-18")?;
        let admins = sid("S-1-5-32-544")?;
        let installer = sid("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")?;
        let local_service = sid("S-1-5-19")?;
        let users = sid("S-1-5-32-545")?;
        let privileged = |s| {
            EqualSid(s, system.0) != 0
                || EqualSid(s, admins.0) != 0
                || (!protected && EqualSid(s, installer.0) != 0)
        };
        ensure!(
            !owner.is_null() && IsValidSid(owner) != 0 && privileged(owner),
            "Untrusted file owner"
        );
        ensure!(
            !acl.is_null() && IsValidAcl(acl) != 0,
            "Missing/invalid DACL"
        );
        let mut control = 0;
        let mut revision = 0;
        ensure!(
            GetSecurityDescriptorControl(sd, &mut control, &mut revision) != 0,
            "Cannot inspect DACL control"
        );
        ensure!(
            control & SE_DACL_PRESENT != 0 && (!protected || control & SE_DACL_PROTECTED != 0),
            "Unprotected DACL"
        );
        let mut seen = [false; 3];
        for index in 0..(*acl).AceCount as u32 {
            let mut ace = null_mut();
            ensure!(GetAce(acl, index, &mut ace) != 0, "Cannot inspect ACE");
            let header = &*(ace as *const ACE_HEADER);
            ensure!(
                header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>(),
                "Short ACE"
            );
            ensure!(header.AceType <= 1, "Unsupported ACL entry");
            if !protected && header.AceFlags & INHERIT_ONLY_ACE as u8 != 0 {
                continue;
            }
            if !protected && header.AceType == 1 {
                continue;
            } // deny can only restrict
            ensure!(
                header.AceType == 0
                    && header.AceFlags
                        & !(OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE) as u8
                        == 0,
                "Unexpected ACE flags/type"
            );
            let a = &*(ace as *const ACCESS_ALLOWED_ACE);
            let trustee = &a.SidStart as *const u32 as *mut c_void;
            let sid_bytes = header.AceSize as usize - 8;
            ensure!(
                sid_bytes >= 8
                    && 8 + *(trustee.cast::<u8>().add(1)) as usize * 4 <= sid_bytes
                    && IsValidSid(trustee) != 0,
                "Invalid trustee SID"
            );
            if protected {
                let slot = if EqualSid(trustee, system.0) != 0 {
                    0
                } else if EqualSid(trustee, admins.0) != 0 {
                    1
                } else if EqualSid(trustee, local_service.0) != 0 {
                    2
                } else if shared && !report && EqualSid(trustee, users.0) != 0 {
                    // UI launch only; never permit Users on Monitor or its report.
                    3
                } else {
                    bail!("Unexpected protected-object trustee");
                };
                ensure!(
                    a.Mask
                        == if slot < 2 {
                            FILE_ALL_ACCESS
                        } else if report {
                            RW
                        } else {
                            RX
                        },
                    "Unexpected protected-object rights"
                );
                if directory {
                    ensure!(header.AceFlags & 3 == 3, "Missing ACL propagation");
                }
                if slot < seen.len() {
                    seen[slot] = true;
                }
            } else if !privileged(trustee) {
                let benign = FILE_GENERIC_READ
                    | FILE_GENERIC_EXECUTE
                    | FILE_ADD_FILE
                    | FILE_ADD_SUBDIRECTORY
                    | GENERIC_READ
                    | GENERIC_EXECUTE;
                ensure!(a.Mask & !benign == 0, "Writable/untrusted ancestor DACL");
            }
        }
        ensure!(
            !protected || seen.iter().all(|x| *x),
            "Missing protected-object trustees"
        );
    }
    Ok(())
}

// Deliberate narrow exception to the Monitor-only LocalService write boundary.
fn inspect_status_descriptor(sd: *mut c_void) -> Result<()> {
    unsafe {
        let mut owner = null_mut();
        let mut acl = null_mut();
        let mut defaulted = 0;
        let mut present = 0;
        ensure!(
            GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted) != 0,
            "Cannot inspect owner"
        );
        ensure!(
            GetSecurityDescriptorDacl(sd, &mut present, &mut acl, &mut defaulted) != 0
                && present != 0,
            "Cannot inspect DACL"
        );
        let system = sid("S-1-5-18")?;
        let admins = sid("S-1-5-32-544")?;
        let local_service = sid("S-1-5-19")?;
        let users = sid("S-1-5-32-545")?;
        ensure!(
            !owner.is_null()
                && IsValidSid(owner) != 0
                && (EqualSid(owner, system.0) != 0 || EqualSid(owner, admins.0) != 0),
            "StatusOwnerUntrusted"
        );
        ensure!(
            !acl.is_null() && IsValidAcl(acl) != 0,
            "Missing/invalid DACL"
        );
        let mut control = 0;
        let mut revision = 0;
        ensure!(
            GetSecurityDescriptorControl(sd, &mut control, &mut revision) != 0
                && control & SE_DACL_PROTECTED != 0,
            "StatusDaclUnprotected"
        );
        let mut seen = [false; 4];
        for index in 0..(*acl).AceCount as u32 {
            let mut ace = null_mut();
            ensure!(GetAce(acl, index, &mut ace) != 0, "Cannot inspect ACE");
            let header = &*(ace as *const ACE_HEADER);
            ensure!(
                header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>() && header.AceType == 0,
                "StatusAclEntryUnsupported"
            );
            ensure!(header.AceFlags & 3 == 3, "Missing ACL propagation");
            ensure!(
                header.AceFlags
                    & !(OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE) as u8
                    == 0,
                "StatusAceFlagsUnexpected"
            );
            let a = &*(ace as *const ACCESS_ALLOWED_ACE);
            let trustee = &a.SidStart as *const u32 as *mut c_void;
            let sid_bytes = header.AceSize as usize - 8;
            ensure!(
                sid_bytes >= 8
                    && 8 + *(trustee.cast::<u8>().add(1)) as usize * 4 <= sid_bytes
                    && IsValidSid(trustee) != 0,
                "Invalid trustee SID"
            );
            let (slot, mask) = if EqualSid(trustee, system.0) != 0 {
                (0, FILE_ALL_ACCESS)
            } else if EqualSid(trustee, admins.0) != 0 {
                (1, FILE_ALL_ACCESS)
            } else if EqualSid(trustee, local_service.0) != 0 {
                (2, STATUS_LS_RIGHTS)
            } else if EqualSid(trustee, users.0) != 0 {
                (3, RX)
            } else {
                bail!("StatusTrusteeUnexpected");
            };
            ensure!(a.Mask == mask, "StatusRightsUnexpected");
            seen[slot] = true;
        }
        ensure!(seen.iter().all(|x| *x), "StatusTrusteesMissing");
    }
    Ok(())
}

struct Layout {
    root: PathBuf,
    held: Vec<File>,
}
impl Layout {
    fn parents() -> Result<Self> {
        let base = base()?;
        let text = path_text(&base)?;
        let root = wide(&text[..3])?;
        ensure!(
            unsafe { GetDriveTypeW(root.as_ptr()) } == 3,
            "A fixed local drive is required"
        );
        let mut held = Vec::new();
        let mut p = PathBuf::from(&text[..3]);
        // Metadata-only opens do not enforce share-delete restrictions on Windows.
        let f = open(
            &p,
            READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
            OPEN_EXISTING,
            None,
        )?;
        inspect(&f, true, false, false, false)?;
        held.push(f);
        for part in text[3..].split('\\').filter(|p| !p.is_empty()) {
            p.push(part);
            let f = open(
                &p,
                READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
                OPEN_EXISTING,
                None,
            )?;
            inspect(&f, true, false, false, false)
                .with_context(|| format!("Untrusted ancestor {}", p.display()))?;
            held.push(f);
        }
        Ok(Self {
            root: base.join("Secblitz"),
            held,
        })
    }
    fn directory(&mut self, path: &Path, create: bool, created: &mut Vec<PathBuf>) -> Result<()> {
        if create {
            let sd = descriptor(if path == self.root {
                APP_DIRECTORY_SD
            } else {
                DIRECTORY_SD
            })?;
            let sa = attributes(&sd);
            let p = wide(path)?;
            if unsafe { CreateDirectoryW(p.as_ptr(), &sa) } == 0 {
                let code = unsafe { GetLastError() };
                ensure!(
                    code == ERROR_ALREADY_EXISTS,
                    "Create directory failed ({code})"
                );
            } else {
                created.push(path.to_owned());
            }
        }
        let f = open(
            path,
            READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
            OPEN_EXISTING,
            None,
        )?;
        inspect(&f, true, true, false, path == self.root)?;
        self.held.push(f);
        Ok(())
    }
    fn status_directory(
        &mut self,
        path: &Path,
        create: bool,
        created: &mut Vec<PathBuf>,
    ) -> Result<()> {
        if create {
            let sd = descriptor(STATUS_DIRECTORY_SD)?;
            let sa = attributes(&sd);
            let p = wide(path)?;
            if unsafe { CreateDirectoryW(p.as_ptr(), &sa) } == 0 {
                let code = unsafe { GetLastError() };
                ensure!(
                    code == ERROR_ALREADY_EXISTS,
                    "Create directory failed ({code})"
                );
            } else {
                created.push(path.to_owned());
            }
        }
        let f = open(
            path,
            READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
            OPEN_EXISTING,
            None,
        )?;
        let i = info(&f)?;
        ensure!(
            i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                && i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
            "StatusDirectoryInvalid"
        );
        unsafe {
            let mut sd = null_mut();
            let rc = GetSecurityInfo(
                f.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut sd,
            );
            ensure!(rc == 0, "StatusAclUnreadable({rc})");
            let _sd = Local(sd);
            inspect_status_descriptor(sd)?;
        }
        self.held.push(f);
        Ok(())
    }
}

/// `<Program Files>\Secblitz\Status` when the running executable is the
/// installed one; `None` for portable/dev copies.
pub fn trusted_status_dir() -> Option<PathBuf> {
    let root = base().ok()?.join("Secblitz");
    let exe = std::env::current_exe().ok()?;
    let parent = exe.parent()?;
    parent
        .to_string_lossy()
        .eq_ignore_ascii_case(&root.to_string_lossy())
        .then(|| root.join("Status"))
}

pub fn ensure_status_dir() -> Result<PathBuf> {
    let mut layout = Layout::parents()?;
    let root = layout.root.clone();
    layout.directory(&root, false, &mut Vec::new())?;
    let status = root.join("Status");
    layout.status_directory(&status, true, &mut Vec::new())?;
    Ok(status)
}

fn manager(access: ServiceManagerAccess) -> Result<ServiceManager> {
    Ok(ServiceManager::local_computer(None::<&str>, access)?)
}
fn absent(e: &windows_service::Error) -> bool {
    matches!(e, windows_service::Error::Winapi(e) if e.raw_os_error() == Some(1060))
}
fn command(path: &Path) -> Result<String> {
    Ok(format!("\"{}\" service run", path_text(path)?))
}

pub fn install() -> Result<()> {
    crate::platform::require_admin("Service installation requires Administrator elevation")?;
    ensure!(
        cfg!(target_arch = "x86_64"),
        "The monitor requires Windows x64"
    );
    // Only the protected installed copy may become the service binary: a
    // copy in a user-writable folder could be swapped before it is read.
    ensure!(
        trusted_status_dir().is_some(),
        "The background check needs Secblitz installed with its setup program"
    );
    let scm = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    match scm.open_service(NAME, ServiceAccess::QUERY_CONFIG) {
        Ok(_) => bail!("SecblitzMonitor already exists; it was not changed"),
        Err(e) if absent(&e) => (),
        Err(e) => return Err(e.into()),
    }
    let mut created_files = Vec::new();
    let mut created_dirs = Vec::new();
    let mut registration = None;
    let result = (|| -> Result<()> {
        let mut layout = Layout::parents()?;
        let root = layout.root.clone();
        layout.directory(&root, true, &mut created_dirs)?;
        let monitor = root.join("Monitor");
        layout.directory(&monitor, true, &mut created_dirs)?;
        layout.status_directory(&root.join("Status"), true, &mut created_dirs)?;
        let binary = root.join("secblitz.exe");
        let source_path = std::env::current_exe()?;
        let mut source = open(&source_path, GENERIC_READ, OPEN_EXISTING, None)?;
        let source_info = info(&source)?;
        ensure!(
            source_info.dwFileAttributes
                & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
                == 0,
            "Invalid installer executable"
        );
        let binary_sd = descriptor(BINARY_SD)?;
        match open(
            &binary,
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL,
            CREATE_NEW,
            Some(&binary_sd),
        ) {
            Ok(mut output) => {
                created_files.push(binary.clone());
                inspect(&output, false, true, false, true)?;
                std::io::copy(&mut source, &mut output)?;
                output.sync_all()?;
                drop(output);
            }
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| matches!(e.raw_os_error(), Some(80 | 183))) =>
            {
                let existing = open(&binary, GENERIC_READ | READ_CONTROL, OPEN_EXISTING, None)?;
                inspect(&existing, false, true, false, true)?;
                let i = info(&existing)?;
                ensure!(
                    i.dwVolumeSerialNumber == source_info.dwVolumeSerialNumber
                        && i.nFileIndexHigh == source_info.nFileIndexHigh
                        && i.nFileIndexLow == source_info.nFileIndexLow,
                    "Destination binary exists and is not this installer; refusing overwrite"
                );
            }
            Err(e) => return Err(e),
        }
        let image = open(&binary, GENERIC_READ | READ_CONTROL, OPEN_EXISTING, None)?;
        inspect(&image, false, true, false, true)?;
        layout.held.push(image);
        let report = monitor.join("latest.json");
        let report_sd = descriptor(REPORT_SD)?;
        match open(
            &report,
            GENERIC_WRITE | READ_CONTROL,
            CREATE_NEW,
            Some(&report_sd),
        ) {
            Ok(mut f) => {
                created_files.push(report.clone());
                inspect(&f, false, true, true, false)?;
                f.write_all(b"{\"status\":\"not yet scanned\"}\n")?;
                f.sync_all()?;
            }
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| matches!(e.raw_os_error(), Some(80 | 183))) =>
            {
                inspect(
                    &open(
                        &report,
                        READ_CONTROL | FILE_READ_ATTRIBUTES,
                        OPEN_EXISTING,
                        None,
                    )?,
                    false,
                    true,
                    true,
                    false,
                )?;
            }
            Err(e) => return Err(e),
        }
        let service = scm.create_service(
            &ServiceInfo {
                name: NAME.into(),
                display_name: "Secblitz read-only security monitor".into(),
                service_type: ServiceType::OWN_PROCESS,
                // Do not expose a runnable, partially configured registration.
                start_type: ServiceStartType::Disabled,
                error_control: ServiceErrorControl::Normal,
                executable_path: binary.clone(),
                launch_arguments: vec!["service".into(), "run".into()],
                dependencies: vec![],
                account_name: Some(ACCOUNT.into()),
                account_password: None,
            },
            ServiceAccess::CHANGE_CONFIG
                | ServiceAccess::DELETE
                | ServiceAccess::WRITE_DAC
                | ServiceAccess::WRITE_OWNER,
        )?;
        registration = Some(service);
        let service = registration.as_ref().unwrap();
        let sd = descriptor(SERVICE_SD)?;
        ensure!(
            unsafe {
                SetServiceObjectSecurity(
                    service.raw_handle().cast(),
                    OWNER_SECURITY_INFORMATION
                        | DACL_SECURITY_INFORMATION
                        | PROTECTED_DACL_SECURITY_INFORMATION,
                    sd.0,
                )
            } != 0,
            "Cannot secure service configuration: {}",
            error()
        );
        let cmd = wide(command(&binary)?)?;
        ensure!(
            unsafe {
                ChangeServiceConfigW(
                    service.raw_handle().cast(),
                    u32::MAX,
                    u32::MAX,
                    u32::MAX,
                    cmd.as_ptr(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    null(),
                )
            } != 0,
            "Cannot set service command: {}",
            error()
        );
        #[repr(C)]
        struct Privileges {
            names: *const u16,
        }
        let mut names = wide("SeChangeNotifyPrivilege")?;
        names.push(0);
        let privileges = Privileges {
            names: names.as_ptr(),
        };
        ensure!(
            unsafe {
                ChangeServiceConfig2W(
                    service.raw_handle().cast(),
                    6,
                    &privileges as *const _ as *const c_void,
                )
            } != 0,
            "Cannot restrict service privileges: {}",
            error()
        );
        service.set_description("Read-only security observations every 15 minutes. No automatic remediation; latest report in Program Files/Secblitz/Monitor.")?;
        ensure!(
            unsafe {
                ChangeServiceConfigW(
                    service.raw_handle().cast(),
                    u32::MAX,
                    2,
                    u32::MAX,
                    null(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    null(),
                )
            } != 0,
            "Cannot enable automatic startup: {}",
            error()
        );
        Ok(())
    })();
    if let Err(e) = result {
        // Only delete the registration returned by our successful CreateService.
        // If deletion fails, retain the binary rather than strand a live service.
        if let Some(service) = registration.take() {
            service.delete().context(format!(
                "Install failed ({e:#}); rollback could not remove registration; files retained"
            ))?;
        }
        let mut failures = Vec::new();
        for p in created_files.iter().rev() {
            if let Err(e) = std::fs::remove_file(p) {
                failures.push(format!("{}: {e}", p.display()));
            }
        }
        for p in created_dirs.iter().rev() {
            if let Err(e) = std::fs::remove_dir(p) {
                failures.push(format!("{}: {e}", p.display()));
            }
        }
        return Err(e.context(format!(
            "Installation rolled back; cleanup failures: {failures:?}"
        )));
    }
    Ok(())
}

pub fn start() -> Result<()> {
    crate::platform::require_admin("Service startup requires Administrator elevation")?;
    ensure!(
        cfg!(target_arch = "x86_64"),
        "The monitor requires Windows x64"
    );
    let scm = manager(ServiceManagerAccess::CONNECT)?;
    let service = scm.open_service(
        NAME,
        ServiceAccess::QUERY_CONFIG
            | ServiceAccess::QUERY_STATUS
            | ServiceAccess::START
            | ServiceAccess::READ_CONTROL,
    )?;
    // Reject registrations whose configuration can be redirected by an
    // untrusted caller, even if their current command line happens to match.
    unsafe {
        let mut sd = null_mut();
        let rc = GetSecurityInfo(
            service.raw_handle().cast(),
            SE_SERVICE,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        );
        ensure!(rc == 0, "Cannot inspect service security ({rc})");
        let sd = Local(sd);
        inspect_service_descriptor(sd.0)?;
    }
    let mut layout = Layout::parents()?;
    let root = layout.root.clone();
    layout.directory(&root, false, &mut Vec::new())?;
    let monitor = root.join("Monitor");
    layout.directory(&monitor, false, &mut Vec::new())?;
    let binary = root.join("secblitz.exe");
    let image = open(&binary, GENERIC_READ | READ_CONTROL, OPEN_EXISTING, None)?;
    inspect(&image, false, true, false, true)?;
    layout.held.push(image);
    // The service validates its report before announcing Running. Do not open
    // it here with restrictive sharing: a running monitor holds write access.
    let cfg = service.query_config()?;
    validate_start_config(&cfg, &binary)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    match service.query_status()?.current_state {
        ServiceState::Running => return Ok(()),
        ServiceState::Stopped => {
            if let Err(e) = service.start::<&str>(&[]) {
                if !matches!(&e, windows_service::Error::Winapi(e) if e.raw_os_error() == Some(1056))
                {
                    return Err(e.into());
                }
            }
        }
        ServiceState::StartPending => (),
        state => bail!("Monitor cannot start from SCM state {state:?}"),
    }
    loop {
        let status = service.query_status()?;
        match status.current_state {
            ServiceState::Running => return Ok(()),
            ServiceState::StartPending => (),
            state => bail!(
                "Monitor did not reach Running: {state:?}, exit {:?}",
                status.exit_code
            ),
        }
        ensure!(Instant::now() < deadline,
            "Timed out waiting for SCM Running; the monitor may still start. Installation was retained");
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn validate_start_config(
    cfg: &windows_service::service::ServiceConfig,
    binary: &Path,
) -> Result<()> {
    ensure!(
        cfg.executable_path.as_os_str() == OsStr::new(&command(binary)?)
            && cfg.service_type == ServiceType::OWN_PROCESS
            && cfg
                .account_name
                .as_ref()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(ACCOUNT)),
        "Unexpected service configuration; refusing to start"
    );
    Ok(())
}

fn inspect_service_descriptor(sd: *mut c_void) -> Result<()> {
    // Compare trustees/rights semantically: SCM maps generic rights on storage.
    unsafe {
        let mut owner = null_mut();
        let mut defaulted = 0;
        ensure!(
            GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted) != 0,
            "Cannot inspect service owner"
        );
        let admins = sid("S-1-5-32-544")?;
        let system = sid("S-1-5-18")?;
        let local_service = sid("S-1-5-19")?;
        let users = sid("S-1-5-32-545")?;
        ensure!(
            !owner.is_null()
                && IsValidSid(owner) != 0
                && (EqualSid(owner, admins.0) != 0 || EqualSid(owner, system.0) != 0),
            "Untrusted service owner"
        );
        let mut acl = null_mut();
        let mut present = 0;
        ensure!(
            GetSecurityDescriptorDacl(sd, &mut present, &mut acl, &mut defaulted) != 0
                && present != 0
                && !acl.is_null()
                && IsValidAcl(acl) != 0,
            "Missing or invalid service DACL"
        );
        let mut control = 0;
        let mut revision = 0;
        ensure!(
            GetSecurityDescriptorControl(sd, &mut control, &mut revision) != 0
                && control & SE_DACL_PROTECTED != 0,
            "Unprotected service DACL"
        );
        let mut seen = [false; 4];
        for index in 0..(*acl).AceCount as u32 {
            let mut ace = null_mut();
            ensure!(
                GetAce(acl, index, &mut ace) != 0,
                "Cannot inspect service ACE"
            );
            let header = &*(ace as *const ACE_HEADER);
            ensure!(
                header.AceType == 0
                    && header.AceFlags == 0
                    && header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>(),
                "Unexpected service ACE"
            );
            let a = &*(ace as *const ACCESS_ALLOWED_ACE);
            let trustee = &a.SidStart as *const u32 as *mut c_void;
            let available = header.AceSize as usize - 8;
            ensure!(
                available >= 8
                    && 8 + *(trustee.cast::<u8>().add(1)) as usize * 4 <= available
                    && IsValidSid(trustee) != 0,
                "Invalid service trustee"
            );
            let trustees = [system.0, admins.0, local_service.0, users.0];
            let slot = trustees
                .iter()
                .position(|s| EqualSid(trustee, *s) != 0)
                .context("Unexpected service trustee")?;
            let expected = if slot < 2 { 0x000f01ff } else { 0x0002018d };
            ensure!(
                !seen[slot] && (a.Mask == expected || (slot < 2 && a.Mask == GENERIC_ALL)),
                "Unexpected service permissions"
            );
            seen[slot] = true;
        }
        ensure!(seen.iter().all(|v| *v), "Missing service trustees");
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    crate::platform::require_admin("Service removal requires Administrator elevation")?;
    let scm = manager(ServiceManagerAccess::CONNECT)?;
    let service = match scm.open_service(
        NAME,
        ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE,
    ) {
        Ok(s) => s,
        Err(e) if absent(&e) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let cfg = service.query_config()?;
    let expected = command(&base()?.join("Secblitz").join("secblitz.exe"))?;
    ensure!(
        cfg.executable_path.as_os_str() == OsStr::new(&expected)
            && cfg.service_type == ServiceType::OWN_PROCESS
            && cfg
                .account_name
                .as_ref()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(ACCOUNT)),
        "Unexpected service configuration; refusing to delete"
    );
    ensure!(
        service.query_status()?.current_state == ServiceState::Stopped,
        "Stop SecblitzMonitor through SCM before uninstalling"
    );
    service.delete()?;
    Ok(())
}
pub fn status_details() -> Result<super::StatusDetails> {
    use super::{MonitorState, StatusDetails};
    let scm = manager(ServiceManagerAccess::CONNECT)?;
    match scm.open_service(NAME, ServiceAccess::QUERY_STATUS) {
        Ok(s) => {
            let s = s.query_status()?;
            let state = match s.current_state {
                ServiceState::Stopped => MonitorState::Stopped,
                ServiceState::StartPending => MonitorState::StartPending,
                ServiceState::StopPending => MonitorState::StopPending,
                ServiceState::Running => MonitorState::Running,
                ServiceState::ContinuePending => MonitorState::ContinuePending,
                ServiceState::PausePending => MonitorState::PausePending,
                ServiceState::Paused => MonitorState::Paused,
            };
            let (win32_exit_code, service_exit_code) = match s.exit_code {
                ServiceExitCode::Win32(code) => (Some(code), None),
                ServiceExitCode::ServiceSpecific(code) => (None, Some(code)),
            };
            Ok(StatusDetails {
                state,
                win32_exit_code,
                service_exit_code,
                checkpoint: s.checkpoint,
                wait_hint_ms: s.wait_hint.as_millis() as u64,
            })
        }
        Err(e) if absent(&e) => Ok(StatusDetails {
            state: MonitorState::NotInstalled,
            win32_exit_code: None,
            service_exit_code: None,
            checkpoint: 0,
            wait_hint_ms: 0,
        }),
        Err(e) => Err(e.into()),
    }
}

define_windows_service!(ffi_main, service_main);
pub fn run() -> Result<()> {
    service_dispatcher::start(NAME, ffi_main)?;
    Ok(())
}
fn set_status(
    handle: &ServiceStatusHandle,
    state: ServiceState,
    checkpoint: u32,
    failed: bool,
) -> Result<()> {
    handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: if failed {
            ServiceExitCode::ServiceSpecific(1)
        } else {
            ServiceExitCode::Win32(0)
        },
        checkpoint,
        wait_hint: if matches!(
            state,
            ServiceState::StartPending | ServiceState::StopPending
        ) {
            Duration::from_secs(100)
        } else {
            Duration::ZERO
        },
        process_id: None,
    })?;
    Ok(())
}
fn service_main(_: Vec<OsString>) {
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let (wake, receiver) = mpsc::sync_channel(1);
    let handle = match service_control_handler::register(NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            signal.store(true, Ordering::Release);
            let _ = wake.try_send(());
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    }) {
        Ok(h) => h,
        Err(_) => return,
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        set_status(&handle, ServiceState::StartPending, 1, false)?;
        let mut layout = Layout::parents()?;
        let root = layout.root.clone();
        layout.directory(&root, false, &mut Vec::new())?;
        let monitor = root.join("Monitor");
        layout.directory(&monitor, false, &mut Vec::new())?;
        // Best effort: an install predating the Status directory keeps working.
        let status_dir = {
            let d = root.join("Status");
            layout
                .status_directory(&d, false, &mut Vec::new())
                .ok()
                .map(|_| d)
        };
        let mut report = open(
            &monitor.join("latest.json"),
            GENERIC_WRITE | READ_CONTROL,
            OPEN_EXISTING,
            None,
        )?;
        inspect(&report, false, true, true, false)?;
        set_status(&handle, ServiceState::Running, 0, false)?;
        let mut checkpoint = 0;
        while !stop.load(Ordering::Acquire) {
            let started = Instant::now();
            let cancel = stop.clone();
            let worker = std::thread::Builder::new()
                .name("secblitz-observe".into())
                .spawn(move || scan(&cancel))?;
            let mut status_error = None;
            while !worker.is_finished() {
                let _ = receiver.recv_timeout(Duration::from_secs(1));
                if stop.load(Ordering::Acquire) {
                    checkpoint += 1;
                    if let Err(e) =
                        set_status(&handle, ServiceState::StopPending, checkpoint, false)
                    {
                        status_error = Some(e);
                    }
                }
            }
            let snapshot = worker
                .join()
                .map_err(|_| anyhow::anyhow!("Monitor worker panicked"))?;
            if let Some(e) = status_error {
                return Err(e);
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
            let (bytes, summary) = snapshot?;
            ensure!(bytes.len() <= REPORT_LIMIT, "Monitor report exceeded limit");
            report.seek(SeekFrom::Start(0))?;
            report.set_len(0)?;
            report.write_all(&bytes)?;
            report.sync_all()?;
            if let Some(dir) = &status_dir {
                let _ = crate::status::write_to(dir, &summary);
            }
            let remaining = INTERVAL.saturating_sub(started.elapsed());
            let _ = receiver.recv_timeout(remaining);
        }
        set_status(&handle, ServiceState::StopPending, checkpoint + 1, false)?;
        Ok(())
    }));
    let failed = !matches!(outcome, Ok(Ok(())));
    let _ = set_status(&handle, ServiceState::Stopped, 0, failed);
}

fn short(s: &str) -> String {
    s.chars().take(512).collect()
}
fn scan(stop: &AtomicBool) -> Result<(Vec<u8>, crate::status::Status)> {
    use serde_json::json;
    let start = Instant::now();
    let mut rows = Vec::new();
    let mut findings = Vec::new();
    let mut readiness = None;
    let mut incomplete = false;
    let mut items: Vec<(String, crate::status::Item)> = Vec::new();
    match crate::platform::backend().map(crate::permissions::with_permissions) {
        Err(e) => {
            incomplete = true;
            rows.push(json!({"status":"unknown", "error":short(&e.to_string())}));
        }
        Ok(mut backend) => {
            if !stop.load(Ordering::Acquire) {
                readiness = Some(backend.readiness());
            }
            let controls = backend.controls();
            incomplete |= controls.len() > 64;
            for control in controls.into_iter().take(64) {
                if stop.load(Ordering::Acquire) || start.elapsed() >= BUDGET {
                    incomplete = true;
                    break;
                }
                let row = match backend.observe(&control.id) {
                    Ok(o) => {
                        items.push((
                            control.id.clone(),
                            crate::status::classify(&control.id, &control.target, &o),
                        ));
                        json!({"id":short(&control.id), "status":"observed", "eligible":o.eligible,
                        "value":short(&o.value.to_string()), "detail":short(&o.reason),
                        "effective":o.effective, "authority":o.authority})
                    }
                    Err(e) => {
                        incomplete = true;
                        items.push((control.id.clone(), crate::status::Item::Unknown));
                        json!({"id":short(&control.id), "status":"unknown", "error":short(&e.to_string())})
                    }
                };
                rows.push(row);
            }
            if !stop.load(Ordering::Acquire) && start.elapsed() < BUDGET {
                match backend.findings() {
                    Ok(items) => {
                        incomplete |= items.len() > 64;
                        for f in items.into_iter().take(64) {
                            findings.push(json!({"title":short(&f.title), "status":short(&f.status), "detail":short(&f.detail)}));
                        }
                    }
                    Err(e) => {
                        incomplete = true;
                        findings.push(json!({"status":"unknown", "error":short(&e.to_string())}));
                    }
                }
            } else {
                incomplete = true;
            }
        }
    }
    // Serialize through a capped writer: escaping/multi-byte data cannot exceed
    // the bound. An oversized snapshot becomes an explicit unknown summary.
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len() + bytes.len() > REPORT_LIMIT {
                return Err(std::io::Error::other("Report limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let summary = crate::status::summarize(&items, !incomplete, crate::status::now());
    let mut out = Bounded(Vec::new());
    let snapshot = json!({"schema":1, "unix_time":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        "incomplete":incomplete, "observations":rows, "findings":findings, "readiness":readiness});
    if serde_json::to_writer(&mut out, &snapshot).is_err() {
        return Ok((b"{\"schema\":1,\"incomplete\":true,\"status\":\"unknown\",\"error\":\"report exceeded 64 KiB\"}".to_vec(), summary));
    }
    Ok((out.0, summary))
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "status_tests.rs"]
mod status_tests;

#[cfg(test)]
mod start_security_tests {
    use super::*;

    #[test]
    fn service_start_requires_exact_command_account_and_process_flags() {
        let binary = Path::new(r"C:\Program Files\Secblitz\secblitz.exe");
        let mut cfg = windows_service::service::ServiceConfig {
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: command(binary).unwrap().into(),
            load_order_group: None,
            tag_id: 0,
            dependencies: vec![],
            account_name: Some(ACCOUNT.into()),
            display_name: NAME.into(),
        };
        validate_start_config(&cfg, binary).unwrap();
        for path in [
            r"C:\Program Files\Secblitz\secblitz.exe service run",
            r#""C:\Program Files\Secblitz\secblitz.exe" service run extra"#,
            r#""C:\Users\User\secblitz.exe" service run"#,
            r#""C:\Program Files\Secblitz\secblitz.exe" service install"#,
        ] {
            cfg.executable_path = path.into();
            assert!(validate_start_config(&cfg, binary).is_err());
        }
        cfg.executable_path = command(binary).unwrap().into();
        for account in [
            None,
            Some("LocalSystem".into()),
            Some(r"NT AUTHORITY\NetworkService".into()),
        ] {
            cfg.account_name = account;
            assert!(validate_start_config(&cfg, binary).is_err());
        }
        cfg.account_name = Some(ACCOUNT.into());
        for kind in [
            ServiceType::SHARE_PROCESS,
            ServiceType::OWN_PROCESS | ServiceType::INTERACTIVE_PROCESS,
        ] {
            cfg.service_type = kind;
            assert!(validate_start_config(&cfg, binary).is_err());
        }
    }

    #[test]
    fn service_start_rejects_untrusted_registration_security() {
        for good in [
            SERVICE_SD,
            "O:SYG:BAD:P(A;;0xf01ff;;;SY)(A;;0xf01ff;;;BA)(A;;0x2018d;;;LS)(A;;0x2018d;;;BU)",
        ] {
            inspect_service_descriptor(descriptor(good).unwrap().0).unwrap();
        }
        for bad in [
            SERVICE_SD.replace("O:BA", "O:BU"),
            SERVICE_SD.replace("D:P", "D:"),
            SERVICE_SD.replace("CCLCSWLOCRRC;;;BU", "GA;;;BU"),
            SERVICE_SD.replace("CCLCSWLOCRRC;;;LS", "CCLCSWLOCRRCWP;;;LS"),
            format!("{SERVICE_SD}(A;;GA;;;WD)"),
        ] {
            assert!(inspect_service_descriptor(descriptor(&bad).unwrap().0).is_err());
        }
    }
}

//! Registration and control of the `SecblitzFilter` service (LocalService, disabled until a
//! switch goes on) and its protected folders. Needs an elevated caller except `state`.
//! Folders and the binary are inspected before use; anything unexpected fails closed.

use anyhow::{bail, ensure, Context, Result};
use std::{
    ffi::{c_void, OsStr},
    fs::File,
    mem::{size_of, zeroed},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_service::{
    service::{
        Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
        ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
        ServiceState as Scm, ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath},
};

use super::control::ServiceState;
use super::{config, SERVICE_NAME};
use crate::platform::security::{descriptor, error, sid, wide, wide_str, Local};

const ACCOUNT: &str = r"NT AUTHORITY\LocalService";
const DISPLAY_NAME: &str = "Secblitz web protection";
const DESCRIPTION: &str = "Blocks ads, trackers and dangerous websites for Secblitz";
const WAIT: Duration = Duration::from_secs(10);

const RX: u32 = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
const LS_MODIFY: u32 = 0x1301bf;
/// Settings: administrators write, the filter and everyone else may only read.
const FILTER_SD: &str =
    "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;LS)(A;OICI;0x1200a9;;;BU)";
/// Status and lists: the filter may write, everyone else may only read.
const DATA_SD: &str =
    "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;LS)(A;OICI;0x1200a9;;;BU)";
/// Service object: only administrators and SYSTEM may change or start it;
/// local users may only read its state.
const SERVICE_SD: &str =
    "O:BAG:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;CCLCSWLOCRRC;;;LS)(A;;CCLCSWLOCRRC;;;BU)";

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

const NEEDS_ADMIN: &str = "Web protection setup needs administrator rights";

fn local_path(path: &Path) -> Result<&str> {
    let text = path.to_str().context("Non-Unicode Windows path")?;
    let b = text.as_bytes();
    ensure!(
        b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\',
        "Expected an absolute local drive path"
    );
    ensure!(
        !text.contains(['\0', '"', '/', '*', '?', '<', '>', '|']),
        "Invalid Windows path characters"
    );
    Ok(text)
}

fn program_files() -> Result<PathBuf> {
    let mut raw = null_mut();
    // SAFETY: valid GUID and output pointer; freed below even on failure.
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, null_mut(), &mut raw) };
    let path = if hr >= 0 && !raw.is_null() {
        // SAFETY: success returns a NUL-terminated UTF-16 string, freed only below.
        let slice = unsafe { wide_str(raw) };
        slice
            .and_then(|s| String::from_utf16(s).ok())
            .map(PathBuf::from)
    } else {
        None
    };
    // SAFETY: null or the allocation returned above.
    unsafe { CoTaskMemFree(raw.cast()) };
    path.context("Program Files is not available")
}

fn expected_binary() -> Result<PathBuf> {
    let path = program_files()?.join("Secblitz").join("secblitz.exe");
    local_path(&path)?;
    Ok(path)
}

fn installed_binary() -> Result<PathBuf> {
    let binary = expected_binary()?;
    ensure!(
        crate::service::trusted_status_dir().is_some()
            && std::env::current_exe().ok().and_then(|p| p
                .file_name()
                .map(|n| n.eq_ignore_ascii_case("secblitz.exe")))
                == Some(true),
        "Web protection needs Secblitz installed with its setup program"
    );
    Ok(binary)
}

fn command(binary: &Path) -> Result<String> {
    Ok(format!("\"{}\" filter run", local_path(binary)?))
}

fn open(path: &Path, access: u32, creation: u32) -> Result<File> {
    let path = wide(path)?;
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ,
            null(),
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

fn open_dir(path: &Path) -> Result<File> {
    open(
        path,
        READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
        OPEN_EXISTING,
    )
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

struct Ace {
    trustee: *mut c_void,
    kind: u8,
    flags: u8,
    mask: u32,
}

struct Security {
    _sd: Local,
    owner: *mut c_void,
    control: u16,
    aces: Vec<Ace>,
}

fn security(file: &File) -> Result<Security> {
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
        let guard = Local(sd);
        ensure!(
            !owner.is_null() && IsValidSid(owner) != 0,
            "Untrusted file owner"
        );
        ensure!(
            !acl.is_null() && IsValidAcl(acl) != 0,
            "Missing or invalid DACL"
        );
        let mut control = 0;
        let mut revision = 0;
        ensure!(
            GetSecurityDescriptorControl(sd, &mut control, &mut revision) != 0,
            "Cannot inspect DACL control"
        );
        let mut aces = Vec::new();
        for index in 0..(*acl).AceCount as u32 {
            let mut ace = null_mut();
            ensure!(GetAce(acl, index, &mut ace) != 0, "Cannot inspect ACE");
            let header = &*(ace as *const ACE_HEADER);
            ensure!(
                header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>(),
                "Short ACE"
            );
            ensure!(header.AceType <= 1, "Unsupported ACL entry");
            let a = &*(ace as *const ACCESS_ALLOWED_ACE);
            let trustee = &a.SidStart as *const u32 as *mut c_void;
            let sid_bytes = header.AceSize as usize - 8;
            ensure!(
                sid_bytes >= 8
                    && 8 + *(trustee.cast::<u8>().add(1)) as usize * 4 <= sid_bytes
                    && IsValidSid(trustee) != 0,
                "Invalid trustee SID"
            );
            aces.push(Ace {
                trustee,
                kind: header.AceType,
                flags: header.AceFlags,
                mask: a.Mask,
            });
        }
        Ok(Security {
            _sd: guard,
            owner,
            control,
            aces,
        })
    }
}

struct Sids {
    system: Local,
    admins: Local,
    installer: Local,
    local_service: Local,
    users: Local,
}

impl Sids {
    fn new() -> Result<Self> {
        Ok(Self {
            system: sid("S-1-5-18")?,
            admins: sid("S-1-5-32-544")?,
            installer: sid("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")?,
            local_service: sid("S-1-5-19")?,
            users: sid("S-1-5-32-545")?,
        })
    }

    fn eq(a: *mut c_void, b: &Local) -> bool {
        unsafe { EqualSid(a, b.0) != 0 }
    }

    /// SYSTEM or Administrators: may own protected objects.
    fn admin(&self, s: *mut c_void) -> bool {
        Self::eq(s, &self.system) || Self::eq(s, &self.admins)
    }

    /// Also the Windows installer, which owns Program Files content.
    fn privileged(&self, s: *mut c_void) -> bool {
        self.admin(s) || Self::eq(s, &self.installer)
    }
}

const NO_FOLLOW: u32 = FILE_ATTRIBUTE_REPARSE_POINT;
const GRANT_BITS: u32 = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE;

/// A folder on the way to ours (Program Files, ProgramData ...): trusted
/// owner, no reparse point, and no one but the privileged may do more than
/// read, list or add children (the Windows defaults).
fn check_ancestor(file: &File) -> Result<()> {
    let i = info(file)?;
    ensure!(
        i.dwFileAttributes & NO_FOLLOW == 0,
        "Reparse point rejected"
    );
    ensure!(
        i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
        "Wrong object type"
    );
    let s = security(file)?;
    let sids = Sids::new()?;
    ensure!(sids.privileged(s.owner), "Untrusted folder owner");
    let benign = FILE_GENERIC_READ
        | FILE_GENERIC_EXECUTE
        | FILE_ADD_FILE
        | FILE_ADD_SUBDIRECTORY
        | FILE_WRITE_EA
        | FILE_WRITE_ATTRIBUTES
        | GENERIC_READ
        | GENERIC_EXECUTE;
    for ace in &s.aces {
        // Inherit-only entries do not apply here; deny entries only restrict.
        if ace.flags & INHERIT_ONLY_ACE as u8 != 0 || ace.kind == 1 {
            continue;
        }
        if !sids.privileged(ace.trustee) {
            ensure!(ace.mask & !benign == 0, "Writable folder on the path");
        }
    }
    Ok(())
}

fn check_binary(file: &File) -> Result<()> {
    let i = info(file)?;
    ensure!(
        i.dwFileAttributes & (NO_FOLLOW | FILE_ATTRIBUTE_DIRECTORY) == 0 && i.nNumberOfLinks == 1,
        "Unexpected service binary"
    );
    let s = security(file)?;
    let sids = Sids::new()?;
    ensure!(sids.privileged(s.owner), "Untrusted service binary owner");
    for ace in &s.aces {
        if ace.flags & INHERIT_ONLY_ACE as u8 != 0 || ace.kind == 1 {
            continue;
        }
        if !sids.privileged(ace.trustee) {
            ensure!(
                ace.mask & !(RX | GENERIC_READ | GENERIC_EXECUTE) == 0,
                "Service binary is writable"
            );
        }
    }
    Ok(())
}

/// A folder we made: protected DACL, SYSTEM and Administrators full, the
/// filter `ls_mask`, users read only, nothing else.
fn check_protected(file: &File, ls_mask: u32) -> Result<()> {
    let i = info(file)?;
    ensure!(
        i.dwFileAttributes & NO_FOLLOW == 0 && i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
        "Unexpected web protection folder"
    );
    let s = security(file)?;
    let sids = Sids::new()?;
    ensure!(sids.admin(s.owner), "Untrusted web protection folder owner");
    ensure!(
        s.control & SE_DACL_PROTECTED != 0,
        "Web protection folder is not protected"
    );
    let mut seen = [false; 4];
    for ace in &s.aces {
        ensure!(
            ace.kind == 0 && ace.flags & 3 == 3 && ace.flags & !(GRANT_BITS as u8) == 0,
            "Unexpected web protection folder entry"
        );
        let (slot, mask) = if Sids::eq(ace.trustee, &sids.system) {
            (0, FILE_ALL_ACCESS)
        } else if Sids::eq(ace.trustee, &sids.admins) {
            (1, FILE_ALL_ACCESS)
        } else if Sids::eq(ace.trustee, &sids.local_service) {
            (2, ls_mask)
        } else if Sids::eq(ace.trustee, &sids.users) {
            (3, RX)
        } else {
            bail!("Unexpected web protection folder trustee");
        };
        ensure!(ace.mask == mask, "Unexpected web protection folder rights");
        seen[slot] = true;
    }
    ensure!(
        seen.iter().all(|s| *s),
        "Web protection folder trustees are missing"
    );
    Ok(())
}

/// Opens and inspects every folder from the drive root down to `path`
/// (inclusive). The handles pin the folders (no rename or delete) while held.
fn pin_path(path: &Path) -> Result<Vec<File>> {
    let text = local_path(path)?;
    let root = wide(&text[..3])?;
    ensure!(
        unsafe { GetDriveTypeW(root.as_ptr()) } == 3,
        "A fixed local drive is required"
    );
    let mut held = Vec::new();
    let mut p = PathBuf::from(&text[..3]);
    let file = open_dir(&p)?;
    check_ancestor(&file)?;
    held.push(file);
    for part in text[3..].split('\\').filter(|p| !p.is_empty()) {
        p.push(part);
        let file = open_dir(&p)?;
        check_ancestor(&file).with_context(|| format!("Untrusted folder {}", p.display()))?;
        held.push(file);
    }
    Ok(held)
}

fn create_dir(path: &Path, sddl: &str) -> Result<()> {
    let sd = descriptor(sddl)?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    let p = wide(path)?;
    if unsafe { CreateDirectoryW(p.as_ptr(), &sa) } == 0 {
        let code = unsafe { GetLastError() };
        ensure!(
            code == ERROR_ALREADY_EXISTS,
            "Create directory failed ({code})"
        );
    }
    Ok(())
}

pub fn ensure_dirs() -> Result<()> {
    crate::platform::require_admin(NEEDS_ADMIN)?;
    let filter = config::dir()?;
    let root = filter.parent().context("Missing Secblitz folder")?;
    let program_data = root.parent().context("Missing ProgramData folder")?;
    let _held = pin_path(program_data)?;
    // The `Secblitz` folder belongs to the change journal, which creates it
    // with administrators-only permissions and refuses it otherwise.
    let state = crate::platform::state_dir()?;
    ensure!(state == root, "Unexpected web protection folder");
    let root_pin = open_dir(root)?;
    check_ancestor(&root_pin)?;
    create_dir(&filter, FILTER_SD)?;
    let filter_pin = open_dir(&filter)?;
    check_protected(&filter_pin, 0x1200a9)?;
    let data = filter.join("Data");
    create_dir(&data, DATA_SD)?;
    let data_pin = open_dir(&data)?;
    check_protected(&data_pin, LS_MODIFY)?;
    Ok(())
}

fn refuse_links(dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = std::fs::symlink_metadata(entry.path())?;
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & NO_FOLLOW == 0,
            "A link inside the web protection folder was left alone"
        );
        if meta.is_dir() {
            refuse_links(&entry.path())?;
        }
    }
    Ok(())
}

/// Deletes `ProgramData\Secblitz\Filter` after checking the folders on the
/// way and its owner; a folder with a link inside is not touched.
pub fn remove_dir() -> Result<()> {
    crate::platform::require_admin(NEEDS_ADMIN)?;
    let filter = config::dir()?;
    match std::fs::symlink_metadata(&filter) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    let _held = pin_path(filter.parent().context("Missing Secblitz folder")?)?;
    let pin = open_dir(&filter)?;
    let i = info(&pin)?;
    ensure!(
        i.dwFileAttributes & NO_FOLLOW == 0 && i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
        "Unexpected web protection folder"
    );
    ensure!(
        Sids::new()?.admin(security(&pin)?.owner),
        "Untrusted web protection folder owner"
    );
    refuse_links(&filter)?;
    // The pin shares read only, so close it before the delete.
    drop(pin);
    std::fs::remove_dir_all(&filter).context("Remove the web protection folder")
}

fn manager(access: ServiceManagerAccess) -> Result<ServiceManager> {
    Ok(ServiceManager::local_computer(None::<&str>, access)?)
}

fn absent(e: &windows_service::Error) -> bool {
    matches!(e, windows_service::Error::Winapi(e) if e.raw_os_error() == Some(1060))
}

fn not_active(e: &windows_service::Error) -> bool {
    matches!(e, windows_service::Error::Winapi(e) if matches!(e.raw_os_error(), Some(1062)))
}

fn open_service(access: ServiceAccess) -> Result<Option<Service>> {
    let scm = manager(ServiceManagerAccess::CONNECT)?;
    match scm.open_service(SERVICE_NAME, access) {
        Ok(service) => Ok(Some(service)),
        Err(e) if absent(&e) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn validate_config(service: &Service, binary: &Path) -> Result<()> {
    let cfg = service.query_config()?;
    ensure!(
        cfg.executable_path.as_os_str() == OsStr::new(&command(binary)?)
            && cfg.service_type == ServiceType::OWN_PROCESS
            && cfg
                .account_name
                .as_ref()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(ACCOUNT)),
        "Unexpected web protection service configuration; it was not changed"
    );
    Ok(())
}

fn set_start_type(service: &Service, start: u32) -> Result<()> {
    ensure!(
        unsafe {
            ChangeServiceConfigW(
                service.raw_handle().cast(),
                u32::MAX,
                start,
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
        "Cannot change the web protection start type: {}",
        error()
    );
    Ok(())
}

fn wait_for(service: &Service, goal: Scm) -> Result<bool> {
    let start = Instant::now();
    loop {
        if service.query_status()?.current_state == goal {
            return Ok(true);
        }
        if start.elapsed() >= WAIT {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Registers the service (disabled, not started). Already registered with the
/// expected command and account is fine; anything else is left alone and
/// refused.
pub fn install() -> Result<()> {
    crate::platform::require_admin(NEEDS_ADMIN)?;
    ensure!(
        cfg!(target_arch = "x86_64"),
        "Web protection requires Windows x64"
    );
    let binary = installed_binary()?;
    let _held = pin_path(binary.parent().context("Missing program folder")?)?;
    let image = open(&binary, GENERIC_READ | READ_CONTROL, OPEN_EXISTING)?;
    check_binary(&image)?;
    let scm = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    match scm.open_service(SERVICE_NAME, ServiceAccess::QUERY_CONFIG) {
        Ok(existing) => return validate_config(&existing, &binary),
        Err(e) if absent(&e) => (),
        Err(e) => return Err(e.into()),
    }
    let service = scm.create_service(
        &ServiceInfo {
            name: SERVICE_NAME.into(),
            display_name: DISPLAY_NAME.into(),
            service_type: ServiceType::OWN_PROCESS,
            // Never a runnable, half-configured registration.
            start_type: ServiceStartType::Disabled,
            error_control: ServiceErrorControl::Normal,
            executable_path: binary.clone(),
            launch_arguments: vec!["filter".into(), "run".into()],
            dependencies: vec![],
            account_name: Some(ACCOUNT.into()),
            account_password: None,
        },
        // START: Windows requires it to set restart-on-failure actions.
        ServiceAccess::CHANGE_CONFIG
            | ServiceAccess::START
            | ServiceAccess::DELETE
            | ServiceAccess::WRITE_DAC
            | ServiceAccess::WRITE_OWNER,
    )?;
    let configured = configure(&service, &binary);
    if let Err(e) = configured {
        // Only the registration this call made is removed.
        service.delete().context(format!(
            "Web protection setup failed ({e:#}); the new registration could not be removed"
        ))?;
        return Err(e.context("Web protection setup was rolled back"));
    }
    Ok(())
}

fn configure(service: &Service, binary: &Path) -> Result<()> {
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
        "Cannot secure the web protection service: {}",
        error()
    );
    // Quote explicitly, even for a path without spaces.
    let cmd = wide(command(binary)?)?;
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
        "Cannot set the web protection command: {}",
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
        "Cannot restrict the web protection service: {}",
        error()
    );
    service.set_description(DESCRIPTION)?;
    let restart = |seconds| ServiceAction {
        action_type: ServiceActionType::Restart,
        delay: Duration::from_secs(seconds),
    };
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 60 * 60)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![restart(5), restart(5), restart(30)]),
    })?;
    Ok(())
}

/// Starts (and sets to start with Windows) or stops (and disables) the
/// filter. Turning on waits up to ten seconds for it to be running.
pub fn set_enabled(on: bool) -> Result<()> {
    crate::platform::require_admin(NEEDS_ADMIN)?;
    let service = open_service(
        ServiceAccess::QUERY_CONFIG
            | ServiceAccess::QUERY_STATUS
            | ServiceAccess::CHANGE_CONFIG
            | ServiceAccess::START
            | ServiceAccess::STOP,
    )?
    .context("The web protection service is not installed")?;
    validate_config(&service, &expected_binary()?)?;
    if on {
        set_start_type(&service, 2)?;
        if service.query_status()?.current_state == Scm::Stopped {
            if let Err(e) = service.start::<&str>(&[]) {
                // A concurrent start is fine; everything else is not.
                if !matches!(&e, windows_service::Error::Winapi(e) if e.raw_os_error() == Some(1056))
                {
                    return Err(e.into());
                }
            }
        }
        ensure!(
            wait_for(&service, Scm::Running)?,
            "Web protection did not start in time"
        );
        Ok(())
    } else {
        if let Err(e) = service.stop() {
            if !not_active(&e) {
                return Err(e.into());
            }
        }
        let stopped = wait_for(&service, Scm::Stopped)?;
        set_start_type(&service, 4)?;
        ensure!(stopped, "Web protection did not stop in time");
        Ok(())
    }
}

pub fn state() -> Result<ServiceState> {
    let Some(service) = open_service(ServiceAccess::QUERY_STATUS)? else {
        return Ok(ServiceState::NotInstalled);
    };
    Ok(match service.query_status()?.current_state {
        Scm::Running => ServiceState::Running,
        Scm::Stopped => ServiceState::Stopped,
        _ => ServiceState::Other,
    })
}

pub fn delete() -> Result<()> {
    crate::platform::require_admin(NEEDS_ADMIN)?;
    let Some(service) =
        open_service(ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE)?
    else {
        return Ok(());
    };
    if service.query_status()?.current_state != Scm::Stopped {
        if let Err(e) = service.stop() {
            if !not_active(&e) {
                return Err(e.into());
            }
        }
        ensure!(
            wait_for(&service, Scm::Stopped)?,
            "Web protection did not stop in time"
        );
    }
    match service.delete() {
        Ok(()) => Ok(()),
        Err(e) if absent(&e) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

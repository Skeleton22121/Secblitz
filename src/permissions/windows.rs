use super::descriptor::assess;
use super::{
    repair_target, service_name,
    state::{trusted_owner, State},
};
use crate::model::{CheckStatus, Finding};
use crate::model::Observation;
use anyhow::{ensure, Result};
use serde_json::Value;
use std::{ffi::c_void, io, ptr};

const SERVICES: [&str; 5] = [
    "BITS",
    "wuauserv",
    "WinDefend",
    "Schedule",
    "SecblitzMonitor",
];

#[link(name = "advapi32")]
extern "system" {
    fn OpenSCManagerW(machine: *const u16, database: *const u16, access: u32) -> *mut c_void;
    fn OpenServiceW(manager: *mut c_void, name: *const u16, access: u32) -> *mut c_void;
    fn QueryServiceObjectSecurity(
        service: *mut c_void,
        information: u32,
        descriptor: *mut c_void,
        size: u32,
        needed: *mut u32,
    ) -> i32;
    fn CloseServiceHandle(handle: *mut c_void) -> i32;
    fn SetServiceObjectSecurity(
        service: *mut c_void,
        information: u32,
        descriptor: *const c_void,
    ) -> i32;
    fn QueryServiceConfigW(
        service: *mut c_void,
        config: *mut c_void,
        size: u32,
        needed: *mut u32,
    ) -> i32;
    fn GetSecurityInfo(
        handle: *mut c_void,
        kind: u32,
        info: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        sd: *mut *mut c_void,
    ) -> u32;
    fn IsValidSid(sid: *const c_void) -> i32;
    fn GetLengthSid(sid: *const c_void) -> u32;
}
struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenSCManagerW/OpenServiceW and is closed exactly once.
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}
/// Copies a word buffer filled by Windows into plain bytes, in memory order.
fn words_to_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_ne_bytes()).collect()
}

fn handle(raw: *mut c_void) -> io::Result<Handle> {
    if raw.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(Handle(raw))
    }
}

fn open(id: &str, access: u32) -> Result<Handle> {
    // SAFETY: null machine and database name select the local active services database.
    let manager = handle(unsafe { OpenSCManagerW(ptr::null(), ptr::null(), 1) })?;
    let wide: Vec<u16> = service_name(id)?.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `manager` is live and `wide` is NUL-terminated UTF-16.
    Ok(handle(unsafe {
        OpenServiceW(manager.0, wide.as_ptr(), access)
    })?)
}

fn snapshot(service: &Handle) -> Result<State> {
    let mut storage = vec![0u32; 2048];
    let mut needed = 0;
    // OWNER | GROUP | DACL, deliberately never SACL.
    // SAFETY: `storage` is a writable 8192-byte buffer and that size is what is passed.
    let ok = unsafe {
        QueryServiceObjectSecurity(service.0, 7, storage.as_mut_ptr().cast(), 8192, &mut needed)
    };
    if ok == 0 {
        return Err(io::Error::last_os_error().into());
    }
    ensure!(needed <= 8192, "Invalid service descriptor size");
    State::from_sd(&words_to_bytes(&storage))
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ServiceConfig {
    kind: u32,
    start: u32,
    error: u32,
    binary: *const u16,
    group: *const u16,
    tag: u32,
    dependencies: *const u16,
    account: *const u16,
    display: *const u16,
}

fn config_string(buffer: &[u64], pointer: *const u16) -> Result<String> {
    let base = buffer.as_ptr() as usize;
    let offset = (pointer as usize)
        .checked_sub(base)
        .ok_or_else(|| anyhow::anyhow!("Config string outside buffer"))?;
    ensure!(
        offset.is_multiple_of(2)
            && offset >= std::mem::size_of::<ServiceConfig>()
            && offset < std::mem::size_of_val(buffer),
        "Invalid config string offset"
    );
    let bytes: Vec<u8> = buffer.iter().flat_map(|word| word.to_ne_bytes()).collect();
    let mut wide = Vec::new();
    for pair in bytes[offset..].chunks_exact(2) {
        let c = u16::from_le_bytes([pair[0], pair[1]]);
        if c == 0 {
            return Ok(String::from_utf16(&wide)?);
        }
        wide.push(c);
    }
    anyhow::bail!("Unterminated service config string")
}

/// Identity screening of the service host, not Authenticode or DLL-path auditing.
fn identity(service: &Handle) -> Result<std::fs::File> {
    use std::os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows_sys::Win32::{
        Foundation::LocalFree, System::SystemInformation::GetSystemDirectoryW,
    };
    let mut storage = vec![0u64; 1024];
    let mut needed = 0;
    // SAFETY: `storage` is a writable 8192-byte buffer and that size is what is passed.
    let ok =
        unsafe { QueryServiceConfigW(service.0, storage.as_mut_ptr().cast(), 8192, &mut needed) };
    if ok == 0 {
        return Err(io::Error::last_os_error().into());
    }
    ensure!(needed <= 8192, "Invalid service config size");
    // SAFETY: the successful call filled the start of `storage`, which is 8-byte aligned and larger than ServiceConfig.
    let config = unsafe { ptr::read(storage.as_ptr().cast::<ServiceConfig>()) };
    ensure!(config.kind == 0x20, "Unexpected built-in service type");
    ensure!(
        config_string(&storage, config.account)?.eq_ignore_ascii_case("LocalSystem"),
        "Unexpected built-in service account"
    );
    let binary = config_string(&storage, config.binary)?.to_ascii_lowercase();
    let mut directory = vec![0u16; 32768];
    // SAFETY: the pointer and length describe `directory`.
    let length =
        unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
    ensure!(
        length > 0 && length < directory.len(),
        "Cannot resolve Windows system directory"
    );
    let path =
        std::path::PathBuf::from(String::from_utf16(&directory[..length])?).join("svchost.exe");
    let full = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid system directory"))?
        .to_ascii_lowercase();
    let hosts = [
        full.clone(),
        format!("\"{full}\""),
        "%systemroot%\\system32\\svchost.exe".into(),
        "\"%systemroot%\\system32\\svchost.exe\"".into(),
    ];
    ensure!(
        hosts.iter().any(|host| [" -k netsvcs", " -k netsvcs -p"]
            .iter()
            .any(|args| binary == format!("{host}{args}"))),
        "Unexpected built-in service executable configuration"
    );
    let file = std::fs::OpenOptions::new()
        .access_mode(0x20080)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?; // READ_CONTROL | attributes; open reparse point itself
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.file_attributes() & 0x400 == 0 && metadata.len() != 0,
        "Service host is not a regular non-reparse file"
    );
    let mut owner = ptr::null_mut();
    let mut sd = ptr::null_mut();
    // SAFETY: the file handle is live; the out pointers are valid and the returned descriptor is freed by `Local` below.
    let error = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            1,
            1,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut sd,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32).into());
    }
    struct Local(*mut c_void);
    impl Drop for Local {
        fn drop(&mut self) {
            // SAFETY: the descriptor was allocated by GetSecurityInfo and is freed exactly once.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let _allocation = Local(sd);
    ensure!(
        // SAFETY: the pointer is null-checked first and IsValidSid only inspects the SID header.
        !owner.is_null() && unsafe { IsValidSid(owner) } != 0,
        "Invalid service host owner"
    );
    // SAFETY: `owner` was just validated as a SID.
    let size = unsafe { GetLengthSid(owner) } as usize;
    ensure!((8..=68).contains(&size), "Invalid service host SID size");
    // SAFETY: the SID is valid, `size` bytes long and kept alive by `_allocation`.
    trusted_owner(unsafe { std::slice::from_raw_parts(owner.cast::<u8>(), size) })?;
    Ok(file)
}

pub(super) fn observe(id: &str) -> Result<Observation> {
    let service = open(id, 0x20001)?; // READ_CONTROL | QUERY_CONFIG
    let before = snapshot(&service)?;
    let value = before.value();
    let eligibility = (|| -> Result<()> {
        crate::platform::permission_gate(id)?;
        trusted_owner(&before.owner)?;
        let _host = identity(&service)?;
        repair_target(id, &value)?;
        Ok(())
    })();
    Ok(Observation {
        value,
        eligible: eligibility.is_ok(),
        reason: match eligibility {
            Ok(()) => "Eligible service permission repair".into(),
            Err(error) => format!("Service permissions preserved: {error}"),
        },
        ..Observation::default()
    })
}

pub(super) fn write(id: &str, value: &Value) -> Result<()> {
    let desired = State::parse(value)?;
    desired.repair()?; // No NULL, untrusted owner or complex rollback payloads.
    crate::platform::permission_gate(id)?;
    let service = open(id, 0x60001)?; // READ_CONTROL | WRITE_DAC | QUERY_CONFIG
    let _host = identity(&service)?;
    let current = snapshot(&service)?;
    current.check_transition(&desired)?;
    crate::platform::permission_gate(id)?;
    let _host_again = identity(&service)?;
    ensure!(
        snapshot(&service)? == current,
        "Service descriptor changed before write"
    );
    if current == desired {
        return Ok(());
    }
    let bytes = desired.sd();
    let mut aligned = vec![0u32; bytes.len().div_ceil(4)];
    for (word, chunk) in aligned.iter_mut().zip(bytes.chunks(4)) {
        let mut padded = [0u8; 4];
        padded[..chunk.len()].copy_from_slice(chunk);
        *word = u32::from_ne_bytes(padded);
    }
    // DACL_SECURITY_INFORMATION only: never set owner, group, SACL or protection.
    // SAFETY: `aligned` holds the whole descriptor with 4-byte alignment and outlives the call.
    let ok = unsafe { SetServiceObjectSecurity(service.0, 4, aligned.as_ptr().cast()) };
    if ok == 0 {
        return Err(io::Error::last_os_error().into());
    }
    ensure!(
        snapshot(&service)? == desired,
        "Service DACL exact readback mismatch; mutation outcome requires review"
    );
    Ok(())
}

fn inspect(manager: &Handle, name: &str) -> Result<Finding> {
    let title = format!("Service permissions: {name}");
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // READ_CONTROL only. No CHANGE_CONFIG, WRITE_DAC, owner or SACL access.
    // SAFETY: `manager` is live and `wide` is NUL-terminated UTF-16.
    let service = match handle(unsafe { OpenServiceW(manager.0, wide.as_ptr(), 0x20000) }) {
        Ok(service) => service,
        Err(e) if e.raw_os_error() == Some(1060) => {
            return Ok(Finding {
                title,
                status: CheckStatus::Info,
                detail: "Service is not installed; no DACL assessed.".into(),
            })
        }
        Err(e) => return Err(e.into()),
    };
    // Documented maximum: 8 KiB. u32 storage supplies descriptor alignment.
    let mut storage = vec![0u32; 2048];
    let mut needed = 0;
    // SAFETY: `storage` is a writable 8192-byte buffer and that size is what is passed.
    let ok = unsafe {
        QueryServiceObjectSecurity(service.0, 4, storage.as_mut_ptr().cast(), 8192, &mut needed)
    };
    if ok == 0 {
        return Err(io::Error::last_os_error().into());
    }
    ensure!(needed <= 8192, "Invalid security descriptor length");
    // pcbBytesNeeded is documented for failures; on success the full initialized
    // buffer is safe to inspect. The parser bounds every DACL/ACE/SID access.
    let assessment = assess(&words_to_bytes(&storage))?;
    let (status, detail) = if assessment.unrestricted {
        ("review", "Absent or NULL DACL permits unrestricted access. Administrator investigation required; no automatic repair.".into())
    } else if !assessment.candidates.is_empty() {
        ("review", format!("Candidate dangerous broad-principal grants: {}. {}This is an ACE scan, not effective access or proof of exploitability. {}", assessment.candidates.join("; "), if assessment.complex { "Deny, inherited or unsupported ACE semantics require manual evaluation. " } else { "" }, if matches!(name, "BITS" | "wuauserv") { "Consult the fixed service repair control for gated eligibility." } else { "Review with the service owner; no automatic repair for this service." }))
    } else if assessment.unevaluated_grant {
        ("unknown", "Deny, inherited or unsupported descriptor, ACE or access-mask semantics require manual evaluation; no dangerous supported ALLOW candidate found. No automatic repair.".into())
    } else if assessment.complex {
        ("info", "No dangerous ALLOW bits found for Everyone, Authenticated Users or Builtin Users. Some deny, inherited or extra-permission rules were not evaluated, and none of them can widen access. Other principals, ownership and executable paths were not assessed.".into())
    } else {
        ("info", "No dangerous ALLOW bits found for Everyone, Authenticated Users or Builtin Users in this DACL. Limited scan: other principals, ownership and executable paths were not assessed.".into())
    };
    Ok(Finding {
        title,
        status: status.into(),
        detail,
    })
}

pub(super) fn audit() -> Vec<Finding> {
    // SAFETY: null machine and database name select the local active services database.
    let manager = handle(unsafe { OpenSCManagerW(ptr::null(), ptr::null(), 1) });
    SERVICES
        .iter()
        .map(|name| {
            let result = match &manager {
                Ok(manager) => inspect(manager, name),
                Err(error) => Err(anyhow::anyhow!("Cannot connect to local SCM: {error}")),
            };
            result.unwrap_or_else(|error| Finding {
                title: format!("Service permissions: {name}"),
                status: CheckStatus::Unknown,
                detail: format!("DACL could not be assessed: {error}. No change made."),
            })
        })
        .collect()
}

use super::{
    collect_with, local_path, power_fact, volume_with, PowerReadiness, Readiness, VolumeApi,
    VolumeReadiness,
};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SysFreeString, ERROR_FILE_NOT_FOUND, HANDLE, INVALID_HANDLE_VALUE,
    RPC_E_CHANGED_MODE,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenIntegrityLevel, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetDiskFreeSpaceExW, GetDriveTypeW, GetFileInformationByHandle,
    GetVolumeInformationW, GetVolumePathNameW, BY_HANDLE_FILE_INFORMATION,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, OPEN_EXISTING,
};
use windows_sys::Win32::System::Com::{
    CLSIDFromString, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, DISPATCH_PROPERTYGET, DISPPARAMS, EXCEPINFO,
};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::System::Variant::{VariantClear, VARIANT, VT_BOOL};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_ProgramData, SHGetKnownFolderPath, KF_FLAG_DONT_VERIFY,
};

const PATH_CAPACITY: usize = 32768;
// Win32 DRIVE_FIXED, without an extra feature module.
const DRIVE_FIXED: u32 = 3;

pub(super) fn collect() -> Readiness {
    collect_with(
        || volume(&system_path()?),
        || volume(&journal_path()?),
        power,
        reboot_bounded,
    )
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn system_path() -> Option<Vec<u16>> {
    let mut path = vec![0; PATH_CAPACITY];
    // SAFETY: writable UTF-16 buffer and its exact capacity.
    let len = unsafe { GetWindowsDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    if len == 0 || len >= path.len() {
        return None;
    }
    path.truncate(len + 1);
    local_path(&path).then_some(path)
}

struct TaskMem(*mut u16);
impl Drop for TaskMem {
    fn drop(&mut self) {
        // SAFETY: null or the allocation returned by SHGetKnownFolderPath.
        unsafe { CoTaskMemFree(self.0.cast()) }
    }
}

fn journal_path() -> Option<Vec<u16>> {
    let mut allocation = TaskMem(null_mut());
    // No KF_FLAG_CREATE, and no verification that might access a remote folder.
    // SAFETY: valid GUID and output; RAII frees even a failed call's allocation.
    let hr = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_ProgramData,
            KF_FLAG_DONT_VERIFY as u32,
            null_mut(),
            &mut allocation.0,
        )
    };
    if hr < 0 || allocation.0.is_null() {
        return None;
    }
    let mut path = Vec::new();
    for index in 0..PATH_CAPACITY {
        // SAFETY: successful Shell API returns an allocated NUL-terminated string.
        let unit = unsafe { *allocation.0.add(index) };
        path.push(unit);
        if unit == 0 {
            break;
        }
    }
    if !local_path(&path) {
        return None;
    }
    path.pop();
    if path.last() != Some(&(b'\\' as u16)) {
        path.push(b'\\' as u16);
    }
    path.extend(wide("Secblitz"));
    (path.len() <= PATH_CAPACITY).then_some(path)
}

struct NativeVolume;
impl VolumeApi for NativeVolume {
    fn fixed(&self, path: &[u16]) -> bool {
        // SAFETY: caller validates NUL-terminated paths.
        unsafe { GetDriveTypeW(path.as_ptr()) == DRIVE_FIXED }
    }
    fn root(&self, path: &[u16]) -> Option<Vec<u16>> {
        let mut root = vec![0; PATH_CAPACITY];
        // GetVolumePathNameW resolves the containing volume even for a leaf
        // that does not yet exist. It does not create the Secblitz directory.
        // SAFETY: validated input and writable output with exact capacity.
        if unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
            return None;
        }
        let end = root.iter().position(|c| *c == 0)?;
        root.truncate(end + 1);
        Some(root)
    }
    fn available(&self, root: &[u16]) -> Option<u64> {
        let mut available = 0_u64;
        // First output is caller/quota-aware; no total volume size is collected.
        // SAFETY: validated root and correctly sized output.
        (unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut available, null_mut(), null_mut()) } != 0)
            .then_some(available)
    }
    fn flags(&self, root: &[u16]) -> Option<u32> {
        let mut flags = 0;
        // SAFETY: validated root; only filesystem flags requested. In particular
        // no volume label, serial number or filesystem name is read.
        (unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                &mut flags,
                null_mut(),
                0,
            )
        } != 0)
            .then_some(flags)
    }
}

fn volume(path: &[u16]) -> Option<VolumeReadiness> {
    // GetVolumePathNameW follows junctions. Check and pin each existing ancestor
    // first, without following reparse points or allowing write/delete sharing.
    // Query the last existing ancestor when a leaf is absent; never race a newly
    // created leaf. A reparse point (including a directory mount) is Unknown.
    let (existing, _handles) = local_directories(path)?;
    volume_with(&NativeVolume, &existing)
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: exactly one owned successful file or process-token handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn local_directories(path: &[u16]) -> Option<(Vec<u16>, Vec<Handle>)> {
    if !local_path(path) || !NativeVolume.fixed(&[path[0], path[1], path[2], 0]) {
        return None;
    }
    let mut current = path[..3].to_vec();
    let mut existing = Vec::new();
    let mut handles = Vec::new();
    // Include the drive root before opening any child. Ancestors remain pinned
    // through all volume calls; OPEN_EXISTING cannot create any filesystem state.
    for part in std::iter::once(&[][..]).chain(path[3..path.len() - 1].split(|c| *c == 92)) {
        if !part.is_empty() {
            if current.last() != Some(&92) {
                current.push(92);
            }
            current.extend_from_slice(part);
        } else if !handles.is_empty() {
            continue;
        }
        current.push(0);
        // SAFETY: validated NUL-terminated local path, no security/template inputs.
        let raw = unsafe {
            CreateFileW(
                current.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            // Only an absent child is evidence that the existing parent contains
            // the prospective journal. Denial/sharing/path errors remain Unknown.
            return if !handles.is_empty() && unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND {
                Some((existing, handles))
            } else {
                None
            };
        }
        let handle = Handle(raw);
        // SAFETY: native output structure; inspect it only on success.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(handle.0, &mut info) } == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        {
            return None;
        }
        handles.push(handle);
        existing.clone_from(&current);
        current.pop();
    }
    Some((existing, handles))
}

fn power() -> Option<PowerReadiness> {
    // windows-sys supplies the native repr(C) SYSTEM_POWER_STATUS ABI.
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: correctly sized and aligned writable native structure.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return None;
    }
    power_fact(
        status.ACLineStatus,
        status.BatteryFlag,
        status.BatteryLifePercent,
    )
}

// At most one outstanding worker, including after timeouts. Workers own all
// COM state; only Option<bool> crosses the channel. Never cancel a COM thread.
static REBOOT_WORKER: Mutex<Option<mpsc::Receiver<Option<bool>>>> = Mutex::new(None);

fn reboot_bounded() -> Option<bool> {
    super::bounded_probe(&REBOOT_WORKER, Duration::from_secs(2), reboot_native)
}

struct Apartment(bool);
impl Apartment {
    fn enter() -> Option<Self> {
        // SAFETY: called on the owning worker; successful initialization is balanced.
        let hr = unsafe { CoInitializeEx(null(), COINIT_MULTITHREADED as u32) };
        if hr >= 0 {
            Some(Self(true))
        } else if hr == RPC_E_CHANGED_MODE {
            Some(Self(false))
        } else {
            None
        }
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() }
        }
    }
}

// Standard IDispatch ABI. Use windows-sys VARIANT instead of a hand-sized union:
// it is 24 bytes on x64 and 16 bytes on x86.
#[repr(C)]
struct Dispatch {
    vtable: *const DispatchVtable,
}
#[repr(C)]
struct DispatchVtable {
    query_interface:
        unsafe extern "system" fn(*mut Dispatch, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut Dispatch) -> u32,
    release: unsafe extern "system" fn(*mut Dispatch) -> u32,
    get_type_info_count: unsafe extern "system" fn(*mut Dispatch, *mut u32) -> HRESULT,
    get_type_info: unsafe extern "system" fn(*mut Dispatch, u32, u32, *mut *mut c_void) -> HRESULT,
    get_ids_of_names: unsafe extern "system" fn(
        *mut Dispatch,
        *const GUID,
        *const *const u16,
        u32,
        u32,
        *mut i32,
    ) -> HRESULT,
    invoke: unsafe extern "system" fn(
        *mut Dispatch,
        i32,
        *const GUID,
        u32,
        u16,
        *const DISPPARAMS,
        *mut VARIANT,
        *mut EXCEPINFO,
        *mut u32,
    ) -> HRESULT,
}
struct OwnedDispatch(*mut Dispatch);
impl Drop for OwnedDispatch {
    fn drop(&mut self) {
        // SAFETY: one owned reference from CoCreateInstance, on its apartment.
        unsafe {
            ((*(*self.0).vtable).release)(self.0);
        }
    }
}
struct Variant(VARIANT);
impl Drop for Variant {
    fn drop(&mut self) {
        unsafe {
            VariantClear(&mut self.0);
        }
    }
}
struct Exception(EXCEPINFO);
impl Drop for Exception {
    fn drop(&mut self) {
        // SAFETY: zero or BSTR outputs owned by the caller of Invoke.
        unsafe {
            SysFreeString(self.0.bstrSource);
            SysFreeString(self.0.bstrDescription);
            SysFreeString(self.0.bstrHelpFile);
        }
    }
}

fn system_info_class() -> Option<GUID> {
    // CLSIDFromProgID can CREATE a registry mapping when a ProgID is absent.
    // Read the machine's registered class instead, then parse its GUID. This
    // avoids an HKCU ProgID override. CoCreateInstance's separate class lookup
    // is protected at >Medium IL by the documented COM policy; see the review.
    let key = wide("SOFTWARE\\Classes\\Microsoft.Update.SystemInfo\\CLSID");
    let mut text = [0_u16; 40];
    let mut bytes = std::mem::size_of_val(&text) as u32;
    // SAFETY: fixed registry path, bounded writable buffer, REG_SZ only.
    if unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            null(),
            RRF_RT_REG_SZ,
            null_mut(),
            text.as_mut_ptr().cast(),
            &mut bytes,
        )
    } != 0
    {
        return None;
    }
    parse_system_info_class(&text, bytes)
}

fn parse_system_info_class(text: &[u16; 40], bytes: u32) -> Option<GUID> {
    if bytes != 78 || text[38] != 0 || text[0] != 123 || text[37] != 125 {
        return None;
    }
    let mut class = GUID::from_u128(0);
    // Braced, NUL-terminated CLSID string (not a ProgID); parsing is read-only.
    if unsafe { CLSIDFromString(text.as_ptr(), &mut class) } < 0 {
        return None;
    }
    // CLSID_SystemInformation from Microsoft's wuapi.h. Do not activate an
    // arbitrary class even if this machine mapping is damaged or replaced.
    const EXPECTED: GUID = GUID::from_u128(0xc01b9ba0_bea7_41ba_b604_d0a36f469133);
    (class.data1 == EXPECTED.data1
        && class.data2 == EXPECTED.data2
        && class.data3 == EXPECTED.data3
        && class.data4 == EXPECTED.data4)
        .then_some(class)
}

fn reboot_native() -> Option<bool> {
    // Medium/low IL COM may load an HKCU class, despite an HKLM ProgID lookup.
    // Do not execute that code even at the caller's own privilege. A new worker
    // does not inherit a caller's impersonation token; check the process token.
    if !machine_com_only() {
        return None;
    }
    let _apartment = Apartment::enter()?;
    let class = system_info_class()?;
    const IID_IDISPATCH: GUID = GUID::from_u128(0x00020400_0000_0000_c000_000000000046);
    let mut raw = null_mut();
    // SAFETY: standard IDispatch IID, no aggregation, local in-process only.
    let hr = unsafe {
        CoCreateInstance(
            &class,
            null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IDISPATCH,
            &mut raw,
        )
    };
    if hr < 0 || raw.is_null() {
        return None;
    }
    reboot_property(OwnedDispatch(raw.cast()))
}

fn reboot_property(object: OwnedDispatch) -> Option<bool> {
    const IID_NULL: GUID = GUID::from_u128(0);
    let property = wide("RebootRequired");
    let name = property.as_ptr();
    let mut id = 0;
    // SAFETY: owned IDispatch and a single fixed NUL-terminated property name.
    let table = unsafe { &*(*object.0).vtable };
    if unsafe { (table.get_ids_of_names)(object.0, &IID_NULL, &name, 1, 0, &mut id) } < 0 {
        return None;
    }
    let args = DISPPARAMS {
        rgvarg: null_mut(),
        rgdispidNamedArgs: null_mut(),
        cArgs: 0,
        cNamedArgs: 0,
    };
    // SAFETY: all-zero VARIANT is VT_EMPTY; all-zero EXCEPINFO owns no strings.
    let mut result = Variant(unsafe { std::mem::zeroed() });
    let mut exception = Exception(unsafe { std::mem::zeroed() });
    // SAFETY: no arguments, property-get only; correctly aligned native outputs.
    let hr = unsafe {
        (table.invoke)(
            object.0,
            id,
            &IID_NULL,
            0,
            DISPATCH_PROPERTYGET,
            &args,
            &mut result.0,
            &mut exception.0,
            null_mut(),
        )
    };
    if hr < 0 {
        return None;
    }
    // SAFETY: inspect discriminator before reading the BOOL union member.
    unsafe {
        let value = &result.0.Anonymous.Anonymous;
        if value.vt != VT_BOOL {
            return None;
        }
        match value.Anonymous.boolVal {
            0 => Some(false),
            -1 => Some(true),
            _ => None,
        }
    }
}

fn machine_com_only() -> bool {
    let mut raw = null_mut();
    // SAFETY: current process pseudo-handle, TOKEN_QUERY only, writable output.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return false;
    }
    let token = Handle(raw);
    // usize storage aligns TOKEN_MANDATORY_LABEL, with ample room for its SID.
    let mut storage = [0_usize; 16];
    let capacity = std::mem::size_of_val(&storage);
    let mut bytes = 0;
    // SAFETY: aligned output and exact byte capacity; failure never uses output.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            storage.as_mut_ptr().cast(),
            capacity as u32,
            &mut bytes,
        )
    } == 0
        || (bytes as usize) < std::mem::size_of::<TOKEN_MANDATORY_LABEL>()
        || bytes as usize > capacity
    {
        return false;
    }
    // SAFETY: checked size/alignment above. Validate the returned SID pointer is
    // inside this allocation before reading its fixed S-1-16-RID representation.
    let label = unsafe { &*storage.as_ptr().cast::<TOKEN_MANDATORY_LABEL>() };
    let start = storage.as_ptr() as usize;
    let sid = label.Label.Sid as usize;
    let Some(offset) = sid.checked_sub(start) else {
        return false;
    };
    if offset < std::mem::size_of::<TOKEN_MANDATORY_LABEL>()
        || offset > (bytes as usize).saturating_sub(12)
    {
        return false;
    }
    // SAFETY: twelve SID bytes are within the initialized API output buffer.
    let sid = unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>().add(offset), 12) };
    sid[..8] == [1, 1, 0, 0, 0, 0, 0, 16]
        && u32::from_le_bytes(sid[8..12].try_into().unwrap()) >= 0x3000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct FakeDispatch {
        dispatch: Dispatch,
        releases: u32,
        names_hr: HRESULT,
        invoke_hr: HRESULT,
        vt: u16,
        boolean: i16,
        property_only: bool,
    }
    unsafe extern "system" fn query(
        _: *mut Dispatch,
        _: *const GUID,
        _: *mut *mut c_void,
    ) -> HRESULT {
        -1
    }
    unsafe extern "system" fn add(_: *mut Dispatch) -> u32 {
        1
    }
    unsafe extern "system" fn release(this: *mut Dispatch) -> u32 {
        (*this.cast::<FakeDispatch>()).releases += 1;
        0
    }
    unsafe extern "system" fn count(_: *mut Dispatch, _: *mut u32) -> HRESULT {
        -1
    }
    unsafe extern "system" fn info(
        _: *mut Dispatch,
        _: u32,
        _: u32,
        _: *mut *mut c_void,
    ) -> HRESULT {
        -1
    }
    unsafe extern "system" fn names(
        this: *mut Dispatch,
        _: *const GUID,
        names: *const *const u16,
        count: u32,
        _: u32,
        id: *mut i32,
    ) -> HRESULT {
        let fake = &mut *this.cast::<FakeDispatch>();
        let expected = wide("RebootRequired");
        fake.property_only =
            count == 1 && std::slice::from_raw_parts(*names, expected.len()) == expected;
        *id = 7;
        fake.names_hr
    }
    unsafe extern "system" fn invoke(
        this: *mut Dispatch,
        id: i32,
        _: *const GUID,
        _: u32,
        flags: u16,
        args: *const DISPPARAMS,
        result: *mut VARIANT,
        _: *mut EXCEPINFO,
        _: *mut u32,
    ) -> HRESULT {
        let fake = &mut *this.cast::<FakeDispatch>();
        fake.property_only &= id == 7
            && flags == DISPATCH_PROPERTYGET
            && (*args).cArgs == 0
            && (*args).cNamedArgs == 0
            && (*args).rgvarg.is_null()
            && (*args).rgdispidNamedArgs.is_null();
        (*result).Anonymous.Anonymous.vt = fake.vt;
        (*result).Anonymous.Anonymous.Anonymous.boolVal = fake.boolean;
        fake.invoke_hr
    }
    static TABLE: DispatchVtable = DispatchVtable {
        query_interface: query,
        add_ref: add,
        release,
        get_type_info_count: count,
        get_type_info: info,
        get_ids_of_names: names,
        invoke,
    };

    #[test]
    fn dispatch_fixture_checks_property_types_failures_and_release() {
        use windows_sys::Win32::System::Variant::VT_I4;
        for (names_hr, invoke_hr, vt, boolean, expected) in [
            (0, 0, VT_BOOL, 0, Some(false)),
            (0, 0, VT_BOOL, -1, Some(true)),
            (0, 0, VT_BOOL, 1, None),
            (0, 0, VT_I4, 0, None),
            (0, -1, VT_BOOL, 0, None),
            (-1, 0, VT_BOOL, 0, None),
        ] {
            let mut fake = FakeDispatch {
                dispatch: Dispatch { vtable: &TABLE },
                releases: 0,
                names_hr,
                invoke_hr,
                vt,
                boolean,
                property_only: false,
            };
            // Fixture lends a live native-layout object for this synchronous call;
            // its Release counts the reference instead of deallocating stack memory.
            assert_eq!(reboot_property(OwnedDispatch(&mut fake.dispatch)), expected);
            assert_eq!(fake.releases, 1);
            assert!(fake.property_only);
        }
    }
    #[test]
    fn native_layouts() {
        assert_eq!(std::mem::size_of::<GUID>(), 16);
        assert_eq!(
            std::mem::size_of::<DispatchVtable>(),
            7 * std::mem::size_of::<usize>()
        );
        assert_eq!(std::mem::size_of::<SYSTEM_POWER_STATUS>(), 12);
        assert_eq!(
            std::mem::size_of::<VARIANT>(),
            if cfg!(target_pointer_width = "64") {
                24
            } else {
                16
            }
        );
    }

    #[test]
    fn class_mapping_is_bounded_and_pinned() {
        let text = wide("{C01B9BA0-BEA7-41BA-B604-D0A36F469133}");
        let mut buffer = [0; 40];
        buffer[..text.len()].copy_from_slice(&text);
        assert!(parse_system_info_class(&buffer, 78).is_some());
        for bytes in [0, 38, 76, 77, 79, 80, u32::MAX] {
            assert!(parse_system_info_class(&buffer, bytes).is_none());
        }
        buffer[1] = b'0' as u16;
        assert!(parse_system_info_class(&buffer, 78).is_none());
        buffer[1] = 0;
        assert!(parse_system_info_class(&buffer, 78).is_none());
    }

    // Real Windows agent: run explicitly with --ignored --nocapture in each
    // account context. This prints only the public, path-free readiness schema.
    #[test]
    #[ignore = "requires a real Windows host; inspect facts and OS activity"]
    fn native_readonly_smoke() {
        println!("{}", serde_json::to_string(&collect()).unwrap());
    }
}

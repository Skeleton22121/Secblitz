//! The machine-wide "don't push suggested apps" policy, with what was there
//! before recorded so Remove Secblitz can put it back.
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyValue {
    Absent,
    Dword(u32),
    Other,
}

pub trait PolicyStore {
    fn get(&self) -> Result<PolicyValue>;
    fn set(&mut self, value: u32) -> Result<()>;
    fn delete(&mut self) -> Result<()>;
}

#[derive(Serialize, Deserialize)]
struct Record {
    prior: Option<u32>,
}

pub fn journal_path() -> Result<PathBuf> {
    Ok(crate::platform::app_dir()?.join("suggested-policy.json"))
}

pub fn recorded(journal: &Path) -> bool {
    journal.is_file()
}

pub fn legacy_block(store: &dyn PolicyStore, journal: &Path) -> bool {
    !recorded(journal) && matches!(store.get(), Ok(PolicyValue::Dword(1)))
}

fn write_record(journal: &Path, prior: Option<u32>) -> Result<()> {
    let tmp = journal.with_extension("json.tmp");
    let text = serde_json::to_vec(&Record { prior })?;
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp).context("Save the suggested apps record")?;
        file.write_all(&text)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, journal).context("Save the suggested apps record")?;
    Ok(())
}

fn put_back(store: &mut dyn PolicyStore, prior: Option<u32>) -> Result<()> {
    match prior {
        Some(v) => store.set(v),
        None => store.delete(),
    }
}

pub fn block(store: &mut dyn PolicyStore, journal: &Path) -> Result<()> {
    let fresh = !recorded(journal);
    let mut prior = None;
    if fresh {
        prior = match store.get()? {
            PolicyValue::Absent => None,
            PolicyValue::Dword(v) => Some(v),
            PolicyValue::Other => {
                bail!("The setting has an unexpected value, so it was not changed")
            }
        };
        write_record(journal, prior)?;
    }
    let written = store.set(1).and_then(|_| store.get());
    if matches!(written, Ok(PolicyValue::Dword(1))) {
        return Ok(());
    }
    if fresh {
        let _ = put_back(store, prior);
        let _ = std::fs::remove_file(journal);
    }
    bail!("Windows did not confirm the setting")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Undo {
    Restored,
    NothingRecorded,
    ChangedSince,
}

pub fn undo(store: &mut dyn PolicyStore, journal: &Path) -> Result<Undo> {
    if !recorded(journal) {
        return Ok(Undo::NothingRecorded);
    }
    let text = std::fs::read_to_string(journal).context("Read the suggested apps record")?;
    let Ok(record) = serde_json::from_str::<Record>(&text) else {
        let _ = std::fs::remove_file(journal);
        return Ok(Undo::ChangedSince);
    };
    if store.get()? != PolicyValue::Dword(1) {
        let _ = std::fs::remove_file(journal);
        return Ok(Undo::ChangedSince);
    }
    put_back(store, record.prior)?;
    let expected = match record.prior {
        Some(v) => PolicyValue::Dword(v),
        None => PolicyValue::Absent,
    };
    ensure!(
        store.get()? == expected,
        "Windows did not confirm the setting"
    );
    std::fs::remove_file(journal).context("Remove the suggested apps record")?;
    Ok(Undo::Restored)
}

#[cfg(windows)]
pub struct MachinePolicy;

#[cfg(windows)]
mod sys {
    use super::{MachinePolicy, PolicyStore, PolicyValue};
    use anyhow::{bail, Result};
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_DWORD,
    };

    const NOT_FOUND: u32 = 2;
    const PATH_NOT_FOUND: u32 = 3;
    const MORE_DATA: u32 = 234;
    const KEY: &str = "SOFTWARE\\Policies\\Microsoft\\Windows\\CloudContent";
    const NAME: &str = "DisableWindowsConsumerFeatures";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn open(access: u32) -> Result<Option<HKEY>> {
        let path = wide(KEY);
        let mut handle: HKEY = null_mut();
        let status =
            // SAFETY: `path` is NUL-terminated and `handle` is a valid out-pointer.
            unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, access, &mut handle) };
        match status {
            0 => Ok(Some(handle)),
            NOT_FOUND | PATH_NOT_FOUND => Ok(None),
            other => bail!("cannot open the key ({other})"),
        }
    }

    impl PolicyStore for MachinePolicy {
        fn get(&self) -> Result<PolicyValue> {
            let Some(handle) = open(KEY_READ)? else {
                return Ok(PolicyValue::Absent);
            };
            let name = wide(NAME);
            let mut kind = 0u32;
            let mut data = [0u8; 4];
            let mut size = 4u32;
            // SAFETY: `handle` is open and `data`, `size` describe the same 4-byte buffer.
            let status = unsafe {
                RegQueryValueExW(
                    handle,
                    name.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut size,
                )
            };
            // SAFETY: `handle` was opened above and is closed once.
            unsafe { RegCloseKey(handle) };
            match status {
                0 if kind == REG_DWORD && size == 4 => {
                    Ok(PolicyValue::Dword(u32::from_le_bytes(data)))
                }
                0 | MORE_DATA => Ok(PolicyValue::Other),
                NOT_FOUND | PATH_NOT_FOUND => Ok(PolicyValue::Absent),
                other => bail!("cannot read the value ({other})"),
            }
        }

        fn set(&mut self, value: u32) -> Result<()> {
            let path = wide(KEY);
            let mut handle: HKEY = null_mut();
            // SAFETY: `path` is NUL-terminated and the out-pointers are valid.
            let status = unsafe {
                RegCreateKeyExW(
                    HKEY_LOCAL_MACHINE,
                    path.as_ptr(),
                    0,
                    std::ptr::null(),
                    0,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut handle,
                    null_mut(),
                )
            };
            if status != 0 {
                bail!("cannot open the key for writing ({status})");
            }
            let name = wide(NAME);
            let bytes = value.to_le_bytes();
            let status =
                // SAFETY: `handle` is open and `bytes` holds the 4 bytes passed.
                unsafe { RegSetValueExW(handle, name.as_ptr(), 0, REG_DWORD, bytes.as_ptr(), 4) };
            // SAFETY: `handle` was opened above and is closed once.
            unsafe { RegCloseKey(handle) };
            if status != 0 {
                bail!("cannot write the value ({status})");
            }
            Ok(())
        }

        fn delete(&mut self) -> Result<()> {
            let Some(handle) = open(KEY_SET_VALUE)? else {
                return Ok(());
            };
            let name = wide(NAME);
            // SAFETY: `handle` is open and `name` is NUL-terminated.
            let status = unsafe { RegDeleteValueW(handle, name.as_ptr()) };
            // SAFETY: `handle` was opened above and is closed once.
            unsafe { RegCloseKey(handle) };
            match status {
                0 | NOT_FOUND | PATH_NOT_FOUND => Ok(()),
                other => bail!("cannot remove the value ({other})"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mem(PolicyValue);

    impl PolicyStore for Mem {
        fn get(&self) -> Result<PolicyValue> {
            Ok(self.0)
        }
        fn set(&mut self, value: u32) -> Result<()> {
            self.0 = PolicyValue::Dword(value);
            Ok(())
        }
        fn delete(&mut self) -> Result<()> {
            self.0 = PolicyValue::Absent;
            Ok(())
        }
    }

    fn journal() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("suggested-policy.json");
        (dir, path)
    }

    #[test]
    fn block_records_absent_then_undo_deletes() {
        let (_d, j) = journal();
        let mut s = Mem(PolicyValue::Absent);
        block(&mut s, &j).unwrap();
        assert_eq!(s.0, PolicyValue::Dword(1));
        assert_eq!(std::fs::read_to_string(&j).unwrap(), r#"{"prior":null}"#);
        assert_eq!(undo(&mut s, &j).unwrap(), Undo::Restored);
        assert_eq!(s.0, PolicyValue::Absent);
        assert!(!recorded(&j));
    }

    #[test]
    fn block_records_previous_zero_then_undo_writes_zero() {
        let (_d, j) = journal();
        let mut s = Mem(PolicyValue::Dword(0));
        block(&mut s, &j).unwrap();
        assert_eq!(std::fs::read_to_string(&j).unwrap(), r#"{"prior":0}"#);
        assert_eq!(undo(&mut s, &j).unwrap(), Undo::Restored);
        assert_eq!(s.0, PolicyValue::Dword(0));
    }

    #[test]
    fn block_twice_keeps_first_prior() {
        let (_d, j) = journal();
        let mut s = Mem(PolicyValue::Absent);
        block(&mut s, &j).unwrap();
        block(&mut s, &j).unwrap();
        assert_eq!(std::fs::read_to_string(&j).unwrap(), r#"{"prior":null}"#);
        assert_eq!(undo(&mut s, &j).unwrap(), Undo::Restored);
        assert_eq!(s.0, PolicyValue::Absent);
    }

    #[test]
    fn block_refuses_non_dword() {
        let (_d, j) = journal();
        let mut s = Mem(PolicyValue::Other);
        assert!(block(&mut s, &j).is_err());
        assert_eq!(s.0, PolicyValue::Other);
        assert!(!recorded(&j));
    }

    #[test]
    fn undo_without_record_is_nothing_recorded() {
        let (_d, j) = journal();
        let mut s = Mem(PolicyValue::Dword(1));
        assert_eq!(undo(&mut s, &j).unwrap(), Undo::NothingRecorded);
        assert_eq!(s.0, PolicyValue::Dword(1));
    }

    #[test]
    fn legacy_block_detected_without_record() {
        let (_d, j) = journal();
        assert!(legacy_block(&Mem(PolicyValue::Dword(1)), &j));
        assert!(!legacy_block(&Mem(PolicyValue::Dword(0)), &j));
        assert!(!legacy_block(&Mem(PolicyValue::Absent), &j));
        let mut s = Mem(PolicyValue::Absent);
        block(&mut s, &j).unwrap();
        assert!(!legacy_block(&s, &j));
    }

    #[test]
    fn undo_after_someone_changed_it_leaves_it() {
        let (_d, j) = journal();
        let mut s = Mem(PolicyValue::Absent);
        block(&mut s, &j).unwrap();
        s.0 = PolicyValue::Dword(0);
        assert_eq!(undo(&mut s, &j).unwrap(), Undo::ChangedSince);
        assert_eq!(s.0, PolicyValue::Dword(0));
        assert!(!recorded(&j));
    }
}

//! AES-256-GCM through Windows CNG and the machine-bound DPAPI seal for the
//! per-backup key. No crypto is implemented here, only called.

use super::vault::Sealer;
use anyhow::{ensure, Result};
use rand::TryRng;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{LocalFree, STATUS_SUCCESS};
use windows_sys::Win32::Security::Cryptography::*;

const ENTROPY: &[u8] = b"Secblitz app backup key v1";

pub struct Key([u8; 32]);

impl Key {
    pub fn random() -> Key {
        let mut k = [0u8; 32];
        rand::rngs::SysRng
            .try_fill_bytes(&mut k)
            .expect("the system random generator failed");
        Key(k)
    }
    #[cfg(test)]
    pub fn from_bytes(b: [u8; 32]) -> Key {
        Key(b)
    }
    #[cfg(test)]
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        for b in self.0.iter_mut() {
            // Volatile so the clear is not optimised away.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

pub struct Aes {
    alg: BCRYPT_ALG_HANDLE,
    key: BCRYPT_KEY_HANDLE,
}

// SAFETY: the CNG handles are owned by this value and CNG allows use from any thread.
unsafe impl Send for Aes {}

impl Aes {
    pub fn new(key: &Key) -> Result<Aes> {
        let mut alg: BCRYPT_ALG_HANDLE = null_mut();
        // SAFETY: `alg` is a valid out-pointer; the algorithm id is a static string.
        let status =
            unsafe { BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_AES_ALGORITHM, null(), 0) };
        ensure!(
            status == STATUS_SUCCESS,
            "Encryption is unavailable ({status:#x})"
        );
        let mode: Vec<u16> = "ChainingModeGCM\0".encode_utf16().collect();
        // SAFETY: `mode` is a live NUL-terminated UTF-16 buffer and its byte length is passed.
        let status = unsafe {
            BCryptSetProperty(
                alg,
                BCRYPT_CHAINING_MODE,
                mode.as_ptr().cast(),
                (mode.len() * 2) as u32,
                0,
            )
        };
        if status != STATUS_SUCCESS {
            // SAFETY: `alg` was opened above and is not used afterwards.
            unsafe { BCryptCloseAlgorithmProvider(alg, 0) };
            anyhow::bail!("Encryption is unavailable ({status:#x})");
        }
        let mut handle: BCRYPT_KEY_HANDLE = null_mut();
        // SAFETY: `alg` is open, `handle` is a valid out-pointer and the secret is 32 live bytes.
        let status = unsafe {
            BCryptGenerateSymmetricKey(alg, &mut handle, null_mut(), 0, key.0.as_ptr(), 32, 0)
        };
        if status != STATUS_SUCCESS {
            // SAFETY: `alg` was opened above and is not used afterwards.
            unsafe { BCryptCloseAlgorithmProvider(alg, 0) };
            anyhow::bail!("Encryption is unavailable ({status:#x})");
        }
        Ok(Aes { alg, key: handle })
    }

    fn info(
        nonce: &[u8; 12],
        aad: &[u8],
        tag: &mut [u8; 16],
    ) -> BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
        BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
            cbSize: std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
            dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
            pbNonce: nonce.as_ptr() as *mut u8,
            cbNonce: 12,
            pbAuthData: if aad.is_empty() {
                null_mut()
            } else {
                aad.as_ptr() as *mut u8
            },
            cbAuthData: aad.len() as u32,
            pbTag: tag.as_mut_ptr(),
            cbTag: 16,
            pbMacContext: null_mut(),
            cbMacContext: 0,
            cbAAD: 0,
            cbData: 0,
            dwFlags: 0,
        }
    }
}

impl Drop for Aes {
    fn drop(&mut self) {
        // SAFETY: both handles are valid and released exactly once, here.
        unsafe {
            BCryptDestroyKey(self.key);
            BCryptCloseAlgorithmProvider(self.alg, 0);
        }
    }
}

impl Sealer for Aes {
    fn seal(&self, nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
        let mut tag = [0u8; 16];
        let info = Self::info(nonce, aad, &mut tag);
        let mut out = vec![0u8; plain.len()];
        let mut written = 0u32;
        // SAFETY: input and output buffers are live for the call and the lengths passed match them;
        // `info` points at `nonce`, `aad` and `tag`, which outlive the call.
        let status = unsafe {
            BCryptEncrypt(
                self.key,
                plain.as_ptr(),
                plain.len() as u32,
                (&info as *const BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast(),
                null_mut(),
                0,
                out.as_mut_ptr(),
                out.len() as u32,
                &mut written,
                0,
            )
        };
        ensure!(
            status == STATUS_SUCCESS && written as usize == plain.len(),
            "Encryption failed ({status:#x})"
        );
        out.extend_from_slice(&tag);
        Ok(out)
    }

    fn open(&self, nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
        ensure!(sealed.len() >= 16, "Damaged saved data");
        let (ct, tag_in) = sealed.split_at(sealed.len() - 16);
        let mut tag: [u8; 16] = tag_in.try_into().expect("16 bytes");
        let info = Self::info(nonce, aad, &mut tag);
        let mut out = vec![0u8; ct.len()];
        let mut written = 0u32;
        // SAFETY: as in `seal`; `ct` and `out` are live and equally long.
        let status = unsafe {
            BCryptDecrypt(
                self.key,
                ct.as_ptr(),
                ct.len() as u32,
                (&info as *const BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast(),
                null_mut(),
                0,
                out.as_mut_ptr(),
                out.len() as u32,
                &mut written,
                0,
            )
        };
        ensure!(
            status == STATUS_SUCCESS && written as usize == ct.len(),
            "Damaged saved data"
        );
        Ok(out)
    }
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    }
}

/// Seal the key to this PC (any elevated process here can open it; it is
/// useless if the saved copy is taken to another PC).
pub fn seal_key(key: &Key) -> Result<Vec<u8>> {
    let input = blob(&key.0);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // SAFETY: input and entropy blobs borrow live slices; `out` is a valid out-parameter.
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            &entropy,
            null(),
            null(),
            CRYPTPROTECT_LOCAL_MACHINE | CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    ensure!(
        ok != 0,
        "Couldn't protect the saved data key: {}",
        std::io::Error::last_os_error()
    );
    ensure!(!out.pbData.is_null(), "Couldn't protect the saved data key");
    // SAFETY: on success CryptProtectData returns `cbData` readable bytes at `pbData`.
    let sealed = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    // SAFETY: `pbData` was allocated by CryptProtectData for the caller to free with LocalFree.
    unsafe { LocalFree(out.pbData.cast()) };
    Ok(sealed)
}

pub fn unseal_key(sealed: &[u8]) -> Result<Key> {
    let input = blob(sealed);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // SAFETY: input and entropy blobs borrow live slices; `out` is a valid out-parameter.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            &entropy,
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    ensure!(ok != 0, "Damaged saved data key");
    ensure!(!out.pbData.is_null(), "Damaged saved data key");
    // SAFETY: on success CryptUnprotectData returns `cbData` writable bytes at `pbData`.
    let plain = unsafe { std::slice::from_raw_parts_mut(out.pbData, out.cbData as usize) };
    let result = if plain.len() == 32 {
        let mut k = [0u8; 32];
        k.copy_from_slice(plain);
        Ok(Key(k))
    } else {
        Err(anyhow::anyhow!("Damaged saved data key"))
    };
    for b in plain.iter_mut() {
        // SAFETY: `b` is a valid, unique reference into the returned buffer.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    // SAFETY: `pbData` was allocated by CryptUnprotectData for the caller to free; `plain` is dead.
    unsafe { LocalFree(out.pbData.cast()) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debloat::vault::Sealer;

    #[test]
    fn aes_gcm_roundtrip_and_tamper() {
        let key = Key::random();
        let aes = Aes::new(&key).unwrap();
        let nonce = [9u8; 12];
        let sealed = aes.seal(&nonce, b"aad", b"hello world").unwrap();
        assert_eq!(sealed.len(), 11 + 16);
        assert_eq!(aes.open(&nonce, b"aad", &sealed).unwrap(), b"hello world");
        assert!(aes.open(&nonce, b"aaD", &sealed).is_err(), "aad bound");
        let mut bad = sealed.clone();
        bad[0] ^= 1;
        assert!(aes.open(&nonce, b"aad", &bad).is_err(), "ciphertext bound");
        let mut bad = sealed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(aes.open(&nonce, b"aad", &bad).is_err(), "tag bound");
        assert!(
            Aes::new(&Key::random())
                .unwrap()
                .open(&nonce, b"aad", &sealed)
                .is_err(),
            "key bound"
        );
        let empty = aes.seal(&nonce, b"", b"").unwrap();
        assert_eq!(aes.open(&nonce, b"", &empty).unwrap(), b"");
    }

    #[test]
    fn known_answer_matches_nist_gcm() {
        let key = Key::from_bytes([0u8; 32]);
        let aes = Aes::new(&key).unwrap();
        let sealed = aes.seal(&[0u8; 12], b"", &[0u8; 16]).unwrap();
        assert_eq!(
            crate::debloat::backup::hex(&sealed),
            "cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919"
        );
    }

    #[test]
    fn dpapi_machine_seal_roundtrip() {
        let key = Key::random();
        let blob = seal_key(&key).unwrap();
        assert!(!blob.windows(32).any(|w| w == key.bytes()));
        assert_eq!(unseal_key(&blob).unwrap().bytes(), key.bytes());
        let mut bad = blob.clone();
        let mid = bad.len() / 2;
        bad[mid] ^= 1;
        assert!(unseal_key(&bad).is_err());
    }
}

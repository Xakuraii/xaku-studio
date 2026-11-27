#[cfg(windows)]
mod imp {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn target_name(key_name: &str) -> String {
        format!("XakuStudio/{key_name}")
    }

    pub struct CredentialStore;

    impl CredentialStore {
        pub fn set_secret(key_name: &str, secret: &str) -> Result<(), String> {
            let trimmed_key = key_name.trim();
            if trimmed_key.is_empty() {
                return Err("Credential key cannot be empty".to_string());
            }
            let mut target = to_wide_null(&target_name(trimmed_key));
            let mut user = to_wide_null("XakuStudio");
            let mut blob = secret.as_bytes().to_vec();

            let cred = CREDENTIALW {
                Flags: 0,
                Type: CRED_TYPE_GENERIC,
                TargetName: target.as_mut_ptr(),
                Comment: null_mut(),
                LastWritten: windows_sys::Win32::Foundation::FILETIME {
                    dwLowDateTime: 0,
                    dwHighDateTime: 0,
                },
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: if blob.is_empty() {
                    null_mut()
                } else {
                    blob.as_mut_ptr()
                },
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                AttributeCount: 0,
                Attributes: null_mut(),
                TargetAlias: null_mut(),
                UserName: user.as_mut_ptr(),
            };

            let ok = unsafe { CredWriteW(&cred, 0) };
            if ok == 0 {
                return Err(format!(
                    "CredWriteW failed for '{trimmed_key}': {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(())
        }

        pub fn get_secret(key_name: &str) -> Result<Option<String>, String> {
            let target = to_wide_null(&target_name(key_name.trim()));
            let mut cred_ptr: *mut CREDENTIALW = null_mut();
            let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred_ptr) };
            if ok == 0 {
                let code = unsafe { GetLastError() };
                if code == ERROR_NOT_FOUND {
                    return Ok(None);
                }
                return Err(format!(
                    "CredReadW failed for '{key_name}': {}",
                    std::io::Error::last_os_error()
                ));
            }

            let secret = unsafe {
                let cred = &*cred_ptr;
                if cred.CredentialBlob.is_null() || cred.CredentialBlobSize == 0 {
                    String::new()
                } else {
                    let bytes = std::slice::from_raw_parts(
                        cred.CredentialBlob,
                        cred.CredentialBlobSize as usize,
                    );
                    String::from_utf8_lossy(bytes).into_owned()
                }
            };
            unsafe { CredFree(cred_ptr as *const _) };
            Ok(Some(secret))
        }

        pub fn delete_secret(key_name: &str) -> Result<(), String> {
            let target = to_wide_null(&target_name(key_name.trim()));
            let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
            if ok == 0 {
                let code = unsafe { GetLastError() };
                if code == ERROR_NOT_FOUND {
                    return Ok(());
                }
                return Err(format!(
                    "CredDeleteW failed for '{key_name}': {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(())
        }

        pub fn has_secret(key_name: &str) -> bool {
            matches!(Self::get_secret(key_name), Ok(Some(v)) if !v.is_empty())
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub struct CredentialStore;
    impl CredentialStore {
        pub fn set_secret(_key_name: &str, _secret: &str) -> Result<(), String> {
            Err("OS credential store is only implemented for Windows".to_string())
        }
        pub fn get_secret(_key_name: &str) -> Result<Option<String>, String> {
            Ok(None)
        }
        pub fn delete_secret(_key_name: &str) -> Result<(), String> {
            Ok(())
        }
        pub fn has_secret(_key_name: &str) -> bool {
            false
        }
    }
}

pub use imp::CredentialStore;

//! Current-user DPAPI for the opt-in SMS archive. Never falls back to plaintext.
//!
//! Every error is a stable ASCII code from the `archive:` namespace. This crate sits below the
//! panel and holds no wording: the codes travel to the archive status line, which the panel
//! resolves through its catalog (`localization::stable_code_text`).

/// Encrypt `plaintext` for the current user. Errors are stable `archive:crypto_*` codes.
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, &'static str> {
    crypt(plaintext, true)
}

/// Decrypt `ciphertext` for the current user. Errors are stable `archive:crypto_*` codes.
pub fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, &'static str> {
    crypt(ciphertext, false)
}

#[cfg(not(windows))]
fn crypt(_input: &[u8], _protect: bool) -> Result<Vec<u8>, &'static str> {
    Err("archive:crypto_unsupported")
}

#[cfg(windows)]
fn crypt(input: &[u8], protect: bool) -> Result<Vec<u8>, &'static str> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    let size = u32::try_from(input.len()).map_err(|_| "archive:crypto_too_large")?;
    let source = CRYPT_INTEGER_BLOB {
        cbData: size,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut result = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: both blobs are valid for this synchronous call. No machine-wide flag is used;
    // Windows binds encryption to the current user's credentials and suppresses UI prompts.
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &source,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut result,
            )
        } else {
            CryptUnprotectData(
                &source,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut result,
            )
        }
    };
    if ok == 0 {
        return Err(if protect {
            "archive:crypto_failed"
        } else {
            "archive:crypto_decrypt_failed"
        });
    }
    // SAFETY: DPAPI returns cbData initialized bytes allocated by LocalAlloc on success.
    let bytes = if result.cbData == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(result.pbData, result.cbData as usize).to_vec() }
    };
    if !result.pbData.is_null() {
        // SAFETY: result is the sole owner of the DPAPI-allocated buffer.
        unsafe {
            LocalFree(result.pbData.cast());
        }
    }
    Ok(bytes)
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn dpapi_roundtrip_only_synthetic_data() {
        let original = b"synthetic archive crypto check";
        let encrypted = super::protect(original).expect("current-user DPAPI");
        assert_ne!(encrypted, original);
        assert_eq!(super::unprotect(&encrypted).unwrap(), original);
        assert!(super::unprotect(b"invalid synthetic ciphertext").is_err());
    }
}

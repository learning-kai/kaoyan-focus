use rusqlite::{params, Connection, OptionalExtension};

const PROTECTED_PREFIX: &str = "dpapi:v1:";

pub fn secret_configured(connection: &Connection, key: &str) -> Result<bool, String> {
    Ok(!get_raw_setting(connection, key)?
        .unwrap_or_default()
        .is_empty())
}

pub fn get_secret(connection: &Connection, key: &str) -> Result<String, String> {
    let Some(raw) = get_raw_setting(connection, key)? else {
        return Ok(String::new());
    };
    if raw.is_empty() {
        return Ok(String::new());
    }
    if let Some(payload) = raw.strip_prefix(PROTECTED_PREFIX) {
        return decrypt_secret(payload);
    }
    protect_existing_secret(connection, key, &raw)?;
    Ok(raw)
}

pub fn set_secret(
    connection: &Connection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> Result<(), String> {
    let stored = if value.is_empty() {
        String::new()
    } else {
        format!("{PROTECTED_PREFIX}{}", encrypt_secret(value)?)
    };
    set_raw_setting(connection, key, &stored, updated_at)
}

pub fn set_secret_if_changed(
    connection: &Connection,
    key: &str,
    new_value: &str,
    updated_at: &str,
) -> Result<(), String> {
    if new_value.is_empty() {
        if let Some(raw) = get_raw_setting(connection, key)? {
            if !raw.is_empty() && !raw.starts_with(PROTECTED_PREFIX) {
                protect_existing_secret(connection, key, &raw)?;
            }
        }
        return Ok(());
    }
    set_secret(connection, key, new_value, updated_at)
}

fn protect_existing_secret(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    set_secret(connection, key, value, &now)
}

fn get_raw_setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn set_raw_setting(
    connection: &Connection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> Result<(), String> {
    connection
        .execute(
            "
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            ",
            params![key, value, updated_at],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn encrypt_secret(value: &str) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };

    let mut bytes = value.as_bytes().to_vec();
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(
            &input,
            windows::core::w!("kaoyan-focus credential"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| error.to_string())?;

        let encrypted = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(STANDARD.encode(encrypted))
    }
}

#[cfg(windows)]
fn decrypt_secret(payload: &str) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };

    let mut encrypted = STANDARD
        .decode(payload)
        .map_err(|error| format!("Invalid protected credential payload: {error}"))?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| error.to_string())?;

        let decrypted = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        String::from_utf8(decrypted).map_err(|error| error.to_string())
    }
}

#[cfg(not(windows))]
fn encrypt_secret(_value: &str) -> Result<String, String> {
    Err("Protected credential storage is only supported on Windows.".to_string())
}

#[cfg(not(windows))]
fn decrypt_secret(_payload: &str) -> Result<String, String> {
    Err("Protected credential storage is only supported on Windows.".to_string())
}

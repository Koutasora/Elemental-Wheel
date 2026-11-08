//! Starting the wheel at sign-in: the value in the `Run` key, the same one the installer writes.

use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR};

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Same value name as in `installer/elemental-wheel.iss`.
const VALUE_NAME: &str = "Elemental Wheel";

/// `"<wheel.exe>" --run`, the data written by the installer.
pub fn command(wheel_exe: &Path) -> String {
    format!("\"{}\" --run", wheel_exe.display())
}

/// The wheel lies next to the running editor.
pub fn wheel_exe() -> Result<std::path::PathBuf, String> {
    std::env::current_exe()
        .map(|exe| exe.with_file_name("wheel.exe"))
        .map_err(|e| e.to_string())
}

fn check(status: WIN32_ERROR) -> Result<(), String> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(windows::core::Error::from(status.to_hresult()).message())
    }
}

/// Whether the value exists under `HKCU\<subkey>`.
pub fn is_enabled(subkey: &str) -> bool {
    let subkey = HSTRING::from(subkey);
    // SAFETY: only a probe for existence; no output buffer is passed.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            &HSTRING::from(VALUE_NAME),
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    };
    status == ERROR_SUCCESS
}

/// Writes (`Some(command)`) or deletes (`None`) the value under `HKCU\<subkey>`.
pub fn set(subkey: &str, command: Option<&str>) -> Result<(), String> {
    let subkey = HSTRING::from(subkey);
    let name = HSTRING::from(VALUE_NAME);
    let mut key = HKEY::default();
    // SAFETY: `key` is closed below; the buffers outlive the calls.
    unsafe {
        match command {
            Some(command) => {
                check(RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    &subkey,
                    None,
                    PCWSTR::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    None,
                    &mut key,
                    None,
                ))?;
                let wide: Vec<u8> = command
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                let result = check(RegSetValueExW(key, &name, None, REG_SZ, Some(&wide)));
                let _ = RegCloseKey(key);
                result
            }
            None => {
                let status =
                    RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_SET_VALUE, &mut key);
                if status == ERROR_FILE_NOT_FOUND {
                    return Ok(());
                }
                check(status)?;
                let status = RegDeleteValueW(key, &name);
                let _ = RegCloseKey(key);
                if status == ERROR_FILE_NOT_FOUND {
                    return Ok(());
                }
                check(status)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::RegDeleteTreeW;
    use windows::core::w;

    #[test]
    fn command_quotes_the_path_and_adds_run() {
        assert_eq!(
            command(Path::new(r"C:\Program Files\Elemental Wheel\wheel.exe")),
            r#""C:\Program Files\Elemental Wheel\wheel.exe" --run"#
        );
    }

    #[test]
    fn value_is_written_and_deleted_in_a_throwaway_key() {
        let key = r"Software\ElementalWheelTest\Run";
        // Leftover from an interrupted earlier run.
        // SAFETY: plain call with a constant key name.
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, w!(r"Software\ElementalWheelTest"));
        }
        assert!(!is_enabled(key));
        set(key, Some(r#""C:\x\wheel.exe" --run"#)).unwrap();
        assert!(is_enabled(key));
        set(key, None).unwrap();
        assert!(!is_enabled(key));
        // Deleting what is not there is fine.
        set(key, None).unwrap();
        // SAFETY: plain call with a constant key name.
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, w!(r"Software\ElementalWheelTest"));
        }
        assert!(!is_enabled(key));
    }
}

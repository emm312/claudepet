//! "Launch at Login" toggle, backed by the per-user Run key
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Mirrors
//! `Sources/ClaudePet/UI/LoginItemManager.swift` (which used `SMAppService`).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("ClaudePet");

fn exe_path() -> String {
    unsafe {
        let mut buf = [0u16; MAX_PATH as usize];
        let n = GetModuleFileNameW(None, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_enabled() -> bool {
    unsafe {
        let mut buf = [0u16; 1024];
        let mut cb = (buf.len() * 2) as u32;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_SUBKEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut cb),
        );
        status == ERROR_SUCCESS
    }
}

pub fn set_enabled(enabled: bool) {
    unsafe {
        if enabled {
            let value = format!("\"{}\"", exe_path());
            let data = wide(&value);
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_SUBKEY,
                VALUE_NAME,
                REG_SZ.0,
                Some(data.as_ptr() as *const _),
                (data.len() * 2) as u32,
            );
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_SUBKEY, VALUE_NAME);
        }
    }
}

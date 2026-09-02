//! Auto-update: check GitHub Releases for a newer `claudepet.exe`, download and
//! verify it, then swap it in and relaunch. Uses WinHTTP for HTTPS and BCrypt
//! for SHA-256 - no extra crates, TLS handled by the OS.
//!
//! Publishing side: `src-win/publish.ps1` (or the `release.yml` workflow on a
//! `v*` tag) uploads `claudepet.exe`, `claudepet-setup.exe` and
//! `claudepet.exe.sha256` to the release the check reads.

use std::ffi::c_void;
use std::io;
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::*;
use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "emm312/claudepet";
const API_HOST: &str = "api.github.com";
const USER_AGENT: &str = "ClaudePet-Updater";

#[derive(Debug, Clone)]
pub struct Available {
    pub version: String,
    pub exe_url: String,
    pub sha256_url: Option<String>,
    #[allow(dead_code)]
    pub notes: String,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// Blocking HTTPS GET, following redirects. Returns `(status, body)`.
fn https_get(host: &str, path: &str, accept: Option<&str>) -> io::Result<(u32, Vec<u8>)> {
    unsafe {
        let session = WinHttpOpen(
            PCWSTR(wide(USER_AGENT).as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            return Err(io::Error::last_os_error());
        }
        let _s = Handle(session);

        let connect = WinHttpConnect(session, PCWSTR(wide(host).as_ptr()), INTERNET_DEFAULT_HTTPS_PORT, 0);
        if connect.is_null() {
            return Err(io::Error::last_os_error());
        }
        let _c = Handle(connect);

        let request = WinHttpOpenRequest(
            connect,
            PCWSTR(wide("GET").as_ptr()),
            PCWSTR(wide(path).as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null_mut(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            return Err(io::Error::last_os_error());
        }
        let _r = Handle(request);

        if let Some(a) = accept {
            let h = wide(&format!("Accept: {a}"));
            let _ = WinHttpAddRequestHeaders(request, &h[..h.len() - 1], WINHTTP_ADDREQ_FLAG_ADD);
        }

        WinHttpSendRequest(request, None, None, 0, 0, 0)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        WinHttpReceiveResponse(request, std::ptr::null_mut())
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let mut status: u32 = 0;
        let mut len = 4u32;
        let _ = WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut _ as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        );

        let mut body = Vec::new();
        loop {
            let mut avail: u32 = 0;
            if WinHttpQueryDataAvailable(request, &mut avail).is_err() || avail == 0 {
                break;
            }
            let mut buf = vec![0u8; avail as usize];
            let mut read: u32 = 0;
            if WinHttpReadData(request, buf.as_mut_ptr() as *mut c_void, avail, &mut read).is_err() || read == 0 {
                break;
            }
            buf.truncate(read as usize);
            body.extend_from_slice(&buf);
        }
        Ok((status, body))
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut out = [0u8; 32];
    unsafe {
        let _ = BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out);
    }
    out.iter().map(|b| format!("{b:02x}")).collect()
}

fn split_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    match rest.find('/') {
        Some(i) => Some((rest[..i].to_string(), rest[i..].to_string())),
        None => Some((rest.to_string(), "/".to_string())),
    }
}

/// `"v1.2.3"` > `"1.2.0"` etc. Missing components read as 0.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim()
            .trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(candidate), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// Ask GitHub for the latest release; `Some` only if it's newer than us.
pub fn check() -> Option<Available> {
    let (status, body) = https_get(
        API_HOST,
        &format!("/repos/{REPO}/releases/latest"),
        Some("application/vnd.github+json"),
    )
    .ok()?;
    if status != 200 {
        return None; // 404 = no releases yet; anything else = try later
    }
    let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
    let tag = v.get("tag_name")?.as_str()?.to_string();
    if !is_newer(&tag, CURRENT_VERSION) {
        return None;
    }
    let notes = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();
    let assets = v.get("assets")?.as_array()?;
    let asset_url = |name: &str| {
        assets
            .iter()
            .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(name))
            .and_then(|a| a.get("browser_download_url"))
            .and_then(|u| u.as_str())
            .map(String::from)
    };
    Some(Available {
        version: tag,
        exe_url: asset_url("claudepet.exe")?,
        sha256_url: asset_url("claudepet.exe.sha256"),
        notes,
    })
}

pub fn install_dir() -> io::Result<PathBuf> {
    Ok(std::env::current_exe()?
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf())
}

/// Only auto-apply when we're running from a real install (has an uninstaller
/// sibling) - never swap a dev build out from under `cargo run`.
pub fn is_installed() -> bool {
    install_dir()
        .map(|d| d.join("uninstall.exe").exists())
        .unwrap_or(false)
}

/// Download the new exe, verify its SHA-256 (if the release published one), and
/// write it next to the running exe as `claudepet.new.exe`.
pub fn download_and_stage(info: &Available) -> io::Result<PathBuf> {
    let (host, path) = split_url(&info.exe_url)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad asset url"))?;
    let (status, bytes) = https_get(&host, &path, None)?;
    if status != 200 || bytes.len() < 100_000 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("download failed: status {status}, {} bytes", bytes.len()),
        ));
    }

    if let Some(sha_url) = &info.sha256_url {
        if let Some((h, p)) = split_url(sha_url) {
            if let Ok((200, sha_body)) = https_get(&h, &p, None) {
                let want = String::from_utf8_lossy(&sha_body)
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                if !want.is_empty() && want != sha256_hex(&bytes) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "sha256 mismatch"));
                }
            }
        }
    }

    let staged = install_dir()?.join("claudepet.new.exe");
    std::fs::write(&staged, &bytes)?;
    Ok(staged)
}

/// Rename the running exe aside (allowed on Windows), move the staged exe into
/// place, launch it, and exit. The new instance calls [`cleanup`] to delete the
/// old file.
pub fn apply_and_relaunch(staged: &Path) -> io::Result<()> {
    let dir = install_dir()?;
    let live = dir.join("claudepet.exe");
    let old = dir.join("claudepet.old.exe");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&live, &old)?;
    std::fs::rename(staged, &live)?;
    std::process::Command::new(&live).spawn()?;
    std::process::exit(0);
}

/// Startup housekeeping: remove a previous update's `.old.exe` (retrying while
/// the old process finishes exiting) and any orphan `.new.exe`.
pub fn cleanup() {
    let Ok(dir) = install_dir() else { return };
    for _ in 0..30 {
        match std::fs::remove_file(dir.join("claudepet.old.exe")) {
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
    let _ = std::fs::remove_file(dir.join("claudepet.new.exe"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(is_newer("v1.2", "1.1.9")); // short component
    }

    #[test]
    fn url_split() {
        assert_eq!(
            split_url("https://github.com/emm312/claudepet/releases/download/v1/claudepet.exe"),
            Some((
                "github.com".to_string(),
                "/emm312/claudepet/releases/download/v1/claudepet.exe".to_string()
            ))
        );
        assert_eq!(split_url("https://host.example"), Some(("host.example".into(), "/".into())));
        assert_eq!(split_url("ftp://nope"), None);
    }

    #[test]
    fn sha256_matches_known_vector() {
        // SHA-256("abc")
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}

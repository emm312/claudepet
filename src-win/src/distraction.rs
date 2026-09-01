//! Detects whether the user is currently doomscrolling Instagram Reels, so the
//! pet can go make a nuisance of itself. Mirrors
//! `Sources/ClaudePet/Pet/DistractionDetector.swift`.
//!
//! Windows has no permission-free way to read a browser tab's URL, so this is
//! weaker than the macOS version: it matches the **foreground window title** and
//! **process image name** only. Browsers put the page title in the window title
//! ("... • Instagram" / "Reels ..."), which catches the common case.

use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};

/// Process image names that are themselves a distraction regardless of title.
const DISTRACTING_PROCESSES: &[&str] = &["instagram.exe"];

pub struct DistractionDetector;

impl DistractionDetector {
    pub fn new() -> Self {
        DistractionDetector
    }

    pub fn currently_distracted(&self) -> bool {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return false;
        }

        let process = foreground_process_name(hwnd).unwrap_or_default().to_lowercase();
        if DISTRACTING_PROCESSES.iter().any(|p| process == *p) {
            return true;
        }

        let title = window_title(hwnd).to_lowercase();
        title.contains("instagram") && (title.contains("reel") || title.contains("reels"))
    }
}

fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn foreground_process_name(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; MAX_PATH as usize];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        if ok.is_err() || size == 0 {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        Some(
            full.rsplit(['\\', '/'])
                .next()
                .unwrap_or(&full)
                .to_string(),
        )
    }
}

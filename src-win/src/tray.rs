//! System-tray icon + context menu - the pet's only chrome, like
//! `Sources/ClaudePet/UI/StatusItem.swift`. Uses `Shell_NotifyIcon` and a
//! `TrackPopupMenu` popup rebuilt on each open so stats/peers stay current.

use crate::pet::pet_state::PetState;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
    TrackPopupMenu, HMENU, IDI_APPLICATION, MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
};

pub const ID_FEED: u32 = 1;
pub const ID_PLAY: u32 = 2;
pub const ID_CLEAN: u32 = 3;
pub const ID_SEND: u32 = 4;
pub const ID_AUTOSTART: u32 = 5;
pub const ID_QUIT: u32 = 6;
pub const ID_SEARCH: u32 = 7;
pub const ID_AUTOUPDATE: u32 = 8;
pub const ID_UPDATE_NOW: u32 = 9;

pub const TRAY_CALLBACK_MSG: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;
const TRAY_UID: u32 = 0x9A5;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn add_tray_icon(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK_MSG,
        ..Default::default()
    };
    unsafe {
        nid.hIcon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        let tip = wide("ClaudePet");
        nid.szTip[..tip.len()].copy_from_slice(&tip);
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
    nid
}

pub fn remove_tray_icon(nid: &NOTIFYICONDATAW) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, nid);
    }
}

/// Build + show the context menu at the cursor. Shared by the tray icon and the
/// pet's own right-click.
pub fn show_context_menu(
    hwnd: HWND,
    state: &PetState,
    peers: &[String],
    autostart_on: bool,
    auto_update_on: bool,
    pending_update: Option<&str>,
) {
    // Every menu-item string must stay alive until after TrackPopupMenu returns;
    // `AppendMenuW` does not copy immediately in all cases. Own them here.
    let mut keep: Vec<Vec<u16>> = Vec::new();
    let push = |keep: &mut Vec<Vec<u16>>, s: String| -> PCWSTR {
        keep.push(wide(&s));
        PCWSTR(keep.last().unwrap().as_ptr())
    };

    unsafe {
        let menu: HMENU = CreatePopupMenu().unwrap();

        for label in [
            format!("Hunger: {}%", state.hunger as i32),
            format!("Energy: {}%", state.energy as i32),
            format!("Happiness: {}%", state.happiness as i32),
            format!("Cleanliness: {}%", state.cleanliness as i32),
        ] {
            let p = push(&mut keep, label);
            let _ = AppendMenuW(menu, MF_STRING | MF_DISABLED | MF_GRAYED, 0, p);
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        let p = push(&mut keep, "Feed".into());
        let _ = AppendMenuW(menu, MF_STRING, ID_FEED as usize, p);
        let p = push(&mut keep, "Play".into());
        let _ = AppendMenuW(menu, MF_STRING, ID_PLAY as usize, p);
        let p = push(&mut keep, "Clean".into());
        let _ = AppendMenuW(menu, MF_STRING, ID_CLEAN as usize, p);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        let p = push(&mut keep, "Send Message\u{2026}".into());
        let _ = AppendMenuW(menu, MF_STRING, ID_SEND as usize, p);
        let p = push(&mut keep, "Search for pets".into());
        let _ = AppendMenuW(menu, MF_STRING, ID_SEARCH as usize, p);

        let peers_menu: HMENU = CreatePopupMenu().unwrap();
        if peers.is_empty() {
            let p = push(&mut keep, "No pets nearby".into());
            let _ = AppendMenuW(peers_menu, MF_STRING | MF_DISABLED | MF_GRAYED, 0, p);
        } else {
            for name in peers {
                let p = push(&mut keep, name.clone());
                let _ = AppendMenuW(peers_menu, MF_STRING | MF_DISABLED | MF_GRAYED, 0, p);
            }
        }
        let p = push(&mut keep, "Peers".into());
        let _ = AppendMenuW(menu, MF_POPUP, peers_menu.0 as usize, p);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        let checkmark = |on: bool| {
            MF_STRING
                | if on {
                    MF_CHECKED
                } else {
                    windows::Win32::UI::WindowsAndMessaging::MF_UNCHECKED
                }
        };
        let p = push(&mut keep, "Launch at Login".into());
        let _ = AppendMenuW(menu, checkmark(autostart_on), ID_AUTOSTART as usize, p);
        let p = push(&mut keep, "Automatic updates".into());
        let _ = AppendMenuW(menu, checkmark(auto_update_on), ID_AUTOUPDATE as usize, p);

        if let Some(version) = pending_update {
            let p = push(&mut keep, format!("Install update {version} now"));
            let _ = AppendMenuW(menu, MF_STRING, ID_UPDATE_NOW as usize, p);
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        let p = push(&mut keep, "Quit ClaudePet".into());
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT as usize, p);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Docs: foreground the owner window so the menu dismisses on outside click.
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
    }
    drop(keep);
    // Nudge the message queue (TrackPopupMenu quirk).
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(hwnd, 0, WPARAM(0), LPARAM(0));
    }
}

use windows::core::PCWSTR;

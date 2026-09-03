//! A minimal native "customize pet" window: a ◀ ▶ pair to step through
//! `SkinId::ALL` (wrapping) plus one checkbox per `AccessoryId`. Stands in for
//! `UI/CustomizeWindow.swift`. Built the same way `compose.rs` is (a plain
//! Win32 popup, its own nested message loop, result collected into a struct
//! behind `GWLP_USERDATA`) rather than duplicating the layered-window DIB
//! pixel-preview machinery for a settings dialog - the label updates
//! instantly on each ◀ ▶ click, and the change is applied to the real,
//! persisted pet (`Runtime::set_skin`/`set_accessory`, visible on screen right
//! behind this window) the moment "Done" is clicked.

use crate::pet::sprites::{AccessoryId, SkinId};
use std::collections::HashSet;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetSysColorBrush, COLOR_3DFACE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;

struct CustomizeState {
    skin: SkinId,
    accessories: HashSet<AccessoryId>,
    skin_label: HWND,
    accessory_checkboxes: Vec<(AccessoryId, HWND)>,
    result: Option<(SkinId, HashSet<AccessoryId>)>,
    done: bool,
}

const ID_PREV: isize = 101;
const ID_NEXT: isize = 102;
const ID_DONE: isize = 103;
const ID_ACCESSORY_BASE: isize = 200;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Show the customize window modally relative to `owner`. Always returns the
/// (possibly unchanged) selection - there's no discard/cancel concept since
/// every ◀ ▶ click already updated the dialog's own live selection.
pub fn present(owner: HWND, current_skin: SkinId, current_accessories: &HashSet<AccessoryId>) -> Option<(SkinId, HashSet<AccessoryId>)> {
    unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        let class_name = w!("ClaudePetCustomizeClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(customize_proc),
            hInstance: hinst.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_3DFACE),
            ..Default::default()
        };
        RegisterClassW(&wc); // ignore "already registered"

        let mut st = Box::new(CustomizeState {
            skin: current_skin,
            accessories: current_accessories.clone(),
            skin_label: HWND::default(),
            accessory_checkboxes: Vec::new(),
            result: None,
            done: false,
        });

        let (ww, wh) = (280i32, 90i32 + 24 * AccessoryId::ALL.len() as i32 + 40);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_DLGMODALFRAME,
            class_name,
            w!("Customize Pet"),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            (sw - ww) / 2,
            (sh - wh) / 2,
            ww,
            wh,
            owner,
            None,
            hinst,
            Some(&mut *st as *mut _ as *const _),
        )
        .ok()?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);

        let mut msg = MSG::default();
        while !st.done && GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let _ = DestroyWindow(hwnd);
        st.result.take()
    }
}

unsafe fn update_skin_label(st: &CustomizeState) {
    let label = wide(st.skin.display_name());
    let _ = SetWindowTextW(st.skin_label, PCWSTR(label.as_ptr()));
}

unsafe extern "system" fn customize_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            let st_ptr = (*cs).lpCreateParams as *mut CustomizeState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, st_ptr as isize);
            let st = &mut *st_ptr;
            let hinst = (*cs).hInstance;

            let prev = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("\u{25c0}"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                16, 16, 36, 28, hwnd, HMENU(ID_PREV as *mut _), hinst, None,
            );
            let _ = prev;
            let label = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                PCWSTR(wide(st.skin.display_name()).as_ptr()),
                WS_CHILD | WS_VISIBLE,
                60, 20, 160, 20, hwnd, None, hinst, None,
            )
            .unwrap_or_default();
            st.skin_label = label;
            let _ = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("\u{25b6}"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                228, 16, 36, 28, hwnd, HMENU(ID_NEXT as *mut _), hinst, None,
            );

            let mut y = 56i32;
            for accessory in AccessoryId::ALL {
                let cb = CreateWindowExW(
                    Default::default(),
                    w!("BUTTON"),
                    PCWSTR(wide(accessory.display_name()).as_ptr()),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                    16, y, 240, 20, hwnd, HMENU((ID_ACCESSORY_BASE + accessory as isize) as *mut _), hinst, None,
                )
                .unwrap_or_default();
                SendMessageW(cb, BM_SETCHECK, WPARAM(if st.accessories.contains(&accessory) { 1 } else { 0 }), LPARAM(0));
                st.accessory_checkboxes.push((accessory, cb));
                y += 24;
            }

            let _ = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Done"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
                (280 - 76) / 2, y + 10, 76, 28, hwnd, HMENU(ID_DONE as *mut _), hinst, None,
            );
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as isize;
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CustomizeState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &mut *st_ptr;
            match id {
                ID_PREV => {
                    let all = SkinId::ALL;
                    let idx = all.iter().position(|s| *s == st.skin).unwrap_or(0);
                    st.skin = all[(idx + all.len() - 1) % all.len()];
                    update_skin_label(st);
                    LRESULT(0)
                }
                ID_NEXT => {
                    let all = SkinId::ALL;
                    let idx = all.iter().position(|s| *s == st.skin).unwrap_or(0);
                    st.skin = all[(idx + 1) % all.len()];
                    update_skin_label(st);
                    LRESULT(0)
                }
                ID_DONE => {
                    let accessories: HashSet<AccessoryId> = st
                        .accessory_checkboxes
                        .iter()
                        .filter(|(_, cb)| SendMessageW(*cb, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1)
                        .map(|(id, _)| *id)
                        .collect();
                    st.result = Some((st.skin, accessories));
                    st.done = true;
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_CLOSE => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CustomizeState;
            if !st_ptr.is_null() {
                (*st_ptr).done = true;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

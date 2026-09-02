//! A minimal native "send a letter" window: a peer picker + a multiline text box
//! + Send/Cancel. Stands in for `UI/MessageComposer.swift` + `UI/LetterWindow
//! .swift` (the macOS version is a custom letter-themed panel; this is a plain
//! Win32 dialog, same inputs and outputs).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetSysColorBrush, COLOR_3DFACE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WC_COMBOBOXW;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

struct ComposeState {
    peers: Vec<String>,
    single_peer: Option<String>,
    edit: HWND,
    combo: HWND,
    express_cb: HWND,
    result: Option<(String, String, bool)>,
    done: bool,
}

const ID_SEND: isize = 101;
const ID_CANCEL: isize = 102;
const ID_EXPRESS: isize = 103;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Show the compose window modally relative to `owner`. Returns
/// `(text, peer, express)` or `None` if cancelled / no peers.
pub fn present(owner: HWND, peers: &[String]) -> Option<(String, String, bool)> {
    if peers.is_empty() {
        unsafe {
            MessageBoxW(
                owner,
                w!("Nothing responded nearby. Make sure the other pet is running and on the same network."),
                w!("No pets nearby"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        return None;
    }

    unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        let class_name = w!("ClaudePetComposeClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(compose_proc),
            hInstance: hinst.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_3DFACE),
            ..Default::default()
        };
        RegisterClassW(&wc); // ignore "already registered"

        let mut st = Box::new(ComposeState {
            peers: peers.to_vec(),
            single_peer: if peers.len() == 1 { Some(peers[0].clone()) } else { None },
            edit: HWND::default(),
            combo: HWND::default(),
            express_cb: HWND::default(),
            result: None,
            done: false,
        });

        let (ww, wh) = (380i32, 264i32);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_DLGMODALFRAME,
            class_name,
            w!("Send a Letter"),
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

        // Modal: disable the owner while composing.
        let _ = EnableWindow(owner, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(st.edit);

        let mut msg = MSG::default();
        while !st.done && GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let _ = EnableWindow(owner, true);
        let _ = SetForegroundWindow(owner);
        let _ = DestroyWindow(hwnd);

        st.result.take()
    }
}

unsafe extern "system" fn compose_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            let st_ptr = (*cs).lpCreateParams as *mut ComposeState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, st_ptr as isize);
            let st = &mut *st_ptr;
            let hinst = (*cs).hInstance;

            let _label = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!("To:"),
                WS_CHILD | WS_VISIBLE,
                16, 14, 40, 20, hwnd, None, hinst, None,
            );

            if let Some(peer) = &st.single_peer {
                let _ = CreateWindowExW(
                    Default::default(),
                    w!("STATIC"),
                    PCWSTR(wide(peer).as_ptr()),
                    WS_CHILD | WS_VISIBLE,
                    56, 14, 300, 20, hwnd, None, hinst, None,
                );
            } else {
                let combo = CreateWindowExW(
                    Default::default(),
                    WC_COMBOBOXW,
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0),
                    56, 10, 300, 200, hwnd, None, hinst, None,
                )
                .unwrap_or_default();
                for name in &st.peers {
                    let wn = wide(name);
                    SendMessageW(combo, CB_ADDSTRING, WPARAM(0), LPARAM(wn.as_ptr() as isize));
                }
                SendMessageW(combo, CB_SETCURSEL, WPARAM(0), LPARAM(0));
                st.combo = combo;
            }

            let edit = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL
                    | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32),
                16, 44, 340, 104, hwnd, None, hinst, None,
            )
            .unwrap_or_default();
            st.edit = edit;

            let express_cb = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Send by horse (express) \u{1F40E}"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                16, 156, 260, 22, hwnd, HMENU(ID_EXPRESS as *mut _), hinst, None,
            )
            .unwrap_or_default();
            st.express_cb = express_cb;

            let _ = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Send"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
                190, 190, 76, 28, hwnd, HMENU(ID_SEND as *mut _), hinst, None,
            );
            let _ = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Cancel"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                278, 190, 76, 28, hwnd, HMENU(ID_CANCEL as *mut _), hinst, None,
            );
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as isize;
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ComposeState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &mut *st_ptr;
            match id {
                ID_SEND => {
                    let text = get_edit_text(st.edit);
                    let peer = st.single_peer.clone().or_else(|| combo_selection(st.combo, &st.peers));
                    let express = !st.express_cb.0.is_null()
                        && SendMessageW(st.express_cb, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;
                    if let (false, Some(peer)) = (text.trim().is_empty(), peer) {
                        st.result = Some((text.trim().to_string(), peer, express));
                    }
                    st.done = true;
                    LRESULT(0)
                }
                ID_CANCEL => {
                    st.done = true;
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_CLOSE => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ComposeState;
            if !st_ptr.is_null() {
                (*st_ptr).done = true;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn get_edit_text(edit: HWND) -> String {
    let len = GetWindowTextLengthW(edit);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(edit, &mut buf);
    String::from_utf16_lossy(&buf[..n as usize])
}

unsafe fn combo_selection(combo: HWND, peers: &[String]) -> Option<String> {
    if combo.0.is_null() {
        return peers.first().cloned();
    }
    let idx = SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
    if idx < 0 {
        return peers.first().cloned();
    }
    peers.get(idx as usize).cloned()
}

#[allow(dead_code)]
fn _unused(_: RECT) {}

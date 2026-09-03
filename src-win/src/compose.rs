//! A minimal native "send a letter" window: a peer picker + a multiline text box
//! + Send/Cancel. Stands in for `UI/MessageComposer.swift` + `UI/LetterWindow
//! .swift` (the macOS version is a custom letter-themed panel; this is a plain
//! Win32 dialog, same inputs and outputs).
//!
//! Recipient selection is one checkbox per known peer (all unchecked by default,
//! so the user opts each recipient in), mirroring `LetterWindow.swift`'s checkbox
//! row - not a single-selection combo box, which made sending to more than one
//! peer at once impossible.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetSysColorBrush, COLOR_3DFACE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

struct ComposeState {
    peers: Vec<String>,
    single_peer: Option<String>,
    peer_checkboxes: Vec<HWND>,
    edit: HWND,
    express_cb: HWND,
    result: Option<(String, Vec<String>, bool)>,
    done: bool,
}

const ID_SEND: isize = 101;
const ID_CANCEL: isize = 102;
const ID_EXPRESS: isize = 103;
const ID_PEER_BASE: isize = 200;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Show the compose window modally relative to `owner`. Returns
/// `(text, peers, express)` or `None` if cancelled / no peers.
pub fn present(owner: HWND, peers: &[String]) -> Option<(String, Vec<String>, bool)> {
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

        let single_peer = if peers.len() == 1 { Some(peers[0].clone()) } else { None };
        // Extra vertical room for one checkbox row per peer when there's more
        // than one to choose from.
        let peer_rows_h = if single_peer.is_some() { 0 } else { 22 * peers.len() as i32 };
        let mut st = Box::new(ComposeState {
            peers: peers.to_vec(),
            single_peer,
            peer_checkboxes: Vec::new(),
            edit: HWND::default(),
            express_cb: HWND::default(),
            result: None,
            done: false,
        });

        let (ww, wh) = (380i32, 264i32 + peer_rows_h);
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

            let mut peer_rows_h = 0i32;
            if let Some(peer) = &st.single_peer {
                let _ = CreateWindowExW(
                    Default::default(),
                    w!("STATIC"),
                    PCWSTR(wide(peer).as_ptr()),
                    WS_CHILD | WS_VISIBLE,
                    56, 14, 300, 20, hwnd, None, hinst, None,
                );
            } else {
                for (i, name) in st.peers.iter().enumerate() {
                    let cb = CreateWindowExW(
                        Default::default(),
                        w!("BUTTON"),
                        PCWSTR(wide(name).as_ptr()),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                        56, 12 + 22 * i as i32, 300, 20,
                        hwnd, HMENU((ID_PEER_BASE + i as isize) as *mut _), hinst, None,
                    )
                    .unwrap_or_default();
                    // Unchecked by default - the user opts each recipient in
                    // rather than accidentally broadcasting to everyone nearby.
                    SendMessageW(cb, BM_SETCHECK, WPARAM(0), LPARAM(0));
                    st.peer_checkboxes.push(cb);
                }
                peer_rows_h = 22 * st.peers.len() as i32;
            }

            let edit_y = 44 + peer_rows_h;
            let edit = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL
                    | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32),
                16, edit_y, 340, 104, hwnd, None, hinst, None,
            )
            .unwrap_or_default();
            st.edit = edit;

            let express_y = edit_y + 112;
            let express_cb = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Send by horse (express) \u{1F40E}"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                16, express_y, 260, 22, hwnd, HMENU(ID_EXPRESS as *mut _), hinst, None,
            )
            .unwrap_or_default();
            st.express_cb = express_cb;

            let buttons_y = express_y + 34;
            let _ = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Send"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
                190, buttons_y, 76, 28, hwnd, HMENU(ID_SEND as *mut _), hinst, None,
            );
            let _ = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Cancel"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                278, buttons_y, 76, 28, hwnd, HMENU(ID_CANCEL as *mut _), hinst, None,
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
                    let peers = selected_peers(st);
                    let express = !st.express_cb.0.is_null()
                        && SendMessageW(st.express_cb, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;
                    if !text.trim().is_empty() && !peers.is_empty() {
                        st.result = Some((text.trim().to_string(), peers, express));
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

pub(crate) unsafe fn get_edit_text(edit: HWND) -> String {
    let len = GetWindowTextLengthW(edit);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(edit, &mut buf);
    String::from_utf16_lossy(&buf[..n as usize])
}

unsafe fn selected_peers(st: &ComposeState) -> Vec<String> {
    if let Some(peer) = &st.single_peer {
        return vec![peer.clone()];
    }
    st.peers
        .iter()
        .zip(st.peer_checkboxes.iter())
        .filter(|(_, cb)| SendMessageW(**cb, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1)
        .map(|(name, _)| name.clone())
        .collect()
}

#[allow(dead_code)]
fn _unused(_: RECT) {}

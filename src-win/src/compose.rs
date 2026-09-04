//! A minimal native "send a letter" window: a peer picker + a multiline text box
//! + Send/Cancel. Stands in for `UI/MessageComposer.swift` + `UI/LetterWindow
//! .swift` (the macOS version is a custom letter-themed panel; this now shares
//! that same paper-and-clay visual language via GDI painting, same as
//! `letter.rs`'s reader - plain checkbox/edit controls on a themed grey dialog
//! face read as an afterthought next to it).
//!
//! Recipient selection is one checkbox per known peer (all unchecked by default,
//! so the user opts each recipient in), mirroring `LetterWindow.swift`'s checkbox
//! row - not a single-selection combo box, which made sending to more than one
//! peer at once impossible.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FillRect, GetStockObject, LineTo, MoveToEx, RoundRect, SelectObject, SetBkColor,
    SetBkMode, SetTextColor, TextOutW, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL,
    HBRUSH, HDC, HFONT, HGDIOBJ, HOLLOW_BRUSH, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

// Same paper/clay theme as `letter.rs` (kept as a private copy rather than a
// shared module so this file's already-verified-on-a-Windows-box sibling isn't
// touched - see the "Not yet compiled" note in CLAUDE.md).
const PAPER: u32 = rgb(246, 240, 229);
const PAPER_SHADOW: u32 = rgb(217, 204, 186);
const INK: u32 = rgb(41, 31, 23);
const INK_FAINT: u32 = rgb(122, 104, 88);
const CLAY: u32 = rgb(198, 116, 88);

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

struct ComposeState {
    peers: Vec<String>,
    single_peer: Option<String>,
    to_label: HWND,
    peer_checkboxes: Vec<HWND>,
    edit: HWND,
    express_cb: HWND,
    send_btn: HWND,
    cancel_btn: HWND,
    paper_brush: HBRUSH,
    body_font: HFONT,
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

/// Vertical space the painted title/seal/rule occupy above the form controls,
/// mirroring `letter.rs`'s title area (its body starts at y=84 under a rule at
/// y=52). Every control y-coordinate below is the old flat-dialog layout
/// shifted down by this much.
const TOP: i32 = 46;

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
            to_label: HWND::default(),
            peer_checkboxes: Vec::new(),
            edit: HWND::default(),
            express_cb: HWND::default(),
            send_btn: HWND::default(),
            cancel_btn: HWND::default(),
            paper_brush: CreateSolidBrush(COLORREF(PAPER)),
            body_font: HFONT::default(),
            result: None,
            done: false,
        });

        let (ww, wh) = (380i32, TOP + 264i32 + peer_rows_h);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_DLGMODALFRAME,
            class_name,
            w!("ClaudePet"), // themed title is painted inside; keep the caption plain
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

            st.body_font = CreateFontW(
                -14, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, 0, 0, 0, 0, 0, w!("Segoe UI"),
            );

            let to_label = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!("To:"),
                WS_CHILD | WS_VISIBLE,
                20, TOP + 14, 32, 20, hwnd, None, hinst, None,
            )
            .unwrap_or_default();
            SendMessageW(to_label, WM_SETFONT, WPARAM(st.body_font.0 as usize), LPARAM(1));
            st.to_label = to_label;

            let mut peer_rows_h = 0i32;
            if let Some(peer) = &st.single_peer {
                let name = CreateWindowExW(
                    Default::default(),
                    w!("STATIC"),
                    PCWSTR(wide(peer).as_ptr()),
                    WS_CHILD | WS_VISIBLE,
                    56, TOP + 14, 300, 20, hwnd, None, hinst, None,
                )
                .unwrap_or_default();
                SendMessageW(name, WM_SETFONT, WPARAM(st.body_font.0 as usize), LPARAM(1));
            } else {
                for (i, name) in st.peers.iter().enumerate() {
                    let cb = CreateWindowExW(
                        Default::default(),
                        w!("BUTTON"),
                        PCWSTR(wide(name).as_ptr()),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                        56, TOP + 12 + 22 * i as i32, 300, 20,
                        hwnd, HMENU((ID_PEER_BASE + i as isize) as *mut _), hinst, None,
                    )
                    .unwrap_or_default();
                    SendMessageW(cb, WM_SETFONT, WPARAM(st.body_font.0 as usize), LPARAM(1));
                    // Unchecked by default - the user opts each recipient in
                    // rather than accidentally broadcasting to everyone nearby.
                    SendMessageW(cb, BM_SETCHECK, WPARAM(0), LPARAM(0));
                    st.peer_checkboxes.push(cb);
                }
                peer_rows_h = 22 * st.peers.len() as i32;
            }

            let edit_y = TOP + 44 + peer_rows_h;
            let edit = CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL
                    | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32),
                20, edit_y, 340, 104, hwnd, None, hinst, None,
            )
            .unwrap_or_default();
            SendMessageW(edit, WM_SETFONT, WPARAM(st.body_font.0 as usize), LPARAM(1));
            st.edit = edit;

            let express_y = edit_y + 112;
            let express_cb = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Send by horse (express) \u{1F40E}"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                20, express_y, 260, 22, hwnd, HMENU(ID_EXPRESS as *mut _), hinst, None,
            )
            .unwrap_or_default();
            SendMessageW(express_cb, WM_SETFONT, WPARAM(st.body_font.0 as usize), LPARAM(1));
            st.express_cb = express_cb;

            let buttons_y = express_y + 34;
            st.send_btn = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Send"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32 | BS_DEFPUSHBUTTON as u32),
                186, buttons_y, 84, 30, hwnd, HMENU(ID_SEND as *mut _), hinst, None,
            )
            .unwrap_or_default();
            st.cancel_btn = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Cancel"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                278, buttons_y, 78, 30, hwnd, HMENU(ID_CANCEL as *mut _), hinst, None,
            )
            .unwrap_or_default();
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ComposeState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            FillRect(HDC(wparam.0 as *mut _), &rc, (*st_ptr).paper_brush);
            LRESULT(1)
        }

        WM_PAINT => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ComposeState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &*st_ptr;
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            FillRect(hdc, &rc, st.paper_brush);

            // Clay rounded card border, matching letter.rs's reader.
            let clay_pen = CreatePen(PS_SOLID, 1, COLORREF(CLAY));
            let hollow = GetStockObject(HOLLOW_BRUSH);
            let old_pen = SelectObject(hdc, HGDIOBJ(clay_pen.0));
            let old_brush = SelectObject(hdc, hollow);
            let _ = RoundRect(hdc, rc.left + 1, rc.top + 1, rc.right - 1, rc.bottom - 1, 20, 20);

            // Wax-seal dot up by the title.
            let seal = CreateSolidBrush(COLORREF(CLAY));
            let ob = SelectObject(hdc, HGDIOBJ(seal.0));
            let _ = Ellipse(hdc, rc.right - 40, 20, rc.right - 26, 34);
            SelectObject(hdc, ob);
            let _ = DeleteObject(HGDIOBJ(seal.0));

            // Faint rule under the title.
            let rule_pen = CreatePen(PS_SOLID, 1, COLORREF(PAPER_SHADOW));
            SelectObject(hdc, HGDIOBJ(rule_pen.0));
            let _ = MoveToEx(hdc, 22, TOP + 6, None);
            let _ = LineTo(hdc, rc.right - 22, TOP + 6);

            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(HGDIOBJ(clay_pen.0));
            let _ = DeleteObject(HGDIOBJ(rule_pen.0));

            SetBkMode(hdc, TRANSPARENT);

            // Serif title.
            let title_font = CreateFontW(
                -20, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0, 0, 0, 0, 0, 0, w!("Georgia"),
            );
            let of = SelectObject(hdc, HGDIOBJ(title_font.0));
            SetTextColor(hdc, COLORREF(CLAY));
            let title = "Send a Letter";
            let tw: Vec<u16> = title.encode_utf16().collect();
            let _ = TextOutW(hdc, 22, 14, &tw);
            SelectObject(hdc, of);
            let _ = DeleteObject(HGDIOBJ(title_font.0));

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        // Paper background behind labels/checkboxes and the edit box, so the
        // native controls sit on the card instead of a mismatched grey face.
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ComposeState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &*st_ptr;
            let hdc = HDC(wparam.0 as *mut _);
            let ctl = HWND(lparam.0 as *mut _);
            let text_color = if ctl == st.to_label { INK_FAINT } else { INK };
            SetTextColor(hdc, COLORREF(text_color));
            SetBkColor(hdc, COLORREF(PAPER));
            LRESULT(st.paper_brush.0 as isize)
        }

        WM_DRAWITEM => {
            let dis = lparam.0 as *const DRAWITEMSTRUCT;
            if dis.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let is_send = (*dis).CtlID == ID_SEND as u32;
            let is_cancel = (*dis).CtlID == ID_CANCEL as u32;
            if !is_send && !is_cancel {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let hdc = (*dis).hDC;
            let rc = (*dis).rcItem;
            let r = rc.bottom - rc.top;

            if is_send {
                // Filled clay pill, matching the Reply button in letter.rs.
                let fill = CreateSolidBrush(COLORREF(CLAY));
                let pen = CreatePen(PS_SOLID, 1, COLORREF(CLAY));
                let ob = SelectObject(hdc, HGDIOBJ(fill.0));
                let op = SelectObject(hdc, HGDIOBJ(pen.0));
                let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, r, r);
                SelectObject(hdc, ob);
                SelectObject(hdc, op);
                let _ = DeleteObject(HGDIOBJ(fill.0));
                let _ = DeleteObject(HGDIOBJ(pen.0));
                SetTextColor(hdc, COLORREF(rgb(255, 255, 255)));
            } else {
                // Outline pill for Cancel - present but secondary.
                let hollow = GetStockObject(HOLLOW_BRUSH);
                let pen = CreatePen(PS_SOLID, 1, COLORREF(PAPER_SHADOW));
                let ob = SelectObject(hdc, hollow);
                let op = SelectObject(hdc, HGDIOBJ(pen.0));
                let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, r, r);
                SelectObject(hdc, ob);
                SelectObject(hdc, op);
                let _ = DeleteObject(HGDIOBJ(pen.0));
                SetTextColor(hdc, COLORREF(INK_FAINT));
            }

            SetBkMode(hdc, TRANSPARENT);
            let label = if is_send { "Send" } else { "Cancel" };
            let mut lw: Vec<u16> = label.encode_utf16().collect();
            let mut lr = rc;
            DrawTextW(hdc, &mut lw, &mut lr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            LRESULT(1)
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

        WM_DESTROY => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ComposeState;
            if !st_ptr.is_null() {
                let _ = DeleteObject(HGDIOBJ((*st_ptr).paper_brush.0));
                let _ = DeleteObject(HGDIOBJ((*st_ptr).body_font.0));
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

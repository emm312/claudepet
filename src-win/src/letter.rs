//! The "A Letter Arrived" reader: a paper-and-clay themed panel that shows a
//! delivered `PetMessage` and offers an inline reply. Mirrors the *read* mode of
//! `Sources/ClaudePet/UI/LetterWindow.swift` - its visual language (paper fill,
//! clay border + wax seal, serif title, "From:" line, pill Reply button), not
//! its borderless/rounded AppKit chrome. Composing a fresh letter is still
//! `compose.rs`; this only opens when the pet's carried mail is clicked.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FillRect, GetStockObject, InvalidateRect, LineTo, MoveToEx, RoundRect, SelectObject, SetBkColor,
    SetBkMode, SetTextColor, TextOutW, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL,
    HBRUSH, HDC, HGDIOBJ, HOLLOW_BRUSH, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

/// `EM_SETREADONLY` - not surfaced by the `windows` crate's WindowsAndMessaging
/// module; the raw edit-control message value.
const EM_SETREADONLY: u32 = 0x00CF;

use crate::compose::get_edit_text;
use crate::net::PetMessage;

// Letter theme - the same tones as `LetterTheme` in LetterWindow.swift, which
// are keyed off the pet sprite's own clay body colour.
const PAPER: u32 = rgb(246, 240, 229);
const PAPER_SHADOW: u32 = rgb(217, 204, 186);
const INK: u32 = rgb(41, 31, 23);
const INK_FAINT: u32 = rgb(122, 104, 88);
const CLAY: u32 = rgb(198, 116, 88);

const ID_OK: isize = 201;
const ID_REPLY: isize = 202;
const ID_BODY: isize = 203;

/// Approximate client size (the caption + frame trim ~8x37 off the 380x300
/// window); child layout is positioned against this, the painted card against
/// the real `GetClientRect`.
const CW: i32 = 372;
const CH: i32 = 261;

struct ReadState {
    sender: String,
    edit: HWND,
    ok_btn: HWND,
    reply_btn: HWND,
    paper_brush: HBRUSH,
    /// false: showing the arrived letter. true: composing a reply in place.
    replying: bool,
    result: Option<String>,
    done: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Show the letter reader modally over `owner`. Returns `Some(reply_text)` if
/// the reader wrote and sent a reply, else `None`.
pub fn present_read(owner: HWND, msg: &PetMessage) -> Option<String> {
    unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        let class_name = w!("ClaudePetLetterClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(letter_proc),
            hInstance: hinst.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc); // ignore "already registered"

        let mut st = Box::new(ReadState {
            sender: msg.sender_name.clone(),
            edit: HWND::default(),
            ok_btn: HWND::default(),
            reply_btn: HWND::default(),
            paper_brush: CreateSolidBrush(COLORREF(PAPER)),
            replying: false,
            result: None,
            done: false,
        });

        let (ww, wh) = (380i32, 300i32);
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

        // Seed the body with the message text, read-only for now.
        let body = wide(&msg.text);
        let _ = SetWindowTextW(st.edit, PCWSTR(body.as_ptr()));
        SendMessageW(st.edit, EM_SETREADONLY, WPARAM(1), LPARAM(0));

        let _ = EnableWindow(owner, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);

        let mut m = MSG::default();
        while !st.done && GetMessageW(&mut m, None, 0, 0).as_bool() {
            if !IsDialogMessageW(hwnd, &m).as_bool() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }

        let _ = EnableWindow(owner, true);
        let _ = SetForegroundWindow(owner);
        let _ = DestroyWindow(hwnd);
        st.result.take()
    }
}

unsafe extern "system" fn letter_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            let st_ptr = (*cs).lpCreateParams as *mut ReadState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, st_ptr as isize);
            let st = &mut *st_ptr;
            let hinst = (*cs).hInstance;

            // Body text area - borderless so it sits on the paper like the Mac
            // letter rather than in a sunken box.
            let edit = CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32),
                20,
                84,
                CW - 40,
                CH - 84 - 52,
                hwnd,
                HMENU(ID_BODY as *mut _),
                hinst,
                None,
            )
            .unwrap_or_default();
            st.edit = edit;

            st.ok_btn = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("OK"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                20,
                CH - 40,
                74,
                26,
                hwnd,
                HMENU(ID_OK as *mut _),
                hinst,
                None,
            )
            .unwrap_or_default();

            let reply = CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                w!("Reply"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                CW - 20 - 104,
                CH - 42,
                104,
                30,
                hwnd,
                HMENU(ID_REPLY as *mut _),
                hinst,
                None,
            )
            .unwrap_or_default();
            st.reply_btn = reply;
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ReadState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            FillRect(HDC(wparam.0 as *mut _), &rc, (*st_ptr).paper_brush);
            LRESULT(1)
        }

        WM_PAINT => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ReadState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &*st_ptr;
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            FillRect(hdc, &rc, st.paper_brush);

            // Clay rounded card border.
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
            let _ = MoveToEx(hdc, 22, 52, None);
            let _ = LineTo(hdc, rc.right - 22, 52);

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
            let title = if st.replying { "Send a Reply" } else { "A Letter Arrived" };
            let tw: Vec<u16> = title.encode_utf16().collect();
            let _ = TextOutW(hdc, 22, 18, &tw);
            SelectObject(hdc, of);
            let _ = DeleteObject(HGDIOBJ(title_font.0));

            // Serif italic "From: / To:" line.
            let meta_font = CreateFontW(
                -14, 0, 0, 0, FW_NORMAL.0 as i32, 1, 0, 0, 0, 0, 0, 0, 0, w!("Georgia"),
            );
            let of2 = SelectObject(hdc, HGDIOBJ(meta_font.0));
            SetTextColor(hdc, COLORREF(INK_FAINT));
            let lead = if st.replying { "To:" } else { "From:" };
            let meta: Vec<u16> = format!("{lead}  {}", st.sender).encode_utf16().collect();
            let _ = TextOutW(hdc, 22, 58, &meta);
            SelectObject(hdc, of2);
            let _ = DeleteObject(HGDIOBJ(meta_font.0));

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        // Paper background behind the body text, whether it's read-only
        // (WM_CTLCOLORSTATIC) or editable for a reply (WM_CTLCOLOREDIT).
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ReadState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let hdc = HDC(wparam.0 as *mut _);
            SetTextColor(hdc, COLORREF(INK));
            SetBkColor(hdc, COLORREF(PAPER));
            LRESULT((*st_ptr).paper_brush.0 as isize)
        }

        WM_DRAWITEM => {
            let dis = lparam.0 as *const DRAWITEMSTRUCT;
            if dis.is_null() || (*dis).CtlID != ID_REPLY as u32 {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ReadState;
            let replying = !st_ptr.is_null() && (*st_ptr).replying;
            let hdc = (*dis).hDC;
            let rc = (*dis).rcItem;

            let fill = CreateSolidBrush(COLORREF(CLAY));
            let pen = CreatePen(PS_SOLID, 1, COLORREF(CLAY));
            let ob = SelectObject(hdc, HGDIOBJ(fill.0));
            let op = SelectObject(hdc, HGDIOBJ(pen.0));
            let r = rc.bottom - rc.top;
            let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, r, r);
            SelectObject(hdc, ob);
            SelectObject(hdc, op);
            let _ = DeleteObject(HGDIOBJ(fill.0));
            let _ = DeleteObject(HGDIOBJ(pen.0));

            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(rgb(255, 255, 255)));
            let label = if replying { "Send" } else { "Reply" };
            let mut lw: Vec<u16> = label.encode_utf16().collect();
            let mut lr = rc;
            DrawTextW(hdc, &mut lw, &mut lr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            LRESULT(1)
        }

        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as isize;
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ReadState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &mut *st_ptr;
            match id {
                ID_REPLY => {
                    if st.replying {
                        // "Send": hand back the trimmed reply if there is one.
                        let text = get_edit_text(st.edit);
                        if !text.trim().is_empty() {
                            st.result = Some(text.trim().to_string());
                            st.done = true;
                        }
                    } else {
                        // "Reply": clear the body, make it writable, retitle, and
                        // flip OK -> Cancel like the mac panel does.
                        st.replying = true;
                        SendMessageW(st.edit, EM_SETREADONLY, WPARAM(0), LPARAM(0));
                        let _ = SetWindowTextW(st.edit, w!(""));
                        let _ = SetWindowTextW(st.reply_btn, w!("Send"));
                        let _ = SetWindowTextW(st.ok_btn, w!("Cancel"));
                        let _ = InvalidateRect(hwnd, None, true);
                        let _ = SetFocus(st.edit);
                    }
                    LRESULT(0)
                }
                ID_OK => {
                    st.done = true; // result stays None
                    LRESULT(0)
                }
                // Esc via IsDialogMessageW.
                2 => {
                    st.done = true;
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }

        WM_CLOSE => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ReadState;
            if !st_ptr.is_null() {
                (*st_ptr).done = true;
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ReadState;
            if !st_ptr.is_null() {
                let _ = DeleteObject(HGDIOBJ((*st_ptr).paper_brush.0));
            }
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

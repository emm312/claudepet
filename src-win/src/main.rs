//! ClaudePet for Windows - entry point and Win32 shell.
//!
//! Creates one per-pixel-alpha overlay covering the primary monitor, a tray
//! icon, and an adaptive tick timer, then drives `Runtime` from `WM_TIMER`.
//! Mirrors the wiring in `Sources/ClaudePet/main.swift` + `Runtime.swift`.

#![windows_subsystem = "windows"]

mod autostart;
mod compose;
mod distraction;
mod geometry;
mod layered_window;
mod ledges;
mod net;
mod pet;
mod render;
mod runtime;
mod tray;

use layered_window::LayeredWindow;
use net::mdns_udp::MdnsUdpTransport;
use pet::sprites::CLIPS;
use runtime::{Runtime, ZOOM};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

const TICK_TIMER_ID: usize = 1;
const WM_APP_COMPOSE: u32 = WM_APP + 2;
const FAST_INTERVAL_MS: u32 = 33; // ~30 fps
const IDLE_INTERVAL_MS: u32 = 125; // 8 fps

struct App {
    window: LayeredWindow,
    runtime: Runtime,
    tray: windows::Win32::UI::Shell::NOTIFYICONDATAW,
    fast: bool,
    mouse_down: bool,
    moved: bool,
    down_pos: POINT,
}

fn main() {
    unsafe {
        let hinst = GetModuleHandleW(None).expect("GetModuleHandleW");
        let class_name = w!("ClaudePetOverlayClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let (sw, sh) = geometry::primary_screen_size();

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("ClaudePet"),
            WS_POPUP,
            0,
            0,
            sw,
            sh,
            None,
            None,
            hinst,
            None,
        )
        .expect("CreateWindowExW");

        let window = LayeredWindow::attach(hwnd, sw, sh);

        let transport: Box<dyn net::PeerTransport> = match MdnsUdpTransport::new() {
            Ok(t) => Box::new(t),
            Err(e) => {
                eprintln!("ClaudePet: could not open a UDP socket ({e}); messaging disabled.");
                Box::new(NullTransport::default())
            }
        };
        let runtime = Runtime::new(transport, hwnd.0 as isize);
        let tray = tray::add_tray_icon(hwnd);

        let mut app = Box::new(App {
            window,
            runtime,
            tray,
            fast: false,
            mouse_down: false,
            moved: false,
            down_pos: POINT::default(),
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *app as *mut App as isize);

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        render_frame(&mut app);
        SetTimer(hwnd, TICK_TIMER_ID, IDLE_INTERVAL_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        KillTimer(hwnd, TICK_TIMER_ID).ok();
        tray::remove_tray_icon(&app.tray);
    }
}

unsafe fn app_from(hwnd: HWND) -> Option<*mut App> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if p.is_null() {
        None
    } else {
        Some(p)
    }
}

fn cursor_pos() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(app_ptr) = app_from(hwnd) else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };

    match msg {
        WM_TIMER if wparam.0 == TICK_TIMER_ID => {
            let app = &mut *app_ptr;
            app.runtime.tick();

            // Adapt the timer cadence to what the pet is doing.
            let fast = app.runtime.is_fast_motion();
            if fast != app.fast {
                app.fast = fast;
                let ms = if fast { FAST_INTERVAL_MS } else { IDLE_INTERVAL_MS };
                SetTimer(hwnd, TICK_TIMER_ID, ms, None);
            }

            // Per-pixel hit test: only capture the mouse when it's over an
            // opaque pet pixel (or a drag is in progress).
            let c = cursor_pos();
            let over = app.runtime.is_dragging() || app.runtime.cursor_over_pet(c.x, c.y);
            app.window.set_click_through(!over);

            render_frame(app);
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            let app = &mut *app_ptr;
            let c = cursor_pos();
            if app.runtime.cursor_over_pet(c.x, c.y) {
                app.mouse_down = true;
                app.moved = false;
                app.down_pos = c;
                app.runtime.begin_drag(c.x, c.y);
                SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let app = &mut *app_ptr;
            if app.mouse_down {
                let c = cursor_pos();
                if (c.x - app.down_pos.x).abs() > 3 || (c.y - app.down_pos.y).abs() > 3 {
                    app.moved = true;
                }
                app.runtime.drag_to(c.x, c.y);
                render_frame(app);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let app = &mut *app_ptr;
            if app.mouse_down {
                app.mouse_down = false;
                let _ = ReleaseCapture();
                app.runtime.end_drag();
                if !app.moved {
                    app.runtime.on_pet_click();
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            show_menu(app_ptr, hwnd);
            LRESULT(0)
        }

        tray::TRAY_CALLBACK_MSG => {
            let ev = (lparam.0 as u32) & 0xffff;
            if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU || ev == WM_LBUTTONUP {
                show_menu(app_ptr, hwnd);
            }
            LRESULT(0)
        }

        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as u32;
            let app = &mut *app_ptr;
            match id {
                tray::ID_FEED => app.runtime.feed(),
                tray::ID_PLAY => app.runtime.play(),
                tray::ID_CLEAN => app.runtime.clean(),
                tray::ID_SEND => {
                    // Defer: opening the composer pumps its own message loop, so
                    // we must not still be holding `&mut App` here.
                    let _ = PostMessageW(hwnd, WM_APP_COMPOSE, WPARAM(0), LPARAM(0));
                }
                tray::ID_AUTOSTART => autostart::set_enabled(!autostart::is_enabled()),
                tray::ID_QUIT => {
                    KillTimer(hwnd, TICK_TIMER_ID).ok();
                    tray::remove_tray_icon(&app.tray);
                    PostQuitMessage(0);
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_APP_COMPOSE => {
            // Borrow, copy out peers, drop borrow, run the modal composer, then
            // re-borrow to hand the result back.
            let peers = {
                let app = &mut *app_ptr;
                app.runtime.peer_names_owned()
            };
            if let Some((text, peer)) = compose::present(hwnd, &peers) {
                let app = &mut *app_ptr;
                app.runtime.send_message(&text, &peer);
            }
            LRESULT(0)
        }

        WM_DISPLAYCHANGE => {
            let app = &mut *app_ptr;
            app.runtime.display_screen_changed();
            LRESULT(0)
        }

        WM_DESTROY => {
            KillTimer(hwnd, TICK_TIMER_ID).ok();
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_menu(app_ptr: *mut App, hwnd: HWND) {
    let app = &mut *app_ptr;
    let peers = app.runtime.peer_names().to_vec();
    let state = app.runtime.state.clone();
    let on = autostart::is_enabled();
    tray::show_context_menu(hwnd, &state, &peers, on);
}

fn render_frame(app: &mut App) {
    {
        let mut canvas = app.window.canvas();
        canvas.clear();

        if let Some(s) = app.runtime.pet_sprite() {
            if let Some(clip) = CLIPS.get(&s.anim) {
                let frame = &clip.frames[s.frame.min(clip.frames.len() - 1)];
                canvas.blit_grid(frame, ZOOM, s.x, s.y, !s.facing_right);
            }
        }
        if let Some(s) = app.runtime.visitor_sprite() {
            if let Some(clip) = CLIPS.get(&s.anim) {
                let frame = &clip.frames[s.frame.min(clip.frames.len() - 1)];
                canvas.blit_grid(frame, ZOOM, s.x, s.y, !s.facing_right);
            }
        }
    }

    if let Some((text, cx, by)) = app.runtime.bubble() {
        let text = text.to_string();
        app.window.draw_bubble(&text, cx, by);
    }

    app.window.present();
}

/// Fallback transport when even a local UDP socket can't be opened - discovery
/// and messaging are simply inert.
#[derive(Default)]
struct NullTransport;

impl net::PeerTransport for NullTransport {
    fn start(&mut self) {}
    fn local_name(&self) -> String {
        net::mdns_udp::local_display_name()
    }
    fn peer_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn send(&self, _message: &net::PetMessage, _to_peer: &str) {}
    fn try_recv(&self) -> Option<(net::PetMessage, String)> {
        None
    }
}

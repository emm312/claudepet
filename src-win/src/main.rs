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
mod letter;
mod net;
mod pet;
mod render;
mod runtime;
mod tray;
mod update;

use layered_window::LayeredWindow;
use net::mdns_udp::MdnsUdpTransport;
use pet::sprites;
use pet::sprites::CLIPS;
use runtime::{Runtime, ZOOM};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Orange, premultiplied BGRA (opaque). ~ RGB(255, 138, 40).
const BAR_COLOR: [u8; 4] = [40, 138, 255, 255];

const TICK_TIMER_ID: usize = 1;
const UPDATE_APPLY_TIMER_ID: usize = 2;
const WM_APP_COMPOSE: u32 = WM_APP + 2;
const WM_APP_UPDATE_READY: u32 = WM_APP + 3;
const WM_APP_READ_LETTER: u32 = WM_APP + 4;
const FAST_INTERVAL_MS: u32 = 33; // ~30 fps
const IDLE_INTERVAL_MS: u32 = 125; // 8 fps

struct App {
    window: LayeredWindow,
    runtime: Runtime,
    tray: windows::Win32::UI::Shell::NOTIFYICONDATAW,
    fast: bool,
    mouse_down: bool,
    bar_down: bool,
    /// A press landed on the pet's carried mail - a click (no drag) opens the
    /// unread letter.
    mail_down: bool,
    /// A letter window is up, pumping its own modal loop. Suppresses pet-hit
    /// click capture so the overlay doesn't un-transparent itself underneath it.
    modal_open: bool,
    moved: bool,
    down_pos: POINT,
    pending_update: Option<(std::path::PathBuf, String)>,
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
            bar_down: false,
            mail_down: false,
            modal_open: false,
            moved: false,
            down_pos: POINT::default(),
            pending_update: None,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *app as *mut App as isize);

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        render_frame(&mut app);
        SetTimer(hwnd, TICK_TIMER_ID, IDLE_INTERVAL_MS, None);
        spawn_update_checker(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Ensure the transport sent its mDNS GOODBYE even if none of the window
        // messages above ran (e.g. the process was killed from Task Manager).
        app.runtime.shutdown();
        KillTimer(hwnd, TICK_TIMER_ID).ok();
        tray::remove_tray_icon(&app.tray);
    }
}

/// Background: check GitHub Releases ~15s after start, then every 6h. Whenever a
/// newer release appears, download + verify it and hand the staged path to the
/// UI thread via `WM_APP_UPDATE_READY`. Keeps checking even after staging one -
/// with automatic updates *off* the staged build just waits in the menu, and a
/// later release should still supersede it. Each distinct version is staged and
/// posted at most once.
fn spawn_update_checker(hwnd: HWND) {
    let hwnd_bits = hwnd.0 as isize;
    std::thread::spawn(move || {
        update::cleanup();
        std::thread::sleep(std::time::Duration::from_secs(15));
        let mut staged_version: Option<String> = None;
        loop {
            if update::is_installed() {
                if let Some(info) = update::check() {
                    if staged_version.as_deref() != Some(info.version.as_str()) {
                        if let Ok(staged) = update::download_and_stage(&info) {
                            staged_version = Some(info.version.clone());
                            let payload: *mut (std::path::PathBuf, String) =
                                Box::into_raw(Box::new((staged, info.version)));
                            unsafe {
                                let _ = PostMessageW(
                                    HWND(hwnd_bits as *mut _),
                                    WM_APP_UPDATE_READY,
                                    WPARAM(0),
                                    LPARAM(payload as isize),
                                );
                            }
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
        }
    });
}

/// Save state, drop the tray icon, then swap in the staged exe and relaunch
/// (this call exits the process).
unsafe fn apply_update(app_ptr: *mut App, hwnd: HWND) {
    let app = &mut *app_ptr;
    let Some((staged, _)) = app.pending_update.take() else {
        return;
    };
    app.runtime.save_now();
    app.runtime.shutdown();
    KillTimer(hwnd, TICK_TIMER_ID).ok();
    KillTimer(hwnd, UPDATE_APPLY_TIMER_ID).ok();
    tray::remove_tray_icon(&app.tray);
    let _ = update::apply_and_relaunch(&staged);
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
        WM_TIMER if wparam.0 == UPDATE_APPLY_TIMER_ID => {
            KillTimer(hwnd, UPDATE_APPLY_TIMER_ID).ok();
            apply_update(app_ptr, hwnd);
            LRESULT(0)
        }
        WM_APP_UPDATE_READY => {
            let payload = lparam.0 as *mut (std::path::PathBuf, String);
            if payload.is_null() {
                return LRESULT(0);
            }
            let (staged, version) = *Box::from_raw(payload);
            let app = &mut *app_ptr;
            app.pending_update = Some((staged, version.clone()));
            if app.runtime.auto_update() {
                // Brief on-screen notice, then swap + relaunch.
                app.runtime.announce_update(&version);
                render_frame(app);
                SetTimer(hwnd, UPDATE_APPLY_TIMER_ID, 8_000, None);
            }
            // Otherwise it just shows as "Install update vX now" in the menu.
            LRESULT(0)
        }
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
            // opaque pet pixel, the carried mail, the orange bar, or a drag is in
            // progress. While a letter window is modal, stay fully click-through
            // so the overlay doesn't keep grabbing the cursor under it.
            let c = cursor_pos();
            let over = !app.modal_open
                && (app.runtime.is_dragging()
                    || app.bar_down
                    || app.mail_down
                    || app.runtime.cursor_over_bar(c.x, c.y)
                    || app.runtime.cursor_over_mail(c.x, c.y)
                    || app.runtime.cursor_over_pet(c.x, c.y));
            app.window.set_click_through(!over);

            render_frame(app);
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            let c = cursor_pos();
            let shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            let (over_bar, over_pet, over_mail) = {
                let app = &*app_ptr;
                (
                    app.runtime.cursor_over_bar(c.x, c.y),
                    app.runtime.cursor_over_pet(c.x, c.y),
                    app.runtime.cursor_over_mail(c.x, c.y),
                )
            };
            // Shift + click opens the same menu as a two-finger / right click.
            if shift && (over_bar || over_pet || over_mail) {
                show_menu(app_ptr, hwnd);
                return LRESULT(0);
            }
            let app = &mut *app_ptr;
            if over_mail {
                // The envelope sits on top of the pet - a plain click opens the
                // letter, so it wins over petting / dragging.
                app.mail_down = true;
                app.moved = false;
                app.down_pos = c;
                SetCapture(hwnd);
            } else if over_bar {
                app.bar_down = true;
                app.moved = false;
                app.down_pos = c;
                app.runtime.begin_bar_drag(c.x, c.y);
                SetCapture(hwnd);
            } else if over_pet {
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
            if app.mouse_down || app.bar_down || app.mail_down {
                let c = cursor_pos();
                if (c.x - app.down_pos.x).abs() > 3 || (c.y - app.down_pos.y).abs() > 3 {
                    app.moved = true;
                }
                if app.bar_down {
                    app.runtime.bar_drag_to(c.x, c.y);
                    render_frame(app);
                } else if app.mouse_down {
                    app.runtime.drag_to(c.x, c.y);
                    render_frame(app);
                }
                // mail_down: only tracking `moved` so a drag cancels the click;
                // the envelope doesn't move.
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let app = &mut *app_ptr;
            if app.mail_down {
                app.mail_down = false;
                let _ = ReleaseCapture();
                if !app.moved {
                    // Defer: the letter window pumps its own modal loop, so we
                    // must not still hold `&mut App` when it opens.
                    let _ = PostMessageW(hwnd, WM_APP_READ_LETTER, WPARAM(0), LPARAM(0));
                }
            } else if app.bar_down {
                app.bar_down = false;
                let _ = ReleaseCapture();
                app.runtime.end_bar_drag();
                if !app.moved {
                    app.runtime.toggle_dock();
                }
            } else if app.mouse_down {
                app.mouse_down = false;
                let _ = ReleaseCapture();
                app.runtime.end_drag();
                if !app.moved {
                    app.runtime.on_pet_click();
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP | WM_CONTEXTMENU => {
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
                tray::ID_SEARCH => app.runtime.search_for_pets(),
                tray::ID_SEND => {
                    // Defer: opening the composer pumps its own message loop, so
                    // we must not still be holding `&mut App` here.
                    let _ = PostMessageW(hwnd, WM_APP_COMPOSE, WPARAM(0), LPARAM(0));
                }
                tray::ID_READ_LETTER => {
                    let _ = PostMessageW(hwnd, WM_APP_READ_LETTER, WPARAM(0), LPARAM(0));
                }
                tray::ID_AUTOSTART => autostart::set_enabled(!autostart::is_enabled()),
                tray::ID_AUTOUPDATE => {
                    let on = !app.runtime.auto_update();
                    app.runtime.set_auto_update(on);
                }
                tray::ID_UPDATE_NOW => apply_update(app_ptr, hwnd),
                tray::ID_QUIT => {
                    KillTimer(hwnd, TICK_TIMER_ID).ok();
                    tray::remove_tray_icon(&app.tray);
                    app.runtime.shutdown();
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
            if let Some((text, peer, express)) = compose::present(hwnd, &peers) {
                let app = &mut *app_ptr;
                app.runtime.send_message(&text, &peer, express);
            }
            LRESULT(0)
        }

        WM_APP_READ_LETTER => {
            // Peek the oldest unread letter, drop the borrow, run the modal
            // reader, then re-borrow to retire it and maybe send the reply.
            let msg = {
                let app = &mut *app_ptr;
                if app.modal_open {
                    return LRESULT(0);
                }
                match app.runtime.peek_unread() {
                    Some(m) => {
                        app.modal_open = true;
                        m
                    }
                    None => return LRESULT(0),
                }
            };
            let reply = letter::present_read(hwnd, &msg);
            let app = &mut *app_ptr;
            app.modal_open = false;
            app.runtime.pop_unread();
            if let Some(text) = reply {
                app.runtime.send_message(&text, &msg.sender_name, false);
            }
            LRESULT(0)
        }

        WM_DISPLAYCHANGE => {
            let app = &mut *app_ptr;
            app.runtime.display_screen_changed();
            LRESULT(0)
        }

        WM_DESTROY => {
            let app = &mut *app_ptr;
            KillTimer(hwnd, TICK_TIMER_ID).ok();
            app.runtime.shutdown();
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
    let autostart_on = autostart::is_enabled();
    let auto_update_on = app.runtime.auto_update();
    let has_unread = app.runtime.has_unread();
    let pending = app.pending_update.as_ref().map(|(_, v)| v.clone());
    tray::show_context_menu(
        hwnd,
        &state,
        &peers,
        autostart_on,
        auto_update_on,
        has_unread,
        pending.as_deref(),
    );
}

/// Draw one courier actor: horse (under, if express) → pet sprite → mail (over,
/// if carrying). Mirrors for a left-facing actor. The horse/mail are pixel
/// grids in the pet's own style (`pet::sprites::HORSE_FRAMES`/`MAIL_GRID`),
/// drawn at the same `ZOOM` as the pet.
fn draw_actor(canvas: &mut render::Canvas, s: &runtime::FrameSprite) {
    let sprite_px = runtime::SPRITE_PX;
    let flip = !s.facing_right;

    // Riding lifts the pet so it sits astride the horse's back.
    let pet_y = if s.on_horse { s.y - runtime::HORSE_RIDER_LIFT } else { s.y };

    if s.on_horse {
        let frame = &sprites::HORSE_FRAMES[s.horse_frame % sprites::HORSE_FRAMES.len()];
        let hw = sprites::HORSE_GRID_COLS as i32 * ZOOM;
        let hx = s.x + sprite_px / 2 - hw / 2;
        let hy = pet_y + sprite_px / 2 - 4;
        // No edge clamp here: `Canvas::put` clips every pixel, and clamping only
        // the horse would let the rider slide off it near the screen bottom.
        canvas.blit_grid(frame, ZOOM, hx, hy, flip);
    }

    if let Some(clip) = CLIPS.get(&s.anim) {
        let frame = &clip.frames[s.frame.min(clip.frames.len() - 1)];
        canvas.blit_grid(frame, ZOOM, s.x, pet_y, flip);
    }

    if s.carry_mail {
        // Placement lives in `runtime::mail_rect` - single source of truth so the
        // click hit test (`runtime::cursor_over_mail`) can't drift from the draw.
        let (mx, my, _, _) = runtime::mail_rect(s);
        canvas.blit_grid(&sprites::MAIL_GRID, runtime::MAIL_ZOOM, mx, my, flip);
    }
}

fn render_frame(app: &mut App) {
    {
        let mut canvas = app.window.canvas();
        canvas.clear();

        // The orange dock bar is always visible - it's the handle for docking /
        // undocking the pet.
        let (bx, by, bw, bh) = app.runtime.bar_rect();
        canvas.fill_rect(bx, by, bw, bh, BAR_COLOR);

        if let Some(s) = app.runtime.pet_sprite() {
            draw_actor(&mut canvas, &s);
        }
        if let Some(s) = app.runtime.visitor_sprite() {
            draw_actor(&mut canvas, &s);
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
    fn stop(&mut self) {}
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

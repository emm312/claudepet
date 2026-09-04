//! "Claude's Adventure" - a short, self-closing cutscene played right after a
//! letter is sent, when the compose window's "Watch the journey" box was
//! ticked. A baked pixel-art backdrop (`Resources/adventure/castle_bridge.bgra`
//! - a raw 192x192 BGRA blob, *not* a decoded JPEG at runtime; see CLAUDE.md)
//! with the pet walking the stone bridge up to the castle gate, then a brief
//! hold before the window closes itself. If the letter went express the pet
//! gallops in on the horse (`sprites::HORSE_FRAMES`, the same 2-frame cycle the
//! courier uses).
//!
//! Built like `letter.rs` / `customize.rs`: a plain Win32 popup with its own
//! nested message pump. Unlike those it isn't waiting on user input - a
//! `WM_TIMER` drives the animation and trips `done` once the pet arrives.

use std::collections::HashSet;
use std::time::Instant;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, COLORONCOLOR, DIB_RGB_COLORS, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::pet::brain::AnimState;
use crate::pet::sprites::{
    AccessoryId, SkinId, ACCESSORIES, HORSE_FRAMES, HORSE_FRAME_DURATION, PALETTE, SKINS,
};
use crate::render::Canvas;

/// Baked backdrop: raw top-down BGRA, 192x192, alpha forced opaque. Produced
/// once from `Resources/adventure/castle_bridge.jpg` by `Resources/adventure/
/// make_bg.py` (or the System.Drawing snippet in CLAUDE.md) and checked in -
/// nothing here, and no build step, decodes a JPEG.
const BG_W: i32 = 192;
const BG_H: i32 = 192;
static BG_BGRA: &[u8] = include_bytes!("../../Resources/adventure/castle_bridge.bgra");

/// The window is an exact 2x nearest-neighbour blow-up of the backdrop.
const SCALE: i32 = 2;

/// Scene sprite zoom. The pet grid is 16px; on a 192px backdrop, zoom 2 reads
/// as a small figure out on the road (the pet's own overlay uses zoom 5).
const ZOOM: i32 = 2;
const SPRITE_PX: i32 = 16 * ZOOM;

/// Lifts the rider so it sits astride the horse's back rather than inside it -
/// the scene-local analog of `runtime::HORSE_RIDER_LIFT`, scaled for `ZOOM`.
const RIDER_LIFT: i32 = 9;

/// Fake perspective: the pet (and horse) shrink from full size at the near end
/// of the path to `FAR_SCALE` as they reach the castle, so the walk reads as
/// heading into the distance. Linear in path fraction - close enough at this
/// size, and the polyline segments aren't uniform-length anyway.
const NEAR_SCALE: f32 = 1.0;
const FAR_SCALE: f32 = 0.45;

fn scale_at(p: f32) -> f32 {
    NEAR_SCALE + (FAR_SCALE - NEAR_SCALE) * p.clamp(0.0, 1.0)
}

/// How long the pet takes to walk the whole bridge, and how long the finished
/// scene holds on the castle before the window closes itself.
const WALK_SECONDS: f64 = 7.0;
const HOLD_SECONDS: f64 = 1.6;
/// Express arrives noticeably quicker (the courier's own express multiplier is
/// 3x; eased to 2x here so the gallop still reads at this size).
const EXPRESS_WALK_SECONDS: f64 = WALK_SECONDS / 2.0;

/// The stone bridge as normalised (x, y) knots over the backdrop, y pointing
/// down - eyeballed against `castle_bridge.jpg`. The pet follows this polyline
/// from the first knot (just off the bottom edge) to the last (the castle
/// gate), facing whichever way the current segment runs.
const PATH: [(f32, f32); 9] = [
    (0.26, 0.99), // onto the cobbles at the bottom edge
    (0.32, 0.90),
    (0.42, 0.85),
    (0.52, 0.80), // the cobbled path merges into the raised rampart
    (0.62, 0.77),
    (0.71, 0.71), // out to the rampart's rightmost bulge
    (0.70, 0.63), // turning back up toward the keep, now facing left
    (0.58, 0.585),
    (0.47, 0.56), // in where the rampart meets the castle
];

struct AdventureState {
    start: Instant,
    skin: SkinId,
    accessories: Vec<AccessoryId>,
    express: bool,
    /// Scratch frame: the backdrop copied in, sprites drawn over, then stretched
    /// to the window. Held across frames so it isn't reallocated every paint.
    scratch: Vec<u8>,
    /// Set the first frame the pet reaches the gate; the window closes
    /// `HOLD_SECONDS` after that.
    arrived_at: Option<f64>,
    done: bool,
}

const ANIM_TIMER: usize = 1;

/// Play the cutscene modally over `owner`. Returns when the pet has reached the
/// castle and the hold has elapsed, or the user closed the window.
pub fn present(owner: HWND, skin: SkinId, accessories: &HashSet<AccessoryId>, express: bool) {
    unsafe {
        let Ok(hinst) = GetModuleHandleW(None) else {
            return;
        };
        let class_name = w!("ClaudePetAdventureClass");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(adventure_proc),
            hInstance: hinst.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc); // ignore "already registered"

        let mut st = Box::new(AdventureState {
            start: Instant::now(),
            skin,
            accessories: accessories.iter().copied().collect(),
            express,
            scratch: BG_BGRA.to_vec(),
            arrived_at: None,
            done: false,
        });

        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let exstyle = WS_EX_TOPMOST | WS_EX_DLGMODALFRAME;
        // Grow the frame so the *client* area is exactly BG * SCALE.
        let mut rc = RECT { left: 0, top: 0, right: BG_W * SCALE, bottom: BG_H * SCALE };
        let _ = AdjustWindowRectEx(&mut rc, style, false, exstyle);
        let (ww, wh) = (rc.right - rc.left, rc.bottom - rc.top);
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = match CreateWindowExW(
            exstyle,
            class_name,
            w!("Claude's Adventure"),
            style,
            (sw - ww) / 2,
            (sh - wh) / 2,
            ww,
            wh,
            owner,
            None,
            hinst,
            Some(&mut *st as *mut _ as *const _),
        ) {
            Ok(h) => h,
            Err(_) => return,
        };

        let _ = EnableWindow(owner, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        SetTimer(hwnd, ANIM_TIMER, 33, None); // ~30 fps

        let mut msg = MSG::default();
        while !st.done && GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let _ = KillTimer(hwnd, ANIM_TIMER);
        let _ = EnableWindow(owner, true);
        let _ = SetForegroundWindow(owner);
        let _ = DestroyWindow(hwnd);
    }
}

/// Advance the clock-driven state one timer tick. Returns `true` once the pet
/// has arrived and the hold has elapsed, i.e. the window should close.
///
/// Arrival is decided here, from the wall clock - **not** in `render` / `WM_PAINT`,
/// which Windows is free to coalesce or skip entirely while the window is
/// occluded or minimised. If closure depended on paint, an occluded cutscene
/// would hang with the owner window still disabled.
fn advance(st: &mut AdventureState) -> bool {
    let now = st.start.elapsed().as_secs_f64();
    let walk_secs = if st.express { EXPRESS_WALK_SECONDS } else { WALK_SECONDS };
    if now / walk_secs >= 1.0 && st.arrived_at.is_none() {
        st.arrived_at = Some(now);
    }
    matches!(st.arrived_at, Some(arrived) if now - arrived >= HOLD_SECONDS)
}

/// Point + facing along `PATH` at fraction `p` (0..=1) of its total arc length.
fn path_at(p: f32) -> (f32, f32, bool) {
    let mut seglen = [0f32; PATH.len() - 1];
    let mut total = 0f32;
    for i in 0..PATH.len() - 1 {
        let dx = PATH[i + 1].0 - PATH[i].0;
        let dy = PATH[i + 1].1 - PATH[i].1;
        let l = (dx * dx + dy * dy).sqrt();
        seglen[i] = l;
        total += l;
    }
    let target = p.clamp(0.0, 1.0) * total;
    let mut acc = 0f32;
    for i in 0..PATH.len() - 1 {
        if acc + seglen[i] >= target || i == PATH.len() - 2 {
            let t = if seglen[i] > 0.0 { (target - acc) / seglen[i] } else { 0.0 };
            let (x0, y0) = PATH[i];
            let (x1, y1) = PATH[i + 1];
            return (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, x1 >= x0);
        }
        acc += seglen[i];
    }
    let last = PATH[PATH.len() - 1];
    (last.0, last.1, true)
}

/// Rebuild `st.scratch` for the current wall-clock time: backdrop, then horse
/// (if express), then the pet frame + any worn accessories.
fn render(st: &mut AdventureState) {
    // `arrived_at` / `done` are driven from `WM_TIMER` (`advance`), never here -
    // `WM_PAINT` can be skipped while the window is occluded. This only reads it.
    let t = st.start.elapsed().as_secs_f64();
    let walk_secs = if st.express { EXPRESS_WALK_SECONDS } else { WALK_SECONDS };
    let p = (t / walk_secs).min(1.0) as f32;

    st.scratch.copy_from_slice(BG_BGRA);
    let mut canvas = Canvas { px: &mut st.scratch, w: BG_W, h: BG_H };

    let (nx, ny, facing_right) = path_at(p);
    let foot_x = (nx * BG_W as f32) as i32;
    let foot_y = (ny * BG_H as f32) as i32;
    let flip = !facing_right;
    let scale = scale_at(p);

    // Pet grid is 16x16; scale it around the foot point on the path.
    let pet_px = (SPRITE_PX as f32 * scale).round() as i32;
    let pet_x = foot_x - pet_px / 2;
    let mut pet_y = foot_y - pet_px;

    if st.express {
        let frame = &HORSE_FRAMES[((t / HORSE_FRAME_DURATION) as usize) % HORSE_FRAMES.len()];
        let cols = frame.first().map(|r| r.len()).unwrap_or(0) as i32;
        let rows = frame.len() as i32;
        let hw = (cols as f32 * ZOOM as f32 * scale).round() as i32;
        let hh = (rows as f32 * ZOOM as f32 * scale).round() as i32;
        let hx = foot_x - hw / 2;
        let hy = foot_y - hh;
        blit_grid_scaled(&mut canvas, frame, &PALETTE, hx, hy, hw, hh, flip);
        pet_y -= (RIDER_LIFT as f32 * scale).round() as i32;
    }

    if let Some(skin) = SKINS.get(&st.skin) {
        let anim = if st.arrived_at.is_some() { AnimState::Idle } else { AnimState::Walk };
        let clip = skin.clips.get(&anim).or_else(|| skin.clips.get(&AnimState::Walk));
        if let Some(clip) = clip {
            let fi = ((t / clip.frame_duration) as usize) % clip.frames.len();
            blit_grid_scaled(
                &mut canvas, &clip.frames[fi], &skin.palette, pet_x, pet_y, pet_px, pet_px, flip,
            );
            for aid in &st.accessories {
                if let Some(acc) = ACCESSORIES.get(aid) {
                    blit_grid_scaled(
                        &mut canvas, &acc.grid, &acc.palette, pet_x, pet_y, pet_px, pet_px, flip,
                    );
                }
            }
        }
    }
}

/// Nearest-neighbour blit of a palette-index grid into an arbitrary `dest_w` x
/// `dest_h` rect (clipped to the canvas). `Canvas::blit_grid` only does integer
/// zoom; the cutscene needs a fractional scale for the into-the-distance shrink.
#[allow(clippy::too_many_arguments)]
fn blit_grid_scaled(
    canvas: &mut Canvas,
    grid: &[Vec<u8>],
    palette: &[[u8; 4]],
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    flip_h: bool,
) {
    let rows = grid.len() as i32;
    let cols = grid.first().map(|r| r.len()).unwrap_or(0) as i32;
    if rows == 0 || cols == 0 || dest_w <= 0 || dest_h <= 0 {
        return;
    }
    for dy in 0..dest_h {
        let sy = (dy * rows / dest_h).min(rows - 1);
        let py = dest_y + dy;
        if py < 0 || py >= canvas.h {
            continue;
        }
        for dx in 0..dest_w {
            let mut sx = dx * cols / dest_w;
            if flip_h {
                sx = cols - 1 - sx;
            }
            let sx = sx.clamp(0, cols - 1);
            let idx = grid[sy as usize][sx as usize];
            if idx == 0 {
                continue;
            }
            let px = dest_x + dx;
            if px < 0 || px >= canvas.w {
                continue;
            }
            let rgba = palette[idx as usize % palette.len()];
            let bgra = [rgba[2], rgba[1], rgba[0], rgba[3]];
            let i = ((py * canvas.w + px) * 4) as usize;
            canvas.px[i..i + 4].copy_from_slice(&bgra);
        }
    }
}

unsafe extern "system" fn adventure_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            let st_ptr = (*cs).lpCreateParams as *mut AdventureState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, st_ptr as isize);
            LRESULT(0)
        }

        // Painted edge-to-edge every frame, so nothing needs erasing first.
        WM_ERASEBKGND => LRESULT(1),

        WM_TIMER => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AdventureState;
            if !st_ptr.is_null() && advance(&mut *st_ptr) {
                (*st_ptr).done = true;
            }
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }

        WM_PAINT => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AdventureState;
            if st_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let st = &mut *st_ptr;
            render(st);

            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: BG_W,
                    biHeight: -BG_H, // negative => top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0, // BI_RGB
                    ..Default::default()
                },
                ..Default::default()
            };
            SetStretchBltMode(hdc, COLORONCOLOR);
            StretchDIBits(
                hdc,
                0,
                0,
                rc.right,
                rc.bottom,
                0,
                0,
                BG_W,
                BG_H,
                Some(st.scratch.as_ptr() as *const _),
                &bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_CLOSE => {
            let st_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AdventureState;
            if !st_ptr.is_null() {
                (*st_ptr).done = true;
            }
            // Nudge the nested pump so it re-tests `done` even if the timer is
            // somehow gone (matches tray.rs's post-menu WM_NULL).
            let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }

        WM_DESTROY => {
            let _ = KillTimer(hwnd, ANIM_TIMER);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baked_backdrop_is_the_expected_raw_size() {
        // A wrong-sized blob would have StretchDIBits read past the scratch
        // buffer at runtime. 32bpp, no row padding at this width.
        assert_eq!(BG_BGRA.len(), (BG_W * BG_H * 4) as usize);
    }

    #[test]
    fn path_runs_from_the_bottom_edge_to_the_castle_gate() {
        let (_, y0, _) = path_at(0.0);
        let (_, y1, _) = path_at(1.0);
        assert!(y0 > 0.95, "starts off the bottom edge");
        assert!(y1 < 0.7, "ends up at the castle");
        assert!(y1 < y0, "net travel is upward (screen y grows downward)");
    }

    #[test]
    fn path_is_clamped_outside_zero_to_one() {
        assert_eq!(path_at(-1.0), path_at(0.0));
        assert_eq!(path_at(2.0), path_at(1.0));
    }

    #[test]
    fn perspective_shrinks_toward_the_castle_and_clamps() {
        assert!((scale_at(0.0) - NEAR_SCALE).abs() < f32::EPSILON);
        assert!((scale_at(1.0) - FAR_SCALE).abs() < f32::EPSILON);
        assert!(scale_at(0.5) < scale_at(0.0), "further along is smaller");
        assert_eq!(scale_at(-1.0), scale_at(0.0), "clamped below 0");
        assert_eq!(scale_at(2.0), scale_at(1.0), "clamped above 1");
    }
}

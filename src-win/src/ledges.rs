//! Walkable "ledges" - the top edge of every visible top-level window, plus the
//! screen floor - that the pet can stand and walk on and land on when it falls.
//! Mirrors `Sources/ClaudePet/Overlay/WindowLedges.swift`, using `EnumWindows` /
//! DWM frame bounds in place of `CGWindowListCopyWindowInfo`.
//!
//! Coordinates are Win32 screen pixels (origin top-left, +Y down). A ledge's `y`
//! is a window's top edge; "below the pet" means a larger `y`.

use crate::geometry;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindowVisible, GWL_EXSTYLE,
    GWL_STYLE, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ledge {
    pub min_x: f64,
    pub max_x: f64,
    /// Screen y of the walkable surface (a window's top edge, or the floor).
    pub y: f64,
}

/// Removes `cut` from every interval, splitting an interval when the cut falls
/// in its middle.
fn subtract(intervals: &[(f64, f64)], cut: (f64, f64)) -> Vec<(f64, f64)> {
    let mut result = Vec::new();
    for &(lo, hi) in intervals {
        if cut.1 <= lo || cut.0 >= hi {
            result.push((lo, hi));
            continue;
        }
        if cut.0 > lo {
            result.push((lo, cut.0));
        }
        if cut.1 < hi {
            result.push((cut.1, hi));
        }
    }
    result
}

/// The nearest ledge at or below `y` that spans `x` - i.e. what the pet lands on
/// if it falls straight down from `(x, y)`. "Nearest below" = smallest `y` that
/// is still `>= y` (Win32 +Y-down).
pub fn ledge_below(x: f64, y: f64, ledges: &[Ledge]) -> Option<Ledge> {
    ledges
        .iter()
        .filter(|l| l.min_x <= x && x <= l.max_x && l.y >= y)
        .min_by(|a, b| a.y.partial_cmp(&b.y).unwrap())
        .copied()
}

struct EnumState {
    hwnds: Vec<HWND>,
}

extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = unsafe { &mut *(lparam.0 as *mut EnumState) };
    state.hwnds.push(hwnd);
    BOOL(1)
}

fn frame_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut r = RECT::default();
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );
        if hr.is_ok() && r.right > r.left && r.bottom > r.top {
            return Some(r);
        }
        let mut r2 = RECT::default();
        if GetWindowRect(hwnd, &mut r2).is_ok() {
            return Some(r2);
        }
        None
    }
}

fn is_cloaked(hwnd: HWND) -> bool {
    unsafe {
        let mut cloaked: u32 = 0;
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            4,
        );
        hr.is_ok() && cloaked != 0
    }
}

fn is_candidate_window(hwnd: HWND, own_hwnd: isize) -> bool {
    if hwnd.0 as isize == own_hwnd {
        return false;
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        // A real, user-facing window: has a title bar, or explicitly asks to be
        // treated as an app window.
        (style & WS_CAPTION.0 != 0) || (ex & WS_EX_APPWINDOW.0 != 0)
    }
}

struct WinRect {
    min_x: f64,
    max_x: f64,
    top_y: f64,
    bottom_y: f64,
}

/// Current walkable ledges: every candidate window's (partly occluded) top edge,
/// plus the work-area floor as an always-present fallback.
pub fn current_ledges(own_hwnd: isize, min_width: f64) -> Vec<Ledge> {
    let mut state = EnumState { hwnds: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut _ as isize));
    }

    // EnumWindows yields windows front-to-back (top of the Z-order first).
    let mut windows: Vec<WinRect> = Vec::new();
    for hwnd in state.hwnds {
        if !is_candidate_window(hwnd, own_hwnd) || is_cloaked(hwnd) {
            continue;
        }
        let Some(r) = frame_rect(hwnd) else { continue };
        let w = (r.right - r.left) as f64;
        let h = (r.bottom - r.top) as f64;
        if w < min_width || h < 10.0 {
            continue;
        }
        windows.push(WinRect {
            min_x: r.left as f64,
            max_x: r.right as f64,
            top_y: r.top as f64,
            bottom_y: r.bottom as f64,
        });
    }

    // The overlay only covers the primary monitor, so clip ledges to it - a
    // window on a second display must not become something the pet falls toward
    // and can never reach.
    let (screen_w, screen_h) = geometry::primary_screen_size();
    let (screen_w, screen_h) = (screen_w as f64, screen_h as f64);

    let mut ledges: Vec<Ledge> = Vec::new();
    for i in 0..windows.len() {
        let win = &windows[i];
        let mut intervals: Vec<(f64, f64)> = vec![(win.min_x, win.max_x)];
        // Only windows in front of this one (earlier in the list) can hide part
        // of its top edge.
        for front in windows.iter().take(i) {
            let covers_height = front.top_y <= win.top_y && win.top_y <= front.bottom_y;
            let overlaps_x = front.max_x > win.min_x && front.min_x < win.max_x;
            if covers_height && overlaps_x {
                intervals = subtract(&intervals, (front.min_x, front.max_x));
            }
        }
        if win.top_y < 0.0 || win.top_y > screen_h {
            continue;
        }
        for (lo, hi) in intervals {
            let lo = lo.max(0.0);
            let hi = hi.min(screen_w);
            if hi - lo >= min_width {
                ledges.push(Ledge { min_x: lo, max_x: hi, y: win.top_y });
            }
        }
    }

    ledges.extend(fallback_ledges());
    ledges
}

/// The work-area floor of the primary screen - always walkable, so the pet
/// always has somewhere to land.
pub fn fallback_ledges() -> Vec<Ledge> {
    let area = geometry::primary_work_area();
    vec![Ledge { min_x: area.left, max_x: area.right, y: area.bottom }]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ports Tests/ClaudePetTests/WindowLedgesTests.swift, flipped to +Y-down:
    // "below the drop point" is now a larger y, and ledge_below picks the
    // smallest qualifying y.

    #[test]
    fn ledge_below_picks_nearest_qualifying_ledge() {
        let ledges = [
            Ledge { min_x: 0.0, max_x: 500.0, y: 800.0 }, // screen floor
            Ledge { min_x: 100.0, max_x: 300.0, y: 600.0 }, // a window's top edge
            Ledge { min_x: 100.0, max_x: 300.0, y: 400.0 }, // a window higher up
        ];
        // Falling down from y=550 -> first surface at or below is y=600.
        let result = ledge_below(150.0, 550.0, &ledges);
        assert_eq!(result.map(|l| l.y), Some(600.0));
    }

    #[test]
    fn ledge_below_ignores_ledges_not_spanning_x() {
        let ledges = [
            Ledge { min_x: 0.0, max_x: 500.0, y: 800.0 },
            Ledge { min_x: 600.0, max_x: 800.0, y: 500.0 }, // out of x range
        ];
        let result = ledge_below(150.0, 300.0, &ledges);
        assert_eq!(result.map(|l| l.y), Some(800.0));
    }

    #[test]
    fn ledge_below_returns_none_when_nothing_qualifies() {
        let ledges = [Ledge { min_x: 0.0, max_x: 500.0, y: 200.0 }]; // only above the drop point
        let result = ledge_below(150.0, 300.0, &ledges);
        assert_eq!(result, None);
    }

    #[test]
    fn subtract_splits_interval_in_the_middle() {
        assert_eq!(subtract(&[(0.0, 100.0)], (40.0, 60.0)), vec![(0.0, 40.0), (60.0, 100.0)]);
        assert_eq!(subtract(&[(0.0, 100.0)], (-10.0, 30.0)), vec![(30.0, 100.0)]);
        assert_eq!(subtract(&[(0.0, 100.0)], (200.0, 300.0)), vec![(0.0, 100.0)]);
    }
}

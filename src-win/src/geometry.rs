//! Screen geometry helpers in Win32 screen coordinates (origin top-left, +Y
//! down). Mirrors `Sources/ClaudePet/Overlay/ScreenGeometry.swift`, flipped to
//! Windows' coordinate convention.

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Rect {
    pub fn width(&self) -> f64 {
        self.right - self.left
    }
    #[allow(dead_code)]
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

/// Full pixel size of the primary monitor.
pub fn primary_screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

/// Work area of the primary monitor (full screen minus the taskbar). The pet
/// treats `bottom` as the floor it always has to land on.
pub fn primary_work_area() -> Rect {
    let mut r = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut r as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    if r.right <= r.left || r.bottom <= r.top {
        let (w, h) = primary_screen_size();
        return Rect { left: 0.0, top: 0.0, right: w as f64, bottom: h as f64 };
    }
    Rect { left: r.left as f64, top: r.top as f64, right: r.right as f64, bottom: r.bottom as f64 }
}

/// The work area of whichever monitor contains `point` (falls back to primary).
pub fn work_area_containing(x: f64, y: f64) -> Rect {
    unsafe {
        let hmon = MonitorFromPoint(POINT { x: x as i32, y: y as i32 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            let w = mi.rcWork;
            return Rect {
                left: w.left as f64,
                top: w.top as f64,
                right: w.right as f64,
                bottom: w.bottom as f64,
            };
        }
    }
    primary_work_area()
}

/// Clamp a `w`x`h` sprite origin so it stays fully inside the work area that
/// currently contains its centre. Used after display changes / drags.
pub fn clamp_origin(x: f64, y: f64, w: f64, h: f64) -> (f64, f64) {
    let area = work_area_containing(x + w / 2.0, y + h / 2.0);
    let max_x = (area.right - w).max(area.left);
    let max_y = (area.bottom - h).max(area.top);
    (x.clamp(area.left, max_x), y.clamp(area.top, max_y))
}

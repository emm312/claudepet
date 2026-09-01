//! The always-on-top, per-pixel-alpha overlay the pet is drawn on. A single
//! `WS_EX_LAYERED` popup covering the primary monitor; the pet moves within the
//! DIB, not by moving the window. `UpdateLayeredWindow` with a premultiplied
//! BGRA DIB section gives transparent pixels true click-through.
//!
//! Replaces `Overlay/OverlayWindow.swift` + `Overlay/OverlayView.swift`.

use crate::render::Canvas;
use std::ffi::c_void;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, CreatePen, CreateSolidBrush, DeleteDC,
    DeleteObject, DrawTextW, GetDC, ReleaseDC, RoundRect, SelectObject, SetBkMode, SetTextColor,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS,
    DT_CENTER, DT_NOPREFIX, DT_WORDBREAK, FW_SEMIBOLD, HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ, HPEN,
    PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, UpdateLayeredWindow, GWL_EXSTYLE, ULW_ALPHA,
    WS_EX_LAYERED, WS_EX_TRANSPARENT,
};

pub struct LayeredWindow {
    hwnd: HWND,
    pub w: i32,
    pub h: i32,
    mem_dc: HDC,
    bitmap: HBITMAP,
    bits: *mut c_void,
    click_through: bool,
}

impl LayeredWindow {
    /// Attach to an already-created `WS_EX_LAYERED` popup window sized `w`x`h`.
    pub fn attach(hwnd: HWND, w: i32, h: i32) -> Self {
        unsafe {
            let screen_dc = GetDC(None);
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // negative => top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0, // BI_RGB
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                screen_dc,
                &bmi as *const _ as *const BITMAPINFO,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )
            .expect("CreateDIBSection failed");
            let mem_dc = CreateCompatibleDC(screen_dc);
            SelectObject(mem_dc, HGDIOBJ(bitmap.0));
            ReleaseDC(None, screen_dc);
            let _ = &mut bmi;

            let mut lw = LayeredWindow {
                hwnd,
                w,
                h,
                mem_dc,
                bitmap,
                bits,
                click_through: false,
            };
            lw.set_click_through(true);
            lw
        }
    }

    fn pixels(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits as *mut u8, (self.w * self.h * 4) as usize) }
    }

    pub fn canvas(&mut self) -> Canvas<'_> {
        let (w, h) = (self.w, self.h);
        Canvas { px: self.pixels(), w, h }
    }

    /// Toggle whole-window click-through (`WS_EX_TRANSPARENT`). Mirrors
    /// `OverlayView.updateHitTest` flipping `ignoresMouseEvents`.
    pub fn set_click_through(&mut self, on: bool) {
        if on == self.click_through {
            return;
        }
        self.click_through = on;
        unsafe {
            let mut ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) as u32;
            ex |= WS_EX_LAYERED.0;
            if on {
                ex |= WS_EX_TRANSPARENT.0;
            } else {
                ex &= !WS_EX_TRANSPARENT.0;
            }
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex as isize);
        }
    }

    /// Push the current DIB contents to the screen. The window itself stays at
    /// (0,0) covering the primary monitor.
    pub fn present(&self) {
        unsafe {
            let screen_dc = GetDC(None);
            let src = POINT { x: 0, y: 0 };
            let dst = POINT { x: 0, y: 0 };
            let size = SIZE { cx: self.w, cy: self.h };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                screen_dc,
                Some(&dst),
                Some(&size),
                self.mem_dc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen_dc);
        }
    }

    /// Draw a rounded speech bubble with wrapped text, centred on `center_x`
    /// with its bottom `bubble_bottom` px down the screen. GDI can't write the
    /// alpha channel, so afterwards every pixel the draw actually touched (any
    /// non-zero colour) in the bubble's box is forced opaque - leaving the
    /// rounded corners transparent.
    pub fn draw_bubble(&mut self, text: &str, center_x: i32, bubble_bottom: i32) {
        let bw = 240i32;
        let bh = 56i32;
        let mut left = center_x - bw / 2;
        left = left.clamp(4, (self.w - bw - 4).max(4));
        let top = (bubble_bottom - bh).max(4);
        let right = left + bw;
        let bottom = top + bh;

        unsafe {
            let brush: HBRUSH = CreateSolidBrush(COLORREF(rgb(250, 247, 240)));
            let pen: HPEN = CreatePen(PS_SOLID, 1, COLORREF(rgb(184, 122, 92)));
            let old_brush = SelectObject(self.mem_dc, HGDIOBJ(brush.0));
            let old_pen = SelectObject(self.mem_dc, HGDIOBJ(pen.0));
            let _ = RoundRect(self.mem_dc, left, top, right, bottom, 18, 18);

            let font: HFONT = CreateFontW(
                -14, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0,
                0, 0, 0, 0, 0,
                windows::core::w!("Segoe UI"),
            );
            let old_font = SelectObject(self.mem_dc, HGDIOBJ(font.0));
            SetBkMode(self.mem_dc, TRANSPARENT);
            SetTextColor(self.mem_dc, COLORREF(rgb(40, 30, 20)));
            let mut rc = RECT {
                left: left + 12,
                top: top + 6,
                right: right - 12,
                bottom: bottom - 6,
            };
            let mut wtext: Vec<u16> = text.encode_utf16().collect();
            DrawTextW(
                self.mem_dc,
                &mut wtext,
                &mut rc,
                DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
            );

            SelectObject(self.mem_dc, old_font);
            SelectObject(self.mem_dc, old_pen);
            SelectObject(self.mem_dc, old_brush);
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = DeleteObject(HGDIOBJ(pen.0));
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }

        // Repair the alpha channel over the bubble's bounding box.
        let (w, h) = (self.w, self.h);
        let px = self.pixels();
        for y in top.max(0)..bottom.min(h) {
            for x in left.max(0)..right.min(w) {
                let i = ((y * w + x) * 4) as usize;
                if px[i] != 0 || px[i + 1] != 0 || px[i + 2] != 0 {
                    px[i + 3] = 255;
                }
            }
        }
    }
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.mem_dc);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
        }
    }
}

#[inline]
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

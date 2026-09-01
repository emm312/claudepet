//! Software rasteriser for the pixel-grid sprites. Writes premultiplied BGRA
//! (top-down) into a borrowed pixel buffer that backs the layered overlay
//! window's DIB section. Mirrors `PixelArtRenderer` in
//! `Sources/ClaudePet/Pet/Sprites.swift` (nearest-neighbour integer zoom).

use crate::pet::sprites::PALETTE;

/// A borrowed view of the overlay's 32-bit top-down BGRA framebuffer.
pub struct Canvas<'a> {
    pub px: &'a mut [u8],
    pub w: i32,
    pub h: i32,
}

impl Canvas<'_> {
    pub fn clear(&mut self) {
        self.px.iter_mut().for_each(|b| *b = 0);
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, bgra: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        // Opaque source pixels only (the palette has no partial alpha), so a
        // straight copy keeps the buffer premultiplied.
        self.px[i..i + 4].copy_from_slice(&bgra);
    }

    /// Blit a 16x16-ish palette-index grid at `dest`, each source pixel expanded
    /// to a `zoom` x `zoom` block. `flip_h` mirrors horizontally.
    pub fn blit_grid(
        &mut self,
        grid: &[Vec<u8>],
        zoom: i32,
        dest_x: i32,
        dest_y: i32,
        flip_h: bool,
    ) {
        let rows = grid.len() as i32;
        let cols = grid.first().map(|r| r.len()).unwrap_or(0) as i32;
        let grid_w = cols * zoom;

        for (r, row) in grid.iter().enumerate() {
            for (c, &idx) in row.iter().enumerate() {
                if idx == 0 {
                    continue;
                }
                let rgba = PALETTE[idx as usize % PALETTE.len()];
                let bgra = [rgba[2], rgba[1], rgba[0], rgba[3]];
                for zy in 0..zoom {
                    for zx in 0..zoom {
                        let local_x = c as i32 * zoom + zx;
                        let px = if flip_h {
                            dest_x + (grid_w - 1 - local_x)
                        } else {
                            dest_x + local_x
                        };
                        let py = dest_y + r as i32 * zoom + zy;
                        self.put(px, py, bgra);
                    }
                }
            }
        }
        let _ = rows;
    }
}

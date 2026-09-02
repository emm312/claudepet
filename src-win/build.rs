//! Bake `horse.jpg` and `mail.jpg` (repo root) into embedded sprites: decode,
//! key out the flat background, crop to content, nearest-neighbour downscale to
//! a small target, emit premultiplied BGRA (alpha 0 or 255) with an 8-byte
//! `[w:u32_le][h:u32_le]` header to `$OUT_DIR/<name>.bin`.
//!
//! `image` is a build-dependency only - none of it ends up in `claudepet.exe`.

use std::io::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../horse.jpg");
    println!("cargo:rerun-if-changed=../mail.jpg");
    println!("cargo:rerun-if-changed=build.rs");
    bake("../horse.jpg", "horse", 34, 24);
    bake("../mail.jpg", "mail", 20, 14);
}

fn bake(src: &str, name: &str, target_w: u32, target_h: u32) {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dst = Path::new(&out_dir).join(format!("{name}.bin"));

    let img = match image::open(src) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            // Don't fail the build if an asset is missing - emit a 1x1 empty
            // sprite so the crate still compiles.
            eprintln!("build.rs: could not open {src}: {e}; emitting empty {name}");
            let mut f = std::fs::File::create(&dst).unwrap();
            f.write_all(&1u32.to_le_bytes()).unwrap();
            f.write_all(&1u32.to_le_bytes()).unwrap();
            f.write_all(&[0, 0, 0, 0]).unwrap();
            return;
        }
    };

    let (w, h) = img.dimensions();
    let bg = *img.get_pixel(0, 0);
    let is_bg = |p: &image::Rgba<u8>| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
        d(p[0], bg[0]) + d(p[1], bg[1]) + d(p[2], bg[2]) < 60
    };

    // Content bounding box.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            if !is_bg(img.get_pixel(x, y)) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if max_x < min_x {
        min_x = 0;
        min_y = 0;
        max_x = w - 1;
        max_y = h - 1;
    }
    let (cw, ch) = (max_x - min_x + 1, max_y - min_y + 1);

    let cropped = image::imageops::crop_imm(&img, min_x, min_y, cw, ch).to_image();
    let scaled = image::imageops::resize(
        &cropped,
        target_w,
        target_h,
        image::imageops::FilterType::Nearest,
    );

    let mut bgra = Vec::with_capacity((target_w * target_h * 4) as usize);
    for p in scaled.pixels() {
        if is_bg(p) {
            bgra.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            bgra.extend_from_slice(&[p[2], p[1], p[0], 255]); // BGRA, opaque
        }
    }

    let mut f = std::fs::File::create(&dst).unwrap();
    f.write_all(&target_w.to_le_bytes()).unwrap();
    f.write_all(&target_h.to_le_bytes()).unwrap();
    f.write_all(&bgra).unwrap();
}

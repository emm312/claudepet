//! Courier props: the horse (express deliveries) and the mail (every delivery),
//! baked from `horse.jpg` / `mail.jpg` by `build.rs` into embedded BGRA.

use std::sync::LazyLock;

pub struct RgbaSprite {
    pub w: i32,
    pub h: i32,
    /// premultiplied BGRA, top-down, alpha is 0 or 255
    pub px: Vec<u8>,
}

fn parse(bin: &'static [u8]) -> RgbaSprite {
    let w = u32::from_le_bytes(bin[0..4].try_into().unwrap()) as i32;
    let h = u32::from_le_bytes(bin[4..8].try_into().unwrap()) as i32;
    RgbaSprite { w, h, px: bin[8..].to_vec() }
}

static HORSE_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/horse.bin"));
static MAIL_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mail.bin"));

pub static HORSE: LazyLock<RgbaSprite> = LazyLock::new(|| parse(HORSE_BIN));
pub static MAIL: LazyLock<RgbaSprite> = LazyLock::new(|| parse(MAIL_BIN));

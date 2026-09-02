//! The pet's pixel-grid sprites, authored directly in code (16x16, palette
//! indices, `.` = transparent). Mirrors `Sources/ClaudePet/Pet/Sprites.swift`.

use super::brain::AnimState;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Native grid size in "pixels" (before zoom).
pub const GRID_SIZE: usize = 16;

/// Palette index -> RGBA. Index 0 is always transparent. Values match the Swift
/// palette after its `UInt8(component * 255)` truncation:
///  1 = terracotta body, 2 = near-black eyes, 3 = angry red-tinted body,
///  4 = horse body, 5 = horse mane/tail/hooves (also the mail's flap line),
///  6 = mail envelope.
pub const PALETTE: [[u8; 4]; 7] = [
    [0, 0, 0, 0],        // 0 transparent
    [197, 116, 87, 255], // 1 body
    [19, 19, 19, 255],   // 2 eyes
    [206, 71, 59, 255],  // 3 angry tint
    [137, 92, 56, 255],  // 4 horse body
    [55, 34, 21, 255],   // 5 horse mane/tail/hooves; mail flap line
    [246, 239, 228, 255], // 6 mail envelope
];

pub struct SpriteClip {
    pub frames: Vec<Vec<Vec<u8>>>, // frame -> row -> palette index
    pub frame_duration: f64,
    /// Every ported clip loops; kept for parity with the Swift `SpriteClip`.
    #[allow(dead_code)]
    pub loops: bool,
}

fn parse(rows: &[&str]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|row| {
            row.chars()
                .map(|c| match c {
                    '1' => 1u8,
                    '2' => 2u8,
                    '3' => 3u8,
                    '4' => 4u8,
                    '5' => 5u8,
                    '6' => 6u8,
                    _ => 0u8,
                })
                .collect()
        })
        .collect()
}

/// Shifts every row horizontally by `amount` columns (positive = right),
/// dropping anything pushed past the edge rather than wrapping.
fn shift_columns(grid: &[Vec<u8>], amount: i32) -> Vec<Vec<u8>> {
    grid.iter()
        .map(|row| {
            let mut shifted = vec![0u8; row.len()];
            for (i, &value) in row.iter().enumerate() {
                let j = i as i32 + amount;
                if j >= 0 && (j as usize) < row.len() {
                    shifted[j as usize] = value;
                }
            }
            shifted
        })
        .collect()
}

const BLANK7: [&str; 7] = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

fn rows(extra: &[&str]) -> Vec<String> {
    BLANK7
        .iter()
        .chain(extra.iter())
        .map(|s| s.to_string())
        .collect()
}

fn grid(extra: &[&str]) -> Vec<Vec<u8>> {
    let owned = rows(extra);
    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    parse(&refs)
}

pub static CLIPS: LazyLock<HashMap<AnimState, SpriteClip>> = LazyLock::new(build_clips);

fn build_clips() -> HashMap<AnimState, SpriteClip> {
    let idle1 = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
    ]);
    let idle2 = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
    ]);
    let walk1 = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111.........",
    ]);
    let walk2 = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        ".........1111...",
    ]);
    let sleep1 = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        ".....111111.....",
        "................",
    ]);
    let sad1 = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "1111122112211111",
        "1111122112211111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
    ]);
    let angry1 = grid(&[
        "....33333333....",
        "...3333333333...",
        "3333333333333...",
        "3333322332233333",
        "...3322332233333",
        "...3333333333...",
        "...3333333333...",
        "...3333.333.....",
        "...3333.........",
    ]);
    let angry2 = grid(&[
        "....33333333....",
        "...3333333333...",
        "...3333333333...",
        "3333322332233333",
        "3333322332233333",
        "...3333333333...",
        "...3333333333...",
        ".....333.3333...",
        ".........3333...",
    ]);
    let fall1 = grid(&[
        "....11111111....",
        "1111111111111111",
        "1111221111221111",
        "...1221111221...",
        "...1111111111...",
        "...1111111111...",
        "...1111111111...",
        "...1111.111.....",
        "...1111.........",
    ]);
    let fall2 = grid(&[
        "....11111111....",
        "1111111111111111",
        "1111221111221111",
        "...1221111221...",
        "...1111111111...",
        "...1111111111...",
        "...1111111111...",
        ".....111.1111...",
        ".........1111...",
    ]);
    let dance_crouch = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "..1111....1111..",
        "..1111....1111..",
        "................",
    ]);
    let dance_jump = parse(&[
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "..11......11....",
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111111111111111",
        "...1111111111...",
        "...1111111111...",
        "....111111......",
        "....111111......",
    ]);
    let dragged1 = grid(&[
        "....11111111....",
        "...1111111111...",
        "1111122112211...",
        "1111122112211111",
        "...1111111111111",
        "...1111111111...",
        "...1111111111...",
        ".....111111.....",
        "................",
    ]);

    let dance_lean_left = shift_columns(&dance_crouch, -2);
    let dance_lean_right = shift_columns(&dance_crouch, 2);
    let dance_jump_left = shift_columns(&dance_jump, -1);
    let dance_jump_right = shift_columns(&dance_jump, 1);

    let mut m = HashMap::new();
    m.insert(
        AnimState::Idle,
        SpriteClip { frames: vec![idle1.clone(), idle1.clone(), idle1.clone(), idle2], frame_duration: 0.5, loops: true },
    );
    m.insert(
        AnimState::Walk,
        SpriteClip { frames: vec![walk1, walk2], frame_duration: 1.0 / 6.0, loops: true },
    );
    m.insert(
        AnimState::Sleep,
        SpriteClip { frames: vec![sleep1], frame_duration: 1.0, loops: true },
    );
    m.insert(
        AnimState::Sad,
        SpriteClip { frames: vec![sad1, idle1], frame_duration: 0.8, loops: true },
    );
    m.insert(
        AnimState::Dragged,
        SpriteClip { frames: vec![dragged1], frame_duration: 0.2, loops: true },
    );
    m.insert(
        AnimState::Angry,
        SpriteClip { frames: vec![angry1, angry2], frame_duration: 1.0 / 10.0, loops: true },
    );
    m.insert(
        AnimState::Fall,
        SpriteClip { frames: vec![fall1, fall2], frame_duration: 1.0 / 8.0, loops: true },
    );
    m.insert(
        AnimState::Dance,
        SpriteClip {
            frames: vec![
                dance_lean_left,
                dance_jump_left,
                dance_crouch.clone(),
                dance_jump.clone(),
                dance_lean_right,
                dance_jump_right,
                dance_crouch,
                dance_jump,
            ],
            frame_duration: 0.11,
            loops: true,
        },
    );
    m
}

/// The express-delivery horse, authored as a pixel grid in the same style as
/// the pet (flat color blocks, `.` = transparent) rather than baked from a
/// photo. Mirrors `Sources/ClaudePet/Pet/HorseSprite.swift`. Two frames give
/// it a simple gallop cycle: legs gathered under the body, then swept
/// front-forward/back-backward.
pub const HORSE_GRID_COLS: usize = 22;
/// Brisk gallop cadence - matches `HorseSprite.frameDuration` in Swift.
pub const HORSE_FRAME_DURATION: f64 = 1.0 / 12.0;

pub static HORSE_FRAMES: LazyLock<[Vec<Vec<u8>>; 2]> = LazyLock::new(|| {
    [
        // Frame 1: gallop's "collected" phase - all four legs gathered under the body.
        parse(&[
            "......................",
            "...............444....",
            ".............44444444.",
            "..........44444444544.",
            "......444444444444444.",
            "..4444444444444444444.",
            ".544444444444444444444",
            "....44...44.44...44...",
            "....55...55.55...55...",
            "......................",
            "......................",
            "......................",
        ]),
        // Frame 2: gallop's "extended" phase - front legs swept forward, back legs swept back.
        parse(&[
            "......................",
            "...............444....",
            ".............44444444.",
            "..........44444444544.",
            "......444444444444444.",
            "..4444444444444444444.",
            ".544444444444444444444",
            ".44....44.....44...44.",
            ".55....55.....55...55.",
            "......................",
            "......................",
            "......................",
        ]),
    ]
});

/// The mail parcel carried on every courier leg - a plain envelope with a
/// flap and a wax seal. Static (one frame): unlike the horse it doesn't need
/// a gait cycle. Mirrors `Sources/ClaudePet/Pet/MailSprite.swift`.
pub const MAIL_GRID_COLS: usize = 18;
pub const MAIL_GRID_ROWS: usize = 12;

pub static MAIL_GRID: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    parse(&[
        "..................",
        "..................",
        ".5666666666666665.",
        ".6566666666666656.",
        ".6656666666666566.",
        ".6665666336665666.",
        ".6666666666666666.",
        ".6666666666666666.",
        ".6666666666666666.",
        ".6666666666666666.",
        "..................",
        "..................",
    ])
});

#[cfg(test)]
mod tests {
    use super::*;

    // Ports Tests/ClaudePetTests/SpritesTests.swift.

    #[test]
    fn all_clips_have_at_least_one_frame() {
        for (state, clip) in CLIPS.iter() {
            assert!(!clip.frames.is_empty(), "{state:?} has no frames");
        }
    }

    #[test]
    fn every_frame_is_16x16() {
        for (state, clip) in CLIPS.iter() {
            for (fi, frame) in clip.frames.iter().enumerate() {
                assert_eq!(frame.len(), GRID_SIZE, "{state:?} frame {fi} wrong row count");
                for (ri, row) in frame.iter().enumerate() {
                    assert_eq!(row.len(), GRID_SIZE, "{state:?} frame {fi} row {ri} wrong width");
                }
            }
        }
    }

    #[test]
    fn frame_durations_are_positive() {
        for (_, clip) in CLIPS.iter() {
            assert!(clip.frame_duration > 0.0);
        }
    }

    #[test]
    fn every_anim_state_has_a_clip() {
        for state in [
            AnimState::Idle,
            AnimState::Walk,
            AnimState::Sleep,
            AnimState::Sad,
            AnimState::Dragged,
            AnimState::Angry,
            AnimState::Fall,
            AnimState::Dance,
        ] {
            assert!(CLIPS.contains_key(&state), "missing clip for {state:?}");
        }
    }

    #[test]
    fn shift_columns_drops_off_edge() {
        let g = vec![vec![1u8, 2, 3, 4]];
        assert_eq!(shift_columns(&g, 1), vec![vec![0u8, 1, 2, 3]]);
        assert_eq!(shift_columns(&g, -1), vec![vec![2u8, 3, 4, 0]]);
    }
}

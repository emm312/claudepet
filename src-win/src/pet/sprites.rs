//! The pet's pixel-grid sprites, authored directly in code (16x16, palette
//! indices, `.` = transparent). Mirrors `Sources/ClaudePet/Pet/Sprites.swift`.

use super::brain::AnimState;
use serde::{Deserialize, Serialize};
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
    let jump_crouch = grid(&[
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
    let jump_airborne = parse(&[
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
    let eat_open = grid(&[
        "....11111111....",
        "...1111111111...",
        "...1122112211...",
        "1111122112211111",
        "1111112222111111",
        "...1111111111...",
        "...1111111111...",
        "...1111..1111...",
        "...1111..1111...",
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

    let jump_lean_left = shift_columns(&jump_crouch, -2);
    let jump_lean_right = shift_columns(&jump_crouch, 2);
    let jump_airborne_left = shift_columns(&jump_airborne, -1);
    let jump_airborne_right = shift_columns(&jump_airborne, 1);

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
        SpriteClip { frames: vec![sad1, idle1.clone()], frame_duration: 0.8, loops: true },
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
        AnimState::Eat,
        SpriteClip { frames: vec![eat_open, idle1], frame_duration: 0.22, loops: true },
    );
    m.insert(
        AnimState::Jump,
        SpriteClip {
            frames: vec![
                jump_lean_left,
                jump_airborne_left,
                jump_crouch.clone(),
                jump_airborne.clone(),
                jump_lean_right,
                jump_airborne_right,
                jump_crouch,
                jump_airborne,
            ],
            frame_duration: 0.11,
            loops: true,
        },
    );
    m
}

/// A selectable alternate look for the pet. `Classic` is the original
/// terracotta critter; the rest are built by `SKINS` below. Persisted on
/// `PetState::skin` and carried on outbound `PetMessage`s (`sender_skin`) so a
/// peer's chosen skin renders correctly on the receiving screen too.
/// Mirrors `SkinId` in `Sources/ClaudePet/Pet/Skins.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkinId {
    Classic,
    Principal,
    Clown,
    Plant,
    SillyDuck,
    Goose,
    TennisBall,
}

impl SkinId {
    pub const ALL: [SkinId; 7] = [
        SkinId::Classic,
        SkinId::Principal,
        SkinId::Clown,
        SkinId::Plant,
        SkinId::SillyDuck,
        SkinId::Goose,
        SkinId::TennisBall,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            SkinId::Classic => "Classic",
            SkinId::Principal => "Principal",
            SkinId::Clown => "Clown",
            SkinId::Plant => "Potted Plant",
            SkinId::SillyDuck => "Silly Duck",
            SkinId::Goose => "Silly Goose",
            SkinId::TennisBall => "Tennis Ball",
        }
    }
}

impl Default for SkinId {
    fn default() -> Self {
        SkinId::Classic
    }
}

/// A cosmetic extra worn on top of whichever skin is active. Persisted on
/// `PetState::accessories` (any combination can be worn at once) and carried
/// on outbound `PetMessage`s the same way a skin choice is. Mirrors
/// `AccessoryId` in `Sources/ClaudePet/Pet/Skins.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessoryId {
    TopHat,
    Glasses,
    ChefHat,
}

impl AccessoryId {
    pub const ALL: [AccessoryId; 3] = [AccessoryId::TopHat, AccessoryId::Glasses, AccessoryId::ChefHat];

    pub fn display_name(self) -> &'static str {
        match self {
            AccessoryId::TopHat => "Top Hat",
            AccessoryId::Glasses => "Glasses",
            AccessoryId::ChefHat => "Chef Hat",
        }
    }
}

/// A full alternate look: its own palette plus a clip table shaped exactly
/// like `CLIPS`. Every non-classic skin is built by recoloring and stamping a
/// small "topper" patch onto the *same* rig `build_clips` already authored,
/// rather than hand-drawing a second full set of poses - so every skin
/// automatically covers every `AnimState` the classic pet does (full pose
/// parity), and a topper reads correctly in every pose because every classic
/// animation state keeps the head silhouette at the same rows/columns.
/// Mirrors `SkinDef` in `Sources/ClaudePet/Pet/Skins.swift`.
pub struct SkinDef {
    pub palette: Vec<[u8; 4]>,
    pub clips: HashMap<AnimState, SpriteClip>,
}

/// A small overlay stamped on top of whichever skin frame is currently
/// showing: its own full-size grid (mostly index 0/transparent) plus its own
/// tiny palette. Mirrors `AccessoryDef` in `Sources/ClaudePet/Pet/Skins.swift`.
pub struct AccessoryDef {
    pub palette: Vec<[u8; 4]>,
    pub grid: Vec<Vec<u8>>,
}

fn blank_grid() -> Vec<Vec<u8>> {
    vec![vec![0u8; GRID_SIZE]; GRID_SIZE]
}

fn remap(grid: &[Vec<u8>], table: &HashMap<u8, u8>) -> Vec<Vec<u8>> {
    grid.iter()
        .map(|row| row.iter().map(|v| *table.get(v).unwrap_or(v)).collect())
        .collect()
}

fn stamp(grid: Vec<Vec<u8>>, patch: &[(usize, usize, u8)]) -> Vec<Vec<u8>> {
    let mut g = grid;
    for &(r, c, v) in patch {
        if r < g.len() && c < g[r].len() {
            g[r][c] = v;
        }
    }
    g
}

/// Like `stamp`, but only recolors a cell that's already part of the body
/// (non-transparent) in this exact frame, rather than unconditionally
/// painting it - so a patch that targets a limb which is lifted/hidden in a
/// given pose (e.g. `WALK1`/`WALK2` alternate which foot is drawn at all)
/// tracks that pose instead of drawing over a gap that's supposed to be
/// empty. Mirrors `stampOverBody` in `Sources/ClaudePet/Pet/Skins.swift`.
fn stamp_over_body(grid: Vec<Vec<u8>>, patch: &[(usize, usize, u8)]) -> Vec<Vec<u8>> {
    let mut g = grid;
    for &(r, c, v) in patch {
        if r < g.len() && c < g[r].len() && g[r][c] != 0 {
            g[r][c] = v;
        }
    }
    g
}

/// Recolors every frame of the classic rig via `remap_table` (classic indices
/// 1/2/3 -> this skin's own indices), then stamps the same `topper` patch onto
/// every resulting frame. `overlay` (if non-empty) is applied afterward via
/// `stamp_over_body`, so it recolors existing body pixels only rather than
/// unconditionally painting over gaps `topper` would leave alone.
fn transform_clips(
    remap_table: &HashMap<u8, u8>,
    topper: &[(usize, usize, u8)],
    overlay: &[(usize, usize, u8)],
) -> HashMap<AnimState, SpriteClip> {
    CLIPS
        .iter()
        .map(|(state, clip)| {
            let frames = clip
                .frames
                .iter()
                .map(|frame| stamp_over_body(stamp(remap(frame, remap_table), topper), overlay))
                .collect();
            (
                *state,
                SpriteClip { frames, frame_duration: clip.frame_duration, loops: clip.loops },
            )
        })
        .collect()
}

fn identity_remap() -> HashMap<u8, u8> {
    [(1u8, 1u8), (2, 2), (3, 3)].into_iter().collect()
}

/// Like `transform_clips`, but recolors rows above `head_boundary_row`
/// through `head_map` and rows at/below it through `body_map`, instead of one
/// flat index remap - used to give a skin a bare head/face color that's
/// distinct from its neck-down body color (e.g. pale skin above a suit)
/// without repainting outside the classic rig's own silhouette (0 stays
/// transparent either way, so limb gaps in any given pose are untouched).
/// Mirrors `transformRowSplit` in `Sources/ClaudePet/Pet/Skins.swift`.
fn transform_clips_row_split(
    head_boundary_row: usize,
    head_map: &HashMap<u8, u8>,
    body_map: &HashMap<u8, u8>,
    topper: &[(usize, usize, u8)],
    overlay: &[(usize, usize, u8)],
) -> HashMap<AnimState, SpriteClip> {
    CLIPS
        .iter()
        .map(|(state, clip)| {
            let frames = clip
                .frames
                .iter()
                .map(|frame| {
                    let recolored: Vec<Vec<u8>> = frame
                        .iter()
                        .enumerate()
                        .map(|(r, row)| {
                            let map = if r < head_boundary_row { head_map } else { body_map };
                            row.iter().map(|v| if *v == 0 { 0 } else { *map.get(v).unwrap_or(v) }).collect()
                        })
                        .collect();
                    stamp_over_body(stamp(recolored, topper), overlay)
                })
                .collect();
            (
                *state,
                SpriteClip { frames, frame_duration: clip.frame_duration, loops: clip.loops },
            )
        })
        .collect()
}

pub static SKINS: LazyLock<HashMap<SkinId, SkinDef>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(SkinId::Classic, SkinDef { palette: PALETTE.to_vec(), clips: CLIPS.iter().map(|(s, c)| (*s, SpriteClip { frames: c.frames.clone(), frame_duration: c.frame_duration, loops: c.loops })).collect() });
    m.insert(SkinId::Principal, build_principal());
    m.insert(SkinId::Clown, build_clown());
    m.insert(SkinId::Plant, build_plant());
    m.insert(SkinId::SillyDuck, build_silly_duck());
    m.insert(SkinId::Goose, build_goose());
    m.insert(SkinId::TennisBall, build_tennis_ball());
    m
});

/// A bald, pale-skinned head above the neckline, a navy suit from the
/// shoulders down, a shirt collar, and a necktie - styled after a supplied
/// reference photo (bald, fair-skinned, dark suit, tie, round face), kept
/// generic rather than naming the real person. The classic rig's rows 7-10
/// are the head/face (including both eye rows) in every animation state, and
/// row 11 on is shoulders/arms/legs, so that's the split used to recolor
/// skin vs. suit without hand-authoring new poses. Mirrors `buildPrincipal`
/// in `Sources/ClaudePet/Pet/Skins.swift`.
fn build_principal() -> SkinDef {
    let palette = vec![
        [0, 0, 0, 0],
        [236, 200, 170, 255], // 1 pale skin (head)
        [19, 19, 19, 255],    // 2 eyes
        [216, 126, 109, 255], // 3 flushed skin (angry)
        [40, 50, 82, 255],    // 4 navy suit (body)
        [140, 27, 27, 255],   // 5 necktie
        [246, 240, 229, 255], // 6 shirt collar
    ];
    let mut topper = Vec::new();
    // Shirt collar peeking out beside the tie knot.
    topper.push((12, 5, 6u8));
    topper.push((12, 10, 6u8));
    // Necktie: a knot tapering to a single-column tie.
    topper.push((12, 7, 5u8));
    topper.push((12, 8, 5u8));
    topper.push((13, 7, 5u8));
    let head_map: HashMap<u8, u8> = [(1u8, 1u8), (2, 2), (3, 3)].into_iter().collect();
    let body_map: HashMap<u8, u8> = [(1u8, 4u8), (2, 2), (3, 4)].into_iter().collect();
    SkinDef { palette, clips: transform_clips_row_split(11, &head_map, &body_map, &topper, &[]) }
}

/// Bright jumpsuit, a big round red nose, a rainbow fringe of hair flush
/// against the hairline, poofs bulging out past either side of the head, and
/// a ruffled collar. Mirrors `buildClown` in `Sources/ClaudePet/Pet/Skins.swift`.
fn build_clown() -> SkinDef {
    let palette = vec![
        [0, 0, 0, 0],
        [246, 203, 52, 255], // 1 jumpsuit
        [19, 19, 19, 255],   // 2 eyes
        [216, 104, 52, 255], // 3 angry tint
        [218, 54, 52, 255],  // 4 nose + wig red
        [52, 126, 218, 255], // 5 wig blue
        [66, 170, 82, 255],  // 6 wig green
    ];
    let mut topper = Vec::new();
    let wig_colors = [4u8, 5u8, 6u8];
    // Rainbow wig fringe sitting directly on the hairline (row 6).
    for (i, c) in (3..=12).enumerate() {
        topper.push((6, c, wig_colors[i % wig_colors.len()]));
    }
    // Poofs bulging out past either side of the head.
    for (i, r) in (6..=8).enumerate() {
        let color = wig_colors[i % wig_colors.len()];
        topper.push((r, 2, color));
        topper.push((r, 3, color));
        topper.push((r, 12, color));
        topper.push((r, 13, color));
    }
    // Big round nose, centered between the eyes.
    topper.push((9, 7, 4u8));
    topper.push((9, 8, 4u8));
    topper.push((10, 7, 4u8));
    topper.push((10, 8, 4u8));
    // Ruffled collar on the uniform chest band every pose shares.
    let ruff = [4u8, 5u8, 6u8, 4u8, 5u8, 6u8, 4u8, 5u8];
    for (i, c) in (4..=11).enumerate() {
        topper.push((13, c, ruff[i]));
    }
    SkinDef { palette, clips: transform_clips(&identity_remap(), &topper, &[]) }
}

/// A terracotta pot with two green leaves sprouting from the top of the head,
/// in place of hair.
fn build_plant() -> SkinDef {
    let palette = vec![
        [0, 0, 0, 0],
        [197, 116, 87, 255], // 1 terracotta pot
        [19, 19, 19, 255],   // 2 eyes
        [206, 71, 59, 255],  // 3 angry tint
        [55, 132, 62, 255],  // 4 leaves
    ];
    let mut topper = Vec::new();
    for c in 4..=6 {
        topper.push((2, c, 4u8));
    }
    for c in 9..=11 {
        topper.push((2, c, 4u8));
    }
    for c in 3..=7 {
        topper.push((3, c, 4u8));
        topper.push((4, c, 4u8));
    }
    for c in 8..=12 {
        topper.push((3, c, 4u8));
        topper.push((4, c, 4u8));
    }
    for c in 4..=6 {
        topper.push((5, c, 4u8));
    }
    for c in 9..=11 {
        topper.push((5, c, 4u8));
    }
    SkinDef { palette, clips: transform_clips(&identity_remap(), &topper, &[]) }
}

/// A goofy white duck: a stubby orange beak recolored below the eyes and a
/// pair of big orange webbed feet, on an otherwise all-white body. The
/// beak/feet are an `overlay` (recolors existing body pixels only, see
/// `stamp_over_body`) rather than a `topper`, so a pose that hides a foot -
/// `WALK1`/`WALK2` alternate which one is drawn at all - hides the matching
/// orange foot too, instead of always showing both and masking the walk
/// cycle. Mirrors `buildSillyDuck` in `Sources/ClaudePet/Pet/Skins.swift`.
fn build_silly_duck() -> SkinDef {
    let palette = vec![
        [0, 0, 0, 0],
        [246, 246, 241, 255], // 1 white feathers
        [19, 19, 19, 255],    // 2 eyes
        [229, 140, 102, 255], // 3 flushed feathers (angry)
        [242, 153, 40, 255],  // 4 beak + feet
    ];
    let mut overlay = Vec::new();
    // Stubby beak below the eyes, tapering to a point.
    for c in 6..=9 {
        overlay.push((11, c, 4u8));
    }
    overlay.push((12, 7, 4u8));
    overlay.push((12, 8, 4u8));
    // Big webbed orange feet.
    for c in 3..=6 {
        overlay.push((15, c, 4u8));
    }
    for c in 9..=12 {
        overlay.push((15, c, 4u8));
    }
    SkinDef { palette, clips: transform_clips(&identity_remap(), &[], &overlay) }
}

/// A Canada-goose-styled look: a black head/neck with a white cheek
/// "chinstrap", a gray-brown body, and a dark-orange beak + feet. Unlike the
/// duck (a flat recolor), the head above the neckline gets a different color
/// than the body, the same row-split trick `build_principal` uses for its
/// suit. Mirrors `buildGoose` in `Sources/ClaudePet/Pet/Skins.swift`.
fn build_goose() -> SkinDef {
    let palette = vec![
        [0, 0, 0, 0],
        [27, 27, 27, 255],    // 1 black head
        [19, 19, 19, 255],    // 2 eyes
        [77, 27, 27, 255],    // 3 head angry flush
        [152, 147, 128, 255], // 4 gray-brown body
        [139, 102, 82, 255],  // 5 body angry flush
        [246, 246, 241, 255], // 6 white chinstrap
        [212, 123, 27, 255],  // 7 beak + feet
    ];
    let mut topper = Vec::new();
    // White cheek patches flanking the head, forming a chinstrap around the
    // black head without covering either eye.
    topper.push((9, 3, 6u8));
    topper.push((9, 12, 6u8));
    topper.push((10, 3, 6u8));
    topper.push((10, 12, 6u8));
    // Beak and feet are an `overlay` (recolors existing body pixels only,
    // see `build_silly_duck`) so `WALK1`/`WALK2` hiding a foot still hides
    // the matching orange foot instead of masking the walk cycle.
    let mut overlay = Vec::new();
    for c in 6..=9 {
        overlay.push((11, c, 7u8));
    }
    overlay.push((12, 7, 7u8));
    overlay.push((12, 8, 7u8));
    for c in 3..=6 {
        overlay.push((15, c, 7u8));
    }
    for c in 9..=12 {
        overlay.push((15, c, 7u8));
    }
    let head_map: HashMap<u8, u8> = [(1u8, 1u8), (2, 2), (3, 3)].into_iter().collect();
    let body_map: HashMap<u8, u8> = [(1u8, 4u8), (2, 2), (3, 5)].into_iter().collect();
    SkinDef { palette, clips: transform_clips_row_split(11, &head_map, &body_map, &topper, &overlay) }
}

/// An optic-yellow tennis ball: a flat recolor of the classic rig plus the
/// ball's two white seam curves facing each other - a "U" across the
/// forehead (rows 7-8) and an upturned "n" across the belly (rows 12-13).
/// Rows 9-11 are left alone because that's where the eyes land in some pose
/// or other (`fall*` puts them at cols 4/11, the jump frames shift them), and
/// no pose has an eye pixel on rows 7-8 or 12-13. The seam is an `overlay`
/// (recolors existing body pixels only, see `build_silly_duck`) so it never
/// paints into a gap a given pose leaves empty. Mirrors `buildTennisBall` in
/// `Sources/ClaudePet/Pet/Skins.swift`.
fn build_tennis_ball() -> SkinDef {
    let palette = vec![
        [0, 0, 0, 0],
        [214, 232, 62, 255],  // 1 optic yellow-green felt
        [19, 19, 19, 255],    // 2 eyes
        [222, 170, 50, 255],  // 3 angry tint
        [246, 246, 241, 255], // 4 white seam
    ];
    let overlay = [
        // Forehead "U".
        (7, 5, 4u8),
        (7, 10, 4u8),
        (8, 6, 4u8),
        (8, 7, 4u8),
        (8, 8, 4u8),
        (8, 9, 4u8),
        // Belly "n", mirroring it.
        (12, 6, 4u8),
        (12, 7, 4u8),
        (12, 8, 4u8),
        (12, 9, 4u8),
        (13, 5, 4u8),
        (13, 10, 4u8),
    ];
    SkinDef { palette, clips: transform_clips(&identity_remap(), &[], &overlay) }
}

pub static ACCESSORIES: LazyLock<HashMap<AccessoryId, AccessoryDef>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(AccessoryId::TopHat, build_top_hat());
    m.insert(AccessoryId::Glasses, build_glasses());
    m.insert(AccessoryId::ChefHat, build_chef_hat());
    m
});

/// The classic rig's head always starts at row 7 (`CLIPS`' frames all share
/// that top row), so the brim sits at row 6 - directly on the hairline, with
/// no gap - on every skin and every animation state, rather than floating
/// above a variable-height topper (hair/wig/leaves). Mirrors `buildTopHat` in
/// `Sources/ClaudePet/Pet/Skins.swift`.
fn build_top_hat() -> AccessoryDef {
    let mut g = blank_grid();
    for c in 7..=8 {
        g[2][c] = 1; // tapered crown top
    }
    for r in 3..=4 {
        for c in 6..=9 {
            g[r][c] = 1; // crown
        }
    }
    for c in 5..=10 {
        g[5][c] = 2; // hat band
    }
    for c in 3..=12 {
        g[6][c] = 1; // brim, flush on the hairline
    }
    AccessoryDef { palette: vec![[0, 0, 0, 0], [19, 19, 19, 255], [140, 27, 27, 255]], grid: g }
}

/// A puffy white chef's toque: three rounded lobes on top, a straight
/// pleated crown, and a band whose bottom row sits on row 6 - flush on the
/// hairline, the same anchoring `build_top_hat` uses. Mirrors `buildChefHat`
/// in `Sources/ClaudePet/Pet/Skins.swift`.
fn build_chef_hat() -> AccessoryDef {
    let g = parse(&[
        "................",
        "....11.11.11....", // lobe tops
        "...1111111111...",
        "...1112112111...", // lobe creases
        "....11111111....",
        "....12111121....", // pleats
        "....22222222....", // band, flush on the hairline
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
    ]);
    AccessoryDef { palette: vec![[0, 0, 0, 0], [250, 250, 246, 255], [214, 214, 208, 255]], grid: g }
}

fn build_glasses() -> AccessoryDef {
    let mut g = blank_grid();
    for c in 4..=11 {
        g[9][c] = 1;
    }
    g[8][4] = 1;
    g[8][11] = 1;
    AccessoryDef { palette: vec![[0, 0, 0, 0], [19, 19, 19, 255]], grid: g }
}

/// The express-delivery horse, authored as a pixel grid in the same style as
/// the pet (flat color blocks, `.` = transparent) rather than baked from a
/// photo. Faces right: ears and head top-right, a maned neck sloping down into
/// the barrel, rump and tail at the left, four legs below. Mirrors
/// `Sources/ClaudePet/Pet/HorseSprite.swift`. Two frames give it a gallop
/// cycle: legs gathered under the body, then front legs reaching forward and
/// hind legs driving back.
pub const HORSE_GRID_COLS: usize = 22;
/// Brisk gallop cadence - matches `HorseSprite.frameDuration` in Swift.
pub const HORSE_FRAME_DURATION: f64 = 1.0 / 12.0;

/// Rows 0-8 are the same in both frames - only the legs (rows 9-11) move.
const HORSE_TORSO: [&str; 9] = [
    "......................",
    ".................4.4..", // ears
    ".................4444.", // skull
    "................445444", // brow, eye, muzzle
    ".............555444444", // mane over jaw
    "...........554444444..", // mane over neck
    ".554444444444444444...", // tail root + back
    "554444444444444444....", // tail + barrel
    "55.44444444444444.....", // tail + belly
];

pub static HORSE_FRAMES: LazyLock<[Vec<Vec<u8>>; 2]> = LazyLock::new(|| {
    let frame = |legs: [&str; 3]| {
        let rows: Vec<&str> = HORSE_TORSO.iter().copied().chain(legs).collect();
        parse(&rows)
    };
    [
        // Frame 1: gallop's "collected" phase - all four legs gathered under the body.
        frame([
            "....44.44....44.44....",
            "....44.44....44.44....",
            "....55.55....55.55....",
        ]),
        // Frame 2: gallop's "extended" phase - hind legs driving back, fore legs
        // reaching forward. The upper row stays put so the legs read as swinging
        // from the body rather than sliding sideways as a whole.
        frame([
            "....44.44....44.44....",
            "..44.44........44.44..",
            ".55.55..........55.55.",
        ]),
    ]
});

/// What the courier rides on an express delivery. Persisted on
/// `PetState::mount` and carried on outbound `PetMessage`s (`sender_mount`) so
/// the receiving screen's visitor arrives on the sender's own mount, at that
/// mount's speed. Mirrors `MountId` in `Sources/ClaudePet/Pet/MountSprite.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MountId {
    #[default]
    BrownHorse,
    WhiteHorse,
    BlackHorse,
    Motorbike,
}

impl MountId {
    pub const ALL: [MountId; 4] = [MountId::BrownHorse, MountId::WhiteHorse, MountId::BlackHorse, MountId::Motorbike];

    pub fn display_name(self) -> &'static str {
        match self {
            MountId::BrownHorse => "Brown Horse",
            MountId::WhiteHorse => "White Horse",
            MountId::BlackHorse => "Black Horse",
            MountId::Motorbike => "Motorbike",
        }
    }
}

/// One mount's look and pace: its own palette, a 2-frame cycle (gallop or
/// wheel spin), and the express speed multiplier on top of the courier's base
/// speed. Every mount is 12 rows tall (widths vary - see `cols`) with its
/// riding surface around rows 4-6, so the one shared
/// `runtime::HORSE_RIDER_LIFT` keeps every mount's feet/wheels on the ground
/// and the rider seated. Mirrors `MountDef` in
/// `Sources/ClaudePet/Pet/MountSprite.swift`.
pub struct MountDef {
    pub palette: Vec<[u8; 4]>,
    pub frames: Vec<Vec<Vec<u8>>>,
    pub cols: usize,
    pub frame_duration: f64,
    /// Courier speed multiplier while riding - keep in sync with
    /// `MountId.speedMultiplier` in Swift.
    pub speed_mult: f64,
}

/// A horse recolor: the shared `HORSE_FRAMES` grids, with only palette
/// indices 4 (hide) and 5 (mane/tail/hooves) swapped.
fn horse_mount(body: [u8; 4], mane: [u8; 4]) -> MountDef {
    let mut palette = PALETTE.to_vec();
    palette[4] = body;
    palette[5] = mane;
    MountDef {
        palette,
        frames: HORSE_FRAMES.to_vec(),
        cols: HORSE_GRID_COLS,
        frame_duration: HORSE_FRAME_DURATION,
        speed_mult: 3.0,
    }
}

/// A red motorbike facing right like the horse: handlebar, fork and
/// headlight at the front, a seat behind the tank, a tail fender, and a
/// chrome engine and exhaust slung low between two wheels. It's 28 columns
/// wide (the horse is 22) because the rider covers the middle of it - the
/// horse gets away with less because its head and tail stick out - so the
/// front end, fender, and engine/exhaust need to clear the rider to read.
/// Its two frames differ only in the spokes (`+` then `x`), so the wheels
/// read as spinning. It's faster than any horse.
fn motorbike_mount() -> MountDef {
    const TOP: [&str; 7] = [
        "............................",
        "............................",
        ".....................13333..", // handlebar + grip
        "........................3...", // fork
        "........555555.222222...344.", // seat, tank, headlight
        ".2222222222222222222222.34..", // tail fender, body
        "222.....22222222222222..3...", // rear fender, lower body
    ];
    let frame = |wheels: [&str; 5]| {
        let rows: Vec<&str> = TOP.iter().copied().chain(wheels).collect();
        parse(&rows)
    };
    MountDef {
        palette: vec![
            [0, 0, 0, 0],
            [28, 28, 30, 255],    // 1 tyres
            [200, 48, 44, 255],   // 2 red body + tank
            [178, 182, 188, 255], // 3 chrome: fork, engine, exhaust, spokes
            [250, 214, 92, 255],  // 4 headlight
            [48, 40, 38, 255],    // 5 seat
        ],
        frames: vec![
            frame([
                "..111.....3333333......111..",
                ".1.3.1...333333333....1.3.1.", // engine, exhaust
                ".133313333333333......13331.",
                ".1.3.1.........33.....1.3.1.",
                "..111..................111..",
            ]),
            frame([
                "..111.....3333333......111..",
                ".13.31...333333333....13.31.",
                ".1.3.13333333333......1.3.1.",
                ".13.31.........33.....13.31.",
                "..111..................111..",
            ]),
        ],
        cols: 28,
        frame_duration: 1.0 / 16.0,
        speed_mult: 4.0,
    }
}

pub static MOUNTS: LazyLock<HashMap<MountId, MountDef>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(MountId::BrownHorse, horse_mount(PALETTE[4], PALETTE[5]));
    m.insert(MountId::WhiteHorse, horse_mount([232, 228, 220, 255], [150, 146, 140, 255]));
    // The mane is lighter than the body so it still reads against it.
    m.insert(MountId::BlackHorse, horse_mount([40, 36, 34, 255], [110, 104, 98, 255]));
    m.insert(MountId::Motorbike, motorbike_mount());
    m
});

impl MountId {
    pub fn def(self) -> &'static MountDef {
        &MOUNTS[&self]
    }
}

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
            AnimState::Eat,
            AnimState::Jump,
        ] {
            assert!(CLIPS.contains_key(&state), "missing clip for {state:?}");
        }
    }

    #[test]
    fn every_skin_covers_every_anim_state_with_full_size_frames() {
        let states: std::collections::HashSet<_> = CLIPS.keys().copied().collect();
        for id in SkinId::ALL {
            let skin = SKINS.get(&id).unwrap_or_else(|| panic!("no SkinDef registered for {id:?}"));
            let skin_states: std::collections::HashSet<_> = skin.clips.keys().copied().collect();
            assert_eq!(skin_states, states, "{id:?} doesn't cover every anim state");
            for (state, clip) in skin.clips.iter() {
                for (fi, frame) in clip.frames.iter().enumerate() {
                    assert_eq!(frame.len(), GRID_SIZE, "{id:?} {state:?} frame {fi} wrong row count");
                    for row in frame {
                        assert_eq!(row.len(), GRID_SIZE, "{id:?} {state:?} frame {fi} wrong row width");
                    }
                }
            }
        }
    }

    #[test]
    fn every_accessory_grid_is_full_size() {
        for id in AccessoryId::ALL {
            let accessory = ACCESSORIES.get(&id).unwrap_or_else(|| panic!("no AccessoryDef registered for {id:?}"));
            assert_eq!(accessory.grid.len(), GRID_SIZE);
            for row in &accessory.grid {
                assert_eq!(row.len(), GRID_SIZE);
            }
        }
    }

    #[test]
    fn every_mount_has_full_size_frames_and_a_positive_speed() {
        for id in MountId::ALL {
            let mount = MOUNTS.get(&id).unwrap_or_else(|| panic!("no MountDef registered for {id:?}"));
            assert_eq!(mount.frames.len(), 2, "{id:?} should have a 2-frame cycle");
            for frame in &mount.frames {
                assert_eq!(frame.len(), HORSE_FRAMES[0].len(), "{id:?} should match the horse's height");
                for row in frame {
                    assert_eq!(row.len(), mount.cols);
                    assert!(row.iter().all(|&v| (v as usize) < mount.palette.len()), "{id:?} uses an index past its palette");
                }
            }
            assert!(mount.frame_duration > 0.0);
            assert!(mount.speed_mult > 1.0);
        }
        assert!(MountId::Motorbike.def().speed_mult > MountId::BrownHorse.def().speed_mult);
    }

    #[test]
    fn tennis_ball_seam_never_covers_an_eye() {
        let ball = &SKINS[&SkinId::TennisBall];
        for (state, clip) in CLIPS.iter() {
            for (fi, frame) in clip.frames.iter().enumerate() {
                let skinned = &ball.clips[state].frames[fi];
                for (r, row) in frame.iter().enumerate() {
                    for (c, &v) in row.iter().enumerate() {
                        if v == 2 {
                            assert_eq!(skinned[r][c], 2, "{state:?} frame {fi}: seam covers the eye at ({r},{c})");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn shift_columns_drops_off_edge() {
        let g = vec![vec![1u8, 2, 3, 4]];
        assert_eq!(shift_columns(&g, 1), vec![vec![0u8, 1, 2, 3]]);
        assert_eq!(shift_columns(&g, -1), vec![vec![2u8, 3, 4, 0]]);
    }
}

import AppKit

/// A selectable alternate look for the pet: `.classic` is the original
/// terracotta critter; `.principal`, `.clown`, and `.plant` are built by
/// `Skins.all` below. Persisted on `PetState.skinId` and carried on outbound
/// `PetMessage`s so a peer's chosen skin renders correctly on the receiving
/// screen too (see `PetMessage.senderSkin`).
enum SkinId: String, Codable, CaseIterable {
    case classic, principal, clown, plant

    var displayName: String {
        switch self {
        case .classic: return "Classic"
        case .principal: return "Principal"
        case .clown: return "Clown"
        case .plant: return "Potted Plant"
        }
    }
}

/// A cosmetic extra worn on top of whichever skin is active. Persisted on
/// `PetState.accessoryIds` (a set - any combination can be worn at once) and
/// carried on outbound `PetMessage`s the same way a skin choice is.
enum AccessoryId: String, Codable, CaseIterable {
    case topHat, glasses

    var displayName: String {
        switch self {
        case .topHat: return "Top Hat"
        case .glasses: return "Glasses"
        }
    }
}

/// A full alternate look: its own palette plus a clip table shaped exactly
/// like `PetSprites.clips`. Every non-classic skin is built by recoloring and
/// stamping a small "topper" patch onto the *same* rig `PetSprites` already
/// authored, rather than hand-drawing a second full set of poses - so every
/// skin automatically covers every `PetMood.AnimState` the classic pet does
/// (full pose parity), and a topper reads correctly in every pose because
/// every classic animation state keeps the head silhouette at the same
/// rows/columns.
struct SkinDef {
    let palette: [UInt8: NSColor]
    let clips: [PetMood.AnimState: SpriteClip]
}

/// A small overlay stamped on top of whichever skin frame is currently
/// showing. Represented as its own full-size grid (mostly transparent) plus
/// its own tiny palette, composited over the skin's rendered frame at draw
/// time (`PixelArtRenderer.renderComposite`) rather than merged into the
/// skin's own palette-indexed grid, since skin and accessory grids are
/// authored independently and their palette indices aren't related.
struct AccessoryDef {
    let palette: [UInt8: NSColor]
    let grid: [[UInt8]]
}

enum Skins {
    static let all: [SkinId: SkinDef] = [
        .classic: SkinDef(palette: Palette.colors, clips: PetSprites.clips),
        .principal: buildPrincipal(),
        .clown: buildClown(),
        .plant: buildPlant(),
    ]

    private static func blankGrid() -> [[UInt8]] {
        Array(repeating: Array(repeating: UInt8(0), count: Int(PetSprites.gridSize.width)), count: Int(PetSprites.gridSize.height))
    }

    private static func remap(_ grid: [[UInt8]], _ table: [UInt8: UInt8]) -> [[UInt8]] {
        grid.map { row in row.map { table[$0] ?? $0 } }
    }

    private static func stamp(_ grid: [[UInt8]], _ patch: [(row: Int, col: Int, value: UInt8)]) -> [[UInt8]] {
        var g = grid
        for p in patch where p.row >= 0 && p.row < g.count && p.col >= 0 && p.col < g[p.row].count {
            g[p.row][p.col] = p.value
        }
        return g
    }

    /// Recolors every frame of the classic rig via `remapTable` (classic
    /// indices 1/2/3 -> this skin's own indices), then stamps the same
    /// `topper` patch onto every resulting frame.
    private static func transform(remapTable: [UInt8: UInt8], topper: [(row: Int, col: Int, value: UInt8)]) -> [PetMood.AnimState: SpriteClip] {
        var out: [PetMood.AnimState: SpriteClip] = [:]
        for (state, clip) in PetSprites.clips {
            let frames = clip.frames.map { stamp(remap($0, remapTable), topper) }
            out[state] = SpriteClip(frames: frames, frameDuration: clip.frameDuration, loops: clip.loops)
        }
        return out
    }

    /// Like `transform`, but recolors rows above `headBoundaryRow` through
    /// `headMap` and rows at/below it through `bodyMap`, instead of one flat
    /// index remap - used to give a skin a bare head/face color that's
    /// distinct from its neck-down body color (e.g. pale skin above a suit)
    /// without repainting outside the classic rig's own silhouette (0 stays
    /// transparent either way, so limb gaps in any given pose are untouched).
    private static func transformRowSplit(
        headBoundaryRow: Int,
        headMap: [UInt8: UInt8],
        bodyMap: [UInt8: UInt8],
        topper: [(row: Int, col: Int, value: UInt8)]
    ) -> [PetMood.AnimState: SpriteClip] {
        var out: [PetMood.AnimState: SpriteClip] = [:]
        for (state, clip) in PetSprites.clips {
            let frames = clip.frames.map { frame -> [[UInt8]] in
                let recolored = frame.enumerated().map { r, row -> [UInt8] in
                    row.map { value -> UInt8 in
                        guard value != 0 else { return 0 }
                        let map = r < headBoundaryRow ? headMap : bodyMap
                        return map[value] ?? value
                    }
                }
                return stamp(recolored, topper)
            }
            out[state] = SpriteClip(frames: frames, frameDuration: clip.frameDuration, loops: clip.loops)
        }
        return out
    }

    /// A bald, pale-skinned head above the neckline, a navy suit from the
    /// shoulders down, a shirt collar, and a necktie - styled after a
    /// supplied reference photo (bald, fair-skinned, dark suit, tie, round
    /// face), kept generic rather than naming the real person. The classic
    /// rig's rows 7-10 are the head/face (including both eye rows) in every
    /// animation state, and row 11 on is shoulders/arms/legs, so that's the
    /// split used to recolor skin vs. suit without hand-authoring new poses.
    private static func buildPrincipal() -> SkinDef {
        let palette: [UInt8: NSColor] = [
            1: NSColor(calibratedRed: 0.925, green: 0.784, blue: 0.667, alpha: 1), // pale skin (head)
            2: NSColor(calibratedRed: 0.078, green: 0.078, blue: 0.078, alpha: 1), // eyes
            3: NSColor(calibratedRed: 0.847, green: 0.494, blue: 0.427, alpha: 1), // flushed skin (angry)
            4: NSColor(calibratedRed: 0.157, green: 0.196, blue: 0.322, alpha: 1), // navy suit (body)
            5: NSColor(calibratedRed: 0.55, green: 0.106, blue: 0.106, alpha: 1),  // necktie
            6: NSColor(calibratedRed: 0.965, green: 0.941, blue: 0.898, alpha: 1), // shirt collar
        ]
        var topper: [(row: Int, col: Int, value: UInt8)] = []
        // Shirt collar peeking out beside the tie knot.
        topper.append((12, 5, 6))
        topper.append((12, 10, 6))
        // Necktie: a knot tapering to a single-column tie.
        topper.append((12, 7, 5))
        topper.append((12, 8, 5))
        topper.append((13, 7, 5))
        return SkinDef(
            palette: palette,
            clips: transformRowSplit(
                headBoundaryRow: 11,
                headMap: [1: 1, 2: 2, 3: 3],
                bodyMap: [1: 4, 2: 2, 3: 4],
                topper: topper
            )
        )
    }

    /// Bright jumpsuit, a big round red nose, a rainbow fringe of hair flush
    /// against the hairline, poofs bulging out past either side of the head,
    /// and a ruffled collar.
    private static func buildClown() -> SkinDef {
        let palette: [UInt8: NSColor] = [
            1: NSColor(calibratedRed: 0.965, green: 0.796, blue: 0.204, alpha: 1), // jumpsuit
            2: NSColor(calibratedRed: 0.078, green: 0.078, blue: 0.078, alpha: 1), // eyes
            3: NSColor(calibratedRed: 0.847, green: 0.408, blue: 0.204, alpha: 1), // angry tint
            4: NSColor(calibratedRed: 0.855, green: 0.212, blue: 0.204, alpha: 1), // nose + wig red
            5: NSColor(calibratedRed: 0.204, green: 0.494, blue: 0.855, alpha: 1), // wig blue
            6: NSColor(calibratedRed: 0.259, green: 0.667, blue: 0.322, alpha: 1), // wig green
        ]
        var topper: [(row: Int, col: Int, value: UInt8)] = []
        // Rainbow wig fringe sitting directly on the hairline (row 6).
        let wigColors: [UInt8] = [4, 5, 6]
        for (i, c) in (3...12).enumerated() {
            topper.append((6, c, wigColors[i % wigColors.count]))
        }
        // Poofs bulging out past either side of the head.
        for (i, r) in (6...8).enumerated() {
            let color = wigColors[i % wigColors.count]
            topper.append((r, 2, color))
            topper.append((r, 3, color))
            topper.append((r, 12, color))
            topper.append((r, 13, color))
        }
        // Big round nose, centered between the eyes.
        topper.append((9, 7, 4))
        topper.append((9, 8, 4))
        topper.append((10, 7, 4))
        topper.append((10, 8, 4))
        // Ruffled collar on the uniform chest band every pose shares.
        let ruff: [UInt8] = [4, 5, 6, 4, 5, 6, 4, 5]
        for (i, c) in (4...11).enumerated() {
            topper.append((13, c, ruff[i]))
        }
        return SkinDef(palette: palette, clips: transform(remapTable: [1: 1, 2: 2, 3: 3], topper: topper))
    }

    /// A terracotta pot with two green leaves sprouting from the top of the
    /// head, in place of hair.
    private static func buildPlant() -> SkinDef {
        let palette: [UInt8: NSColor] = [
            1: NSColor(calibratedRed: 0.776, green: 0.455, blue: 0.345, alpha: 1), // terracotta pot
            2: NSColor(calibratedRed: 0.078, green: 0.078, blue: 0.078, alpha: 1), // eyes
            3: NSColor(calibratedRed: 0.808, green: 0.282, blue: 0.235, alpha: 1), // angry tint
            4: NSColor(calibratedRed: 0.216, green: 0.518, blue: 0.243, alpha: 1), // leaves
        ]
        var topper: [(row: Int, col: Int, value: UInt8)] = []
        for c in 4...6 { topper.append((2, c, 4)) }
        for c in 9...11 { topper.append((2, c, 4)) }
        for c in 3...7 {
            topper.append((3, c, 4))
            topper.append((4, c, 4))
        }
        for c in 8...12 {
            topper.append((3, c, 4))
            topper.append((4, c, 4))
        }
        for c in 4...6 { topper.append((5, c, 4)) }
        for c in 9...11 { topper.append((5, c, 4)) }
        return SkinDef(palette: palette, clips: transform(remapTable: [1: 1, 2: 2, 3: 3], topper: topper))
    }
}

enum Accessories {
    static let all: [AccessoryId: AccessoryDef] = [
        .topHat: buildTopHat(),
        .glasses: buildGlasses(),
    ]

    private static func blankGrid() -> [[UInt8]] {
        Array(repeating: Array(repeating: UInt8(0), count: Int(PetSprites.gridSize.width)), count: Int(PetSprites.gridSize.height))
    }

    /// The classic rig's head always starts at row 7 (`PetSprites`' frames all
    /// share that top row), so the brim sits at row 6 - directly on the
    /// hairline, with no gap - on every skin and every animation state,
    /// rather than floating above a variable-height topper (hair/wig/leaves).
    private static func buildTopHat() -> AccessoryDef {
        var g = blankGrid()
        for c in 7...8 { g[2][c] = 1 }               // tapered crown top
        for r in 3...4 { for c in 6...9 { g[r][c] = 1 } } // crown
        for c in 5...10 { g[5][c] = 2 }               // hat band
        for c in 3...12 { g[6][c] = 1 }                // brim, flush on the hairline
        return AccessoryDef(
            palette: [
                1: NSColor(calibratedRed: 0.078, green: 0.078, blue: 0.078, alpha: 1), // black felt
                2: NSColor(calibratedRed: 0.55, green: 0.106, blue: 0.106, alpha: 1),  // red band
            ],
            grid: g
        )
    }

    private static func buildGlasses() -> AccessoryDef {
        var g = blankGrid()
        for c in 4...11 { g[9][c] = 1 }
        g[8][4] = 1; g[8][11] = 1
        return AccessoryDef(
            palette: [1: NSColor(calibratedRed: 0.078, green: 0.078, blue: 0.078, alpha: 1)],
            grid: g
        )
    }
}
